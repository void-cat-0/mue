//! Headless Scene scheduling and raw-input regressions; all nodes have no paint.

use super::*;
use crate::{
    SceneEvent,
    event::pointer::{ButtonState, PointerAction},
    input::InputEvent,
    layout::use_layout,
    node::{Node, map_keyed},
    scene::RootInput,
    style::{FontSize, Size, Style, Styleable},
};
use miniquad::{MouseButton, TouchPhase};
use mue_core::{
    Disposable,
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};
use std::cell::{Cell, RefCell};

type Log = Rc<RefCell<Vec<(&'static str, &'static str)>>>;
type RawInput = Rc<RefCell<Vec<InputEvent>>>;
type ContextHook = Box<dyn FnMut(&mut SceneContext<'_>) -> Result<()>>;
type InputHook = Box<dyn FnMut(&InputBatch, &mut SceneContext<'_>) -> Result<()>>;

fn frame() -> SceneFrame {
    SceneFrame {
        dt: 0.125,
        time: 42.,
        viewport: Rect::new(0., 0., 320., 240.),
    }
}

fn leaf(style: Style) -> Node {
    Node::leaf(style, |style| {
        use_layout(style);
    })
}

fn full_leaf(style: Style) -> Node {
    leaf(style.width(Size::Percent(100.)).height(Size::Percent(100.)))
}

fn bounds(node: NodeRef) -> Rect {
    Runtime::with(|rt| rt.node(node.id).layout.rect.get())
}

fn alive(node: NodeRef) -> bool {
    Runtime::with(|rt| rt.nodes.borrow().contains_key(node.id))
}

fn note(log: &Log, name: &'static str, phase: &'static str) {
    log.borrow_mut().push((name, phase));
}

fn count(log: &Log, name: &'static str, phase: &'static str) -> usize {
    log.borrow()
        .iter()
        .filter(|entry| **entry == (name, phase))
        .count()
}

fn app(scene: impl Scene + 'static) -> App {
    App {
        scenes: vec![SceneEntry::new(Box::new(scene))],
        state: AppState::Active,
        host_paused: false,
        command: None,
        displayed: Vec::new(),
        pointer_manager: PointerManager::headless(),
    }
}

fn run(app: &mut App, input: InputBatch, globals: &Cell<usize>) -> Result<()> {
    app.run_checked_frame(
        input,
        frame(),
        || {},
        |_, actual| {
            assert_eq!(actual.dt, frame().dt);
            assert_eq!(actual.time, frame().time);
            assert_eq!(actual.viewport, frame().viewport);
            globals.set(globals.get() + 1);
        },
    )
}

fn empty() -> InputBatch {
    InputBatch::recorded(|_| {})
}

fn tap(id: u64, x: f32, y: f32) -> InputBatch {
    InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, id, x, y, 12.25);
        events.touch_event(TouchPhase::Ended, id, x, y, 12.5);
    })
}

fn record_input(raw: RawInput) -> InputHook {
    Box::new(move |input, _| {
        raw.borrow_mut().extend_from_slice(input.events());
        Ok(())
    })
}

fn assert_raw_events(actual: &[InputEvent], expected: &[InputEvent]) {
    // InputEvent deliberately has no PartialEq; its Debug representation includes
    // every raw field, including round-trippable float positions and timestamps.
    assert_eq!(
        format!("{actual:?}"),
        format!("{expected:?}"),
        "Scene must receive every raw event in its original order"
    );
}

fn record_pointer(
    log: Log,
    name: &'static str,
) -> impl FnMut(&crate::event::pointer::PointerEvent) {
    move |event| {
        note(
            &log,
            name,
            match event.action() {
                PointerAction::Down => "down",
                PointerAction::Move => "move",
                PointerAction::Up => "up",
                PointerAction::Cancel => "cancel",
            },
        );
    }
}

struct Probe {
    name: &'static str,
    log: Log,
    _nodes: Vec<Node>,
    roots: Rc<RefCell<Vec<SceneRoot>>>,
    draws: Rc<RefCell<Vec<NodeRef>>>,
    enter: Option<ContextHook>,
    input: Option<InputHook>,
    fail: Option<&'static str>,
    resource: Option<Rc<()>>,
}

impl Probe {
    fn new(name: &'static str, log: Log, nodes: Vec<Node>, roots: Vec<SceneRoot>) -> Self {
        let draws = roots.iter().map(|root| root.node).collect();
        Self {
            name,
            log,
            _nodes: nodes,
            roots: Rc::new(RefCell::new(roots)),
            draws: Rc::new(RefCell::new(draws)),
            enter: None,
            input: None,
            fail: None,
            resource: None,
        }
    }

    fn phase(&self, phase: &'static str) -> Result<()> {
        note(&self.log, self.name, phase);
        ensure!(self.fail != Some(phase), "deliberate {phase} failure");
        Ok(())
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        note(&self.log, self.name, "drop");
    }
}

impl Scene for Probe {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => {
                self.phase("enter")?;
                if let Some(hook) = &mut self.enter {
                    hook(ctx)?;
                }
                Ok(())
            }
            SceneEvent::Input(input) => {
                self.phase("input")?;
                if let Some(hook) = &mut self.input {
                    hook(input, ctx)?;
                }
                Ok(())
            }
            SceneEvent::Pause => self.phase("pause"),
            SceneEvent::Resume => self.phase("resume"),
            SceneEvent::Returned(_) => Ok(()),
        }
    }

    fn update(&mut self, _: &mut SceneContext<'_>) -> Result<()> {
        self.phase("update")
    }

    fn layout(&mut self, _: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        self.phase("layout")?;
        roots.extend(self.roots.borrow().iter().copied());
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        note(&self.log, self.name, "render");
        for &node in self.draws.borrow().iter() {
            render.draw(node)?;
            // Represents custom Scene drawing between complete UI root draws.
            note(&self.log, self.name, "custom");
        }
        ensure!(self.fail != Some("render"), "deliberate render failure");
        Ok(())
    }
}

#[test]
fn input_only_event_preserves_the_borrowed_batch_and_ignores_other_notifications() {
    struct InputOnly {
        raw: RawInput,
        calls: Rc<Cell<usize>>,
        original_events: *const InputEvent,
    }
    impl Scene for InputOnly {
        fn event(&mut self, event: SceneEvent<'_>, _: &mut SceneContext<'_>) -> Result<()> {
            if let SceneEvent::Input(input) = event {
                self.calls.set(self.calls.get() + 1);
                if !input.events().is_empty() {
                    assert_eq!(
                        input.events().as_ptr(),
                        self.original_events,
                        "Input borrows the original batch storage, not a filtered copy"
                    );
                }
                self.raw.borrow_mut().extend_from_slice(input.events());
            }
            Ok(())
        }
    }

    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 9, 30.125, 40.25, 12345.678901234);
        events.char_event('界', Default::default(), false);
        events.touch_event(TouchPhase::Moved, 9, 31.5, 42.75, 12346.987654321);
        events.touch_event(TouchPhase::Ended, 9, 32.25, 43.5, 12347.111111111);
    });
    let expected = input.events().to_vec();
    let raw = RawInput::default();
    let calls = Rc::new(Cell::new(0));
    let mut app = app(InputOnly {
        raw: raw.clone(),
        calls: calls.clone(),
        original_events: input.events().as_ptr(),
    });
    let globals = Cell::new(0);
    run(&mut app, input, &globals).unwrap();
    assert_eq!(calls.get(), 1, "Enter must not run the Input branch");
    assert_raw_events(&raw.borrow(), &expected);

    app.push_scene(Probe::new("child", Log::default(), Vec::new(), Vec::new()));
    run(&mut app, empty(), &globals).unwrap();
    assert_eq!(calls.get(), 2, "Pause must not run the Input branch");
    let result = Rc::new(());
    let weak_result = Rc::downgrade(&result);
    app.pop_scene_with_result(result);
    run(&mut app, empty(), &globals).unwrap();
    assert_eq!(calls.get(), 2, "Returned and Resume are not Input");
    assert!(
        weak_result.upgrade().is_none(),
        "unmatched owned returns drop"
    );
    run(&mut app, empty(), &globals).unwrap();
    assert_eq!(calls.get(), 3);
    assert_raw_events(&raw.borrow(), &expected);
}

#[test]
fn one_scene_runs_the_frame_phases_once_and_renders_node_update_feedback() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let log = Log::default();
        let width = signal(Size::Px(40.));
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let root = Node::leaf(Style::new().width(width).height(20.), {
            let log = log.clone();
            let rendered = rendered.clone();
            move |style| {
                let layout = use_layout(style);
                style
                    .on_pointer_event
                    .append(record_pointer(log.clone(), "node"));
                style.on_update.append({
                    let log = log.clone();
                    move |dt| {
                        assert_eq!(*dt, frame().dt);
                        note(&log, "node", "update");
                        width.set(Size::Px(80.));
                    }
                });
                style.on_render.append(move |_| {
                    note(&log, "node", "render");
                    rendered.borrow_mut().push(layout.rect.get());
                });
            }
        });
        let root_ref = *root;
        let scene = Probe::new(
            "scene",
            log.clone(),
            vec![root],
            vec![SceneRoot::new(root_ref, Rect::new(20., 30., 100., 60.))],
        );
        let mut app = app(scene);
        let globals = Cell::new(0);
        app.run_checked_frame(
            empty(),
            frame(),
            || note(&log, "host", "update"),
            |_, _| {
                globals.set(globals.get() + 1);
                note(&log, "host", "globals");
            },
        )
        .unwrap();
        assert_eq!(
            *log.borrow(),
            vec![
                ("scene", "enter"),
                ("scene", "input"),
                ("host", "globals"),
                ("host", "update"),
                ("scene", "update"),
                ("scene", "layout"),
                ("node", "update"),
                ("scene", "layout"),
                ("scene", "render"),
                ("node", "render"),
                ("scene", "custom"),
            ]
        );
        assert_eq!(*rendered.borrow(), vec![Rect::new(20., 30., 80., 20.)]);
        let second_frame = log.borrow().len();
        app.run_checked_frame(
            tap(1, 30., 40.),
            frame(),
            || note(&log, "host", "update"),
            |_, _| {
                globals.set(globals.get() + 1);
                note(&log, "host", "globals");
            },
        )
        .unwrap();
        assert_eq!(
            &log.borrow()[second_frame..],
            &[
                ("node", "down"),
                ("node", "up"),
                ("scene", "input"),
                ("host", "globals"),
                ("host", "update"),
                ("scene", "update"),
                ("scene", "layout"),
                ("node", "update"),
                ("scene", "layout"),
                ("scene", "render"),
                ("node", "render"),
                ("scene", "custom"),
            ],
            "UI dispatch precedes raw Scene input, globals, and frame updates"
        );
        assert_eq!(count(&log, "scene", "enter"), 1);
        assert_eq!(count(&log, "node", "update"), 2);
        assert_eq!(globals.get(), 2);
    });
}

#[test]
fn actual_draw_order_and_omitted_roots_control_the_next_batch() {
    let log = Log::default();
    let a = full_leaf(
        Style::new()
            .on_pointer_event(record_pointer(log.clone(), "a"))
            .on_render({
                let log = log.clone();
                move |_| note(&log, "a", "render")
            }),
    );
    let b = full_leaf(
        Style::new()
            .on_pointer_event(record_pointer(log.clone(), "b"))
            .on_render({
                let log = log.clone();
                move |_| note(&log, "b", "render")
            }),
    );
    let (a_ref, b_ref) = (*a, *b);
    let a_bounds = Rect::new(20., 30., 100., 80.);
    let b_bounds = Rect::new(60., 50., 100., 80.);
    let mut scene = Probe::new(
        "scene",
        log.clone(),
        vec![a, b],
        vec![
            SceneRoot::new(a_ref, a_bounds),
            SceneRoot::new(b_ref, b_bounds),
        ],
    );
    *scene.draws.borrow_mut() = vec![b_ref, a_ref];
    let draws = scene.draws.clone();
    let raw = RawInput::default();
    scene.input = Some(record_input(raw.clone()));
    let mut app = app(scene);
    let globals = Cell::new(0);
    run(&mut app, empty(), &globals).unwrap();
    assert_eq!(bounds(a_ref), a_bounds);
    assert_eq!(bounds(b_ref), b_bounds);
    let render_order: Vec<_> = log
        .borrow()
        .iter()
        .copied()
        .filter(|(_, phase)| *phase == "render" || *phase == "custom")
        .collect();
    assert_eq!(
        render_order,
        vec![
            ("scene", "render"),
            ("b", "render"),
            ("scene", "custom"),
            ("a", "render"),
            ("scene", "custom")
        ]
    );
    assert!(!app.hit_test_ui(Vector::new(5., 5.)));
    assert!(app.hit_test_ui(Vector::new(70., 60.)));

    *draws.borrow_mut() = vec![a_ref, b_ref];
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 1, 70., 60., 7.125);
        events.touch_event(TouchPhase::Moved, 1, 71.5, 61.25, 7.1875);
        events.touch_event(TouchPhase::Ended, 1, 72., 62., 7.25);
        events.char_event('x', Default::default(), false);
        events.touch_event(TouchPhase::Started, 2, 250., 180., 9.125);
        events.touch_event(TouchPhase::Moved, 2, 250.5, 179.75, 9.1875);
        events.touch_event(TouchPhase::Ended, 2, 251., 181., 9.25);
    });
    let mut expected = input.events().to_vec();
    run(&mut app, input, &globals).unwrap();
    assert_eq!(count(&log, "a", "down"), 1);
    assert_eq!(count(&log, "a", "move"), 1);
    assert_eq!(count(&log, "a", "up"), 1);
    assert_eq!(count(&log, "b", "down"), 0);
    assert_raw_events(&raw.borrow(), &expected);

    *draws.borrow_mut() = vec![a_ref];
    let input = tap(3, 70., 60.);
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_eq!(
        count(&log, "b", "down"),
        1,
        "this batch uses the previous drawing"
    );
    assert!(
        !app.hit_test_ui(Vector::new(150., 100.)),
        "an omitted root is no longer displayed"
    );
    let input = tap(4, 70., 60.);
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_eq!(count(&log, "a", "down"), 2);
    assert_eq!(count(&log, "b", "down"), 1);
    assert_raw_events(&raw.borrow(), &expected);
    assert_eq!(
        globals.get(),
        4,
        "global work runs once, regardless of root count"
    );
}

#[test]
fn scene_and_ui_callbacks_cannot_give_a_new_root_input_from_the_same_batch() {
    for open_from_ui in [false, true] {
        let log = Log::default();
        let roots = Rc::new(RefCell::new(Vec::new()));
        let draws = Rc::new(RefCell::new(Vec::new()));
        let opened = Rc::new(RefCell::new(Vec::<Node>::new()));
        let open: Rc<dyn Fn()> = Rc::new({
            let log = log.clone();
            let roots = roots.clone();
            let draws = draws.clone();
            let opened = opened.clone();
            move || {
                if !opened.borrow().is_empty() {
                    return;
                }
                let popup =
                    full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "popup")));
                roots
                    .borrow_mut()
                    .push(SceneRoot::new(*popup, Rect::new(100., 20., 40., 40.)));
                draws.borrow_mut().push(*popup);
                opened.borrow_mut().push(popup);
            }
        });
        let base = full_leaf(Style::new().on_pointer_event({
            let open = open.clone();
            let log = log.clone();
            move |event| {
                record_pointer(log.clone(), "base")(event);
                if open_from_ui && event.action() == PointerAction::Down {
                    open();
                }
            }
        }));
        roots
            .borrow_mut()
            .push(SceneRoot::new(*base, Rect::new(10., 20., 30., 30.)));
        draws.borrow_mut().push(*base);
        let mut scene = Probe::new("scene", log.clone(), vec![base], Vec::new());
        scene.roots = roots;
        scene.draws = draws;
        let raw = RawInput::default();
        scene.input = Some(Box::new({
            let raw = raw.clone();
            move |input, _| {
                raw.borrow_mut().extend_from_slice(input.events());
                if !open_from_ui
                    && input.events().iter().any(|event| {
                        matches!(
                            event,
                            InputEvent::Touch {
                                phase: TouchPhase::Started,
                                ..
                            }
                        )
                    })
                {
                    open();
                }
                Ok(())
            }
        }));
        let mut app = app(scene);
        let globals = Cell::new(0);
        run(&mut app, empty(), &globals).unwrap();
        let input = InputBatch::recorded(|events| {
            let first_x = if open_from_ui { 15. } else { 105. };
            events.touch_event(TouchPhase::Started, 1, first_x, 25., 1.);
            events.touch_event(TouchPhase::Ended, 1, first_x, 25., 1.125);
            events.touch_event(TouchPhase::Started, 2, 105., 25., 1.25);
            events.touch_event(TouchPhase::Ended, 2, 105., 25., 1.375);
        });
        let mut expected = input.events().to_vec();
        run(&mut app, input, &globals).unwrap();
        assert_eq!(count(&log, "popup", "down"), 0);
        assert_eq!(count(&log, "base", "down"), usize::from(open_from_ui));
        assert_raw_events(&raw.borrow(), &expected);
        assert!(app.hit_test_ui(Vector::new(105., 25.)));
        let input = tap(3, 105., 25.);
        expected.extend_from_slice(input.events());
        run(&mut app, input, &globals).unwrap();
        assert_eq!(count(&log, "popup", "down"), 1);
        assert_raw_events(&raw.borrow(), &expected);
        assert_eq!(globals.get(), 3);
    }
}

#[test]
fn scene_receives_full_raw_batch_in_order_including_interactive_and_modal_hits() {
    let log = Log::default();
    let ui = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "ui")));
    let covered = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "covered")));
    let modal = full_leaf(Style::new());
    let decoration =
        full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "decoration")));
    let modal_bounds = Rect::new(100., 20., 80., 80.);
    let roots = vec![
        SceneRoot::new(*ui, Rect::new(20., 30., 60., 60.)),
        SceneRoot::new(*covered, modal_bounds),
        SceneRoot::new(*modal, modal_bounds).input(RootInput::Modal),
        SceneRoot::new(*decoration, frame().viewport).input(RootInput::None),
    ];
    let raw = RawInput::default();
    let mut scene = Probe::new(
        "scene",
        log.clone(),
        vec![ui, covered, modal, decoration],
        roots,
    );
    scene.input = Some(Box::new({
        let log = log.clone();
        let raw = raw.clone();
        move |input, _| {
            if !input.events().is_empty() {
                assert_eq!(
                    count(&log, "ui", "up"),
                    1,
                    "UI dispatch finishes before raw Scene input"
                );
            }
            raw.borrow_mut().extend_from_slice(input.events());
            Ok(())
        }
    }));
    let mut app = app(scene);
    let globals = Cell::new(0);
    run(&mut app, empty(), &globals).unwrap();
    assert!(app.hit_test_ui(Vector::new(30.25, 40.5)));
    assert!(app.hit_test_ui(Vector::new(120.75, 40.25)));
    assert!(!app.hit_test_ui(Vector::new(250.5, 180.25)));
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 1, 30.25, 40.5, 12345.125);
        events.touch_event(TouchPhase::Started, 2, 120.75, 40.25, 12345.25);
        events.char_event('界', Default::default(), false);
        events.touch_event(TouchPhase::Moved, 1, 220.5, 180.25, 12345.375);
        events.touch_event(TouchPhase::Moved, 2, 121.25, 41.5, 12345.5);
        events.touch_event(TouchPhase::Started, 3, 250.5, 180.25, 12345.625);
        events.touch_event(TouchPhase::Ended, 1, 221.75, 181.5, 12345.75);
        events.touch_event(TouchPhase::Ended, 2, 122.5, 42.75, 12345.875);
        events.touch_event(TouchPhase::Moved, 3, 251.25, 181.5, 12346.);
        events.touch_event(TouchPhase::Ended, 3, 252.75, 182.25, 12346.125);
    });
    let expected = input.events().to_vec();
    run(&mut app, input, &globals).unwrap();
    assert_raw_events(&raw.borrow(), &expected);
    assert_eq!(count(&log, "ui", "down"), 1);
    assert_eq!(count(&log, "ui", "move"), 1);
    assert_eq!(count(&log, "ui", "up"), 1);
    assert_eq!(
        count(&log, "covered", "down"),
        0,
        "modal still blocks lower UI"
    );
    assert_eq!(
        count(&log, "decoration", "down"),
        0,
        "RootInput::None remains draw-only"
    );
    assert_eq!(globals.get(), 2);
}

#[test]
fn loading_ui_hold_replaces_after_old_batch_and_game_receives_next_move_up_without_down() {
    let log = Log::default();
    let loading_node =
        full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "loading-node")));
    let loading_ref = *loading_node;
    let roots = vec![SceneRoot::new(loading_ref, Rect::new(20., 30., 60., 60.))];
    let mut loading = Probe::new("loading", log.clone(), vec![loading_node], roots);
    let loading_raw = RawInput::default();
    let game_raw = RawInput::default();
    let game_taps = Rc::new(Cell::new(0));
    loading.input = Some(Box::new({
        let log = log.clone();
        let loading_raw = loading_raw.clone();
        let game_raw = game_raw.clone();
        let game_taps = game_taps.clone();
        move |input, ctx| {
            loading_raw.borrow_mut().extend_from_slice(input.events());
            if input.events().is_empty() {
                return Ok(());
            }
            assert_eq!(count(&log, "loading-node", "down"), 2);
            assert_eq!(count(&log, "loading-node", "move"), 1);
            assert_eq!(count(&log, "loading-node", "up"), 1);
            let game_node = full_leaf(
                Style::new()
                    .on_pointer_event(record_pointer(log.clone(), "game-node"))
                    .on_tap({
                        let game_taps = game_taps.clone();
                        move |_| game_taps.set(game_taps.get() + 1)
                    }),
            );
            let roots = vec![SceneRoot::new(*game_node, Rect::new(20., 30., 60., 60.))];
            let mut game = Probe::new("game", log.clone(), vec![game_node], roots);
            game.input = Some(record_input(game_raw.clone()));
            ctx.replace(game);
            Ok(())
        }
    }));
    let mut app = app(loading);
    let globals = Cell::new(0);
    run(&mut app, empty(), &globals).unwrap();
    log.borrow_mut().clear();
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 90, 30.25, 40.5, 100.125);
        events.touch_event(TouchPhase::Moved, 90, 31.5, 41.25, 100.25);
        events.char_event('l', Default::default(), false);
        events.touch_event(TouchPhase::Started, 91, 35., 45., 100.375);
        events.touch_event(TouchPhase::Ended, 91, 35., 45., 100.5);
    });
    let expected_old = input.events().to_vec();
    run(&mut app, input, &globals).unwrap();
    assert_raw_events(&loading_raw.borrow(), &expected_old);
    assert!(
        game_raw.borrow().is_empty(),
        "the completed old input batch is never replayed"
    );
    assert_eq!(count(&log, "game", "input"), 0);
    assert_eq!(count(&log, "game", "enter"), 1);
    assert_eq!(count(&log, "game", "render"), 1);
    assert_eq!(count(&log, "loading-node", "cancel"), 1);
    assert!(!alive(loading_ref));
    let lifecycle: Vec<_> = log
        .borrow()
        .iter()
        .copied()
        .filter(|(name, phase)| {
            *name == "loading-node" || *phase == "input" || *phase == "drop" || *phase == "enter"
        })
        .collect();
    assert_eq!(
        lifecycle,
        vec![
            ("loading-node", "down"),
            ("loading-node", "move"),
            ("loading-node", "down"),
            ("loading-node", "up"),
            ("loading", "input"),
            ("loading-node", "cancel"),
            ("loading", "drop"),
            ("game", "enter"),
        ]
    );
    let continuation = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 90, 32.25, 42.5, 100.625);
        events.touch_event(TouchPhase::Ended, 90, 32.75, 42.25, 100.75);
    });
    let mut expected_game = continuation.events().to_vec();
    run(&mut app, continuation, &globals).unwrap();
    assert_raw_events(&game_raw.borrow(), &expected_game);
    assert_eq!(count(&log, "game-node", "down"), 0, "no synthetic Down");
    assert_eq!(count(&log, "game-node", "move"), 0);
    assert_eq!(count(&log, "game-node", "up"), 0);
    assert_eq!(game_taps.get(), 0, "the old hold cannot tap new UI");
    let fresh = tap(90, 30., 40.);
    expected_game.extend_from_slice(fresh.events());
    run(&mut app, fresh, &globals).unwrap();
    assert_raw_events(&game_raw.borrow(), &expected_game);
    assert_eq!(count(&log, "game-node", "down"), 1);
    assert_eq!(count(&log, "game-node", "up"), 1);
    assert_eq!(game_taps.get(), 1);
    assert_eq!(count(&log, "game", "enter"), 1);
    assert_eq!(globals.get(), 4);
}

#[test]
fn unmatched_move_and_up_reach_scene_without_any_prior_down_or_router_record() {
    let log = Log::default();
    let raw = RawInput::default();
    let interrupted = Rc::new(RefCell::new(Vec::new()));
    let taps = Rc::new(Cell::new(0));
    let root = full_leaf(
        Style::new()
            .on_pointer_event(record_pointer(log.clone(), "node"))
            .on_tap({
                let taps = taps.clone();
                move |_| taps.set(taps.get() + 1)
            }),
    );
    let roots = vec![SceneRoot::new(*root, Rect::new(20., 30., 60., 60.))];
    let mut scene = Probe::new("scene", log.clone(), vec![root], roots);
    scene.input = Some(Box::new({
        let raw = raw.clone();
        let interrupted = interrupted.clone();
        move |input, _| {
            raw.borrow_mut().extend_from_slice(input.events());
            interrupted.borrow_mut().push(input.is_interrupted());
            Ok(())
        }
    }));
    let mut app = app(scene);
    let globals = Cell::new(0);
    // This contact never had a Down, even on the first activation frame.
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 701, 30.25, 40.5, 80.125);
        events.touch_event(TouchPhase::Ended, 701, 30.5, 40.75, 80.25);
        events.mouse_motion_event(31.25, 41.5);
        events.mouse_button_up_event(MouseButton::Left, 31.5, 41.75);
    });
    let mut expected = input.events().to_vec();
    run(&mut app, input, &globals).unwrap();
    assert_raw_events(&raw.borrow(), &expected);
    // A fresh unmatched contact inside displayed UI is not hidden either.
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 702, 32.25, 42.5, 80.375);
        events.touch_event(TouchPhase::Ended, 702, 32.5, 42.75, 80.5);
        events.touch_event(TouchPhase::Ended, 703, 33.25, 43.5, 80.625);
        events.mouse_motion_event(33.5, 43.75);
        events.mouse_button_up_event(MouseButton::Right, 34.25, 44.5);
    });
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_raw_events(&raw.borrow(), &expected);
    run(&mut app, InputBatch::interrupted(), &globals).unwrap();
    assert_eq!(*interrupted.borrow(), vec![false, false, true]);
    assert_raw_events(&raw.borrow(), &expected);
    assert_eq!(count(&log, "node", "down"), 0);
    assert_eq!(count(&log, "node", "move"), 0);
    assert_eq!(count(&log, "node", "up"), 0);
    assert_eq!(taps.get(), 0);
    assert_eq!(count(&log, "scene", "enter"), 1);
    assert_eq!(globals.get(), 3);
}

#[test]
fn ui_capture_moves_outside_then_removed_root_cancels_without_giving_new_root_same_batch_down() {
    let log = Log::default();
    let old = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "old-node")));
    let new = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "new-node")));
    let (old_ref, new_ref) = (*old, *new);
    let area = Rect::new(20., 30., 60., 60.);
    let mut scene = Probe::new(
        "scene",
        log.clone(),
        vec![old, new],
        vec![SceneRoot::new(old_ref, area)],
    );
    let roots = scene.roots.clone();
    let draws = scene.draws.clone();
    let raw = RawInput::default();
    scene.input = Some(Box::new({
        let raw = raw.clone();
        move |input, _| {
            raw.borrow_mut().extend_from_slice(input.events());
            if input.events().iter().any(|event| {
                matches!(
                    event,
                    InputEvent::Touch {
                        phase: TouchPhase::Started,
                        id: 2,
                        ..
                    }
                )
            }) {
                *roots.borrow_mut() = vec![SceneRoot::new(new_ref, area)];
                *draws.borrow_mut() = vec![new_ref];
            }
            Ok(())
        }
    }));
    let mut app = app(scene);
    let globals = Cell::new(0);
    run(&mut app, empty(), &globals).unwrap();
    log.borrow_mut().clear();
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 1, 30.25, 40.5, 6.125);
    });
    let mut expected = input.events().to_vec();
    run(&mut app, input, &globals).unwrap();
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 250.5, 180.25, 6.25);
        events.touch_event(TouchPhase::Started, 2, 31.25, 41.5, 6.375);
        events.touch_event(TouchPhase::Started, 3, 32.5, 42.75, 6.5);
        events.touch_event(TouchPhase::Ended, 3, 32.5, 42.75, 6.625);
    });
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_eq!(
        count(&log, "old-node", "move"),
        1,
        "Down capture survives moving outside"
    );
    assert_eq!(count(&log, "old-node", "down"), 3);
    assert_eq!(count(&log, "old-node", "up"), 1);
    assert_eq!(count(&log, "old-node", "cancel"), 0);
    assert_eq!(
        count(&log, "new-node", "down"),
        0,
        "new root cannot receive this batch's Down"
    );
    assert!(app.displayed[0].node == new_ref);
    assert!(
        alive(old_ref),
        "removing a displayed root need not dispose its owner"
    );
    assert_raw_events(&raw.borrow(), &expected);

    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 30.5, 40.75, 6.75);
        events.touch_event(TouchPhase::Ended, 1, 30.75, 40.5, 6.875);
        events.touch_event(TouchPhase::Ended, 2, 31.5, 41.75, 7.);
    });
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_eq!(
        count(&log, "old-node", "cancel"),
        2,
        "each removed-root capture is canceled once"
    );
    assert_eq!(count(&log, "old-node", "move"), 1);
    assert_eq!(count(&log, "old-node", "up"), 1);
    assert_eq!(count(&log, "new-node", "down"), 0);
    assert_eq!(count(&log, "new-node", "move"), 0);
    assert_eq!(count(&log, "new-node", "up"), 0);
    assert_raw_events(&raw.borrow(), &expected);
    let input = tap(4, 30., 40.);
    expected.extend_from_slice(input.events());
    run(&mut app, input, &globals).unwrap();
    assert_eq!(count(&log, "old-node", "cancel"), 2);
    assert_eq!(count(&log, "new-node", "down"), 1);
    assert_eq!(count(&log, "new-node", "up"), 1);
    assert_raw_events(&raw.borrow(), &expected);
    assert_eq!(globals.get(), 5);
}

#[test]
fn all_root_node_snapshots_are_frozen_before_any_node_update() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let log = Log::default();
        let child = Rc::new(Cell::new(None));
        let children = map_keyed(rows, |key| *key, {
            let log = log.clone();
            let child = child.clone();
            move |_| {
                let node = leaf(
                    Style::new()
                        .width(20.)
                        .height(20.)
                        .on_update({
                            let log = log.clone();
                            move |_| note(&log, "child", "update")
                        })
                        .on_render({
                            let log = log.clone();
                            move |_| note(&log, "child", "render")
                        }),
                );
                child.set(Some(*node));
                node
            }
        });
        let a = full_leaf(Style::new().on_update({
            let log = log.clone();
            move |_| {
                note(&log, "a", "update");
                rows.set(Rc::from([1]));
            }
        }));
        let b = full_leaf(Style::new().children(children).on_update({
            let log = log.clone();
            move |_| note(&log, "b", "update")
        }));
        let roots = vec![
            SceneRoot::new(*a, Rect::new(10., 10., 80., 80.)),
            SceneRoot::new(*b, Rect::new(100., 20., 80., 80.)),
        ];
        let mut app = app(Probe::new("scene", log.clone(), vec![a, b], roots));
        let globals = Cell::new(0);
        run(&mut app, empty(), &globals).unwrap();
        assert!(alive(
            child
                .get()
                .expect("the first root mounted a child in the second")
        ));
        assert_eq!(
            count(&log, "child", "render"),
            1,
            "new content is laid out and drawn immediately"
        );
        assert_eq!(
            count(&log, "child", "update"),
            0,
            "new content waits for the next frame tick"
        );
        run(&mut app, empty(), &globals).unwrap();
        assert_eq!(count(&log, "child", "update"), 1);
        assert_eq!(count(&log, "a", "update"), 2);
        assert_eq!(count(&log, "b", "update"), 2);
        assert_eq!(globals.get(), 2);
    });
}

#[test]
fn later_root_geometry_feedback_resolves_earlier_root_font_measurement_before_render() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let font = signal(FontSize::Px(10.));
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let a = Node::leaf(Style::new().font_size(font).height(20.), {
            let rendered = rendered.clone();
            move |style| {
                let size = crate::node::use_font_size();
                let layout = use_layout(style);
                layout.set_measure_fn(move |known: taffy::Size<Option<f32>>, _| taffy::Size {
                    width: known.width.unwrap_or(size.get() * 2.),
                    height: known.height.unwrap_or(20.),
                });
                style.on_update.append(move |_| {
                    assert_eq!(size.get(), 18.);
                    assert_eq!(
                        layout.rect.get(),
                        Rect::new(10., 20., 36., 20.),
                        "all roots must converge before the first node update"
                    );
                });
                style.on_render.append(move |_| {
                    rendered.borrow_mut().push((size.get(), layout.rect.get()));
                });
            }
        });
        let a_ref = *a;
        let mut changed = false;
        let b = full_leaf(Style::new().on_layout(move |_| {
            if !changed {
                changed = true;
                font.set(FontSize::Px(18.));
            }
        }));
        let roots = vec![
            SceneRoot::new(a_ref, Rect::new(10., 20., 80., 40.)),
            SceneRoot::new(*b, Rect::new(100., 20., 80., 40.)),
        ];
        let mut app = app(Probe::new("scene", Log::default(), vec![a, b], roots));
        let globals = Cell::new(0);
        run(&mut app, empty(), &globals).unwrap();
        assert_eq!(
            *rendered.borrow(),
            vec![(18., Rect::new(10., 20., 36., 20.))]
        );
        assert_eq!(bounds(a_ref), Rect::new(10., 20., 36., 20.));
        assert_eq!(globals.get(), 1);
    });
}

#[test]
fn duplicate_shared_and_ancestor_roots_return_errors_before_ticking_nodes() {
    for kind in ["duplicate", "shared", "ancestor"] {
        let owner = OwnedScope::new();
        owner.run(|| {
            let log = Log::default();
            let child = leaf(
                Style::new()
                    .width(20.)
                    .height(20.)
                    .on_update(|_| panic!("invalid roots must not tick")),
            );
            let child_ref = *child;
            let area = Rect::new(10., 20., 100., 80.);
            let (nodes, roots) = match kind {
                "duplicate" => (vec![child], vec![SceneRoot::new(child_ref, area); 2]),
                "ancestor" => {
                    let parent = full_leaf(Style::new().children(child));
                    let roots = vec![
                        SceneRoot::new(*parent, area),
                        SceneRoot::new(child_ref, area),
                    ];
                    (vec![parent], roots)
                }
                _ => {
                    // Independent layoutless parents share a borrowed child handle.
                    let a = Node::leaf(
                        Style::new()
                            .children((*signal(Rc::<[NodeRef]>::from([child_ref]))).owned()),
                        |_| {},
                    );
                    let b = Node::leaf(
                        Style::new()
                            .children((*signal(Rc::<[NodeRef]>::from([child_ref]))).owned()),
                        |_| {},
                    );
                    let roots = vec![SceneRoot::new(*a, area), SceneRoot::new(*b, area)];
                    (vec![a, b, child], roots)
                }
            };
            let mut app = app(Probe::new("scene", log.clone(), nodes, roots));
            let error = run(&mut app, empty(), &Cell::new(0)).unwrap_err();
            assert!(app.is_faulted(), "{kind}: {error:#}");
            assert_eq!(count(&log, "scene", "render"), 0);
            assert!(app.displayed.is_empty());
        });
    }
}

#[test]
fn callback_errors_fault_once_and_explicit_replacement_recovers() {
    for phase in ["enter", "input", "update", "layout", "render"] {
        let log = Log::default();
        let node = full_leaf(Style::new());
        let node_ref = *node;
        let mut scene = Probe::new(
            "bad",
            log.clone(),
            vec![node],
            vec![SceneRoot::new(node_ref, Rect::new(10., 20., 40., 40.))],
        );
        scene.fail = Some(phase);
        let mut app = app(scene);
        let globals = Cell::new(0);
        let error = run(&mut app, empty(), &globals).unwrap_err();
        assert!(format!("{error:#}").contains(&format!("deliberate {phase} failure")));
        assert!(app.is_faulted());
        assert!(
            app.displayed.is_empty(),
            "partial rendering cannot retain UI input targets"
        );
        let stopped = log.borrow().clone();
        let global_count = globals.get();
        let faulted_input = InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Started, 1, 15., 25., 2.125);
        });
        assert!(run(&mut app, faulted_input, &globals).is_err());
        assert_eq!(*log.borrow(), stopped, "faults must not retry a callback");
        assert_eq!(globals.get(), global_count);

        let raw = RawInput::default();
        let mut good = Probe::new("good", log.clone(), Vec::new(), Vec::new());
        good.input = Some(record_input(raw.clone()));
        app.replace_scene(good);
        let recovery_input = InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Started, 2, 16., 26., 2.25);
            events.touch_event(TouchPhase::Moved, 1, 15.5, 25.5, 2.375);
        });
        run(&mut app, recovery_input, &globals).unwrap();
        assert!(!app.is_faulted());
        assert!(
            !alive(node_ref),
            "replacement releases the failed Scene tree"
        );
        assert_eq!(count(&log, "bad", "drop"), 1);
        assert_eq!(count(&log, "good", "enter"), 1);
        assert_eq!(count(&log, "good", "input"), 0, "recovery drains old input");
        assert!(raw.borrow().is_empty());
        assert_eq!(count(&log, "good", "render"), 1);
        let continuation = InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Moved, 1, 17.25, 27.5, 2.5);
            events.touch_event(TouchPhase::Moved, 2, 18.5, 28.25, 2.625);
            events.touch_event(TouchPhase::Ended, 1, 19.75, 29., 2.75);
            events.touch_event(TouchPhase::Ended, 2, 20.25, 30.5, 2.875);
        });
        let expected = continuation.events().to_vec();
        run(&mut app, continuation, &globals).unwrap();
        assert_eq!(count(&log, "good", "enter"), 1);
        assert_eq!(count(&log, "good", "input"), 1);
        assert_raw_events(&raw.borrow(), &expected);
        assert_eq!(globals.get(), global_count + 2);
    }
}

#[test]
fn commands_requested_during_recovery_enter_wait_until_the_next_frame() {
    for exit in [false, true] {
        let log = Log::default();
        let mut bad = Probe::new("a", log.clone(), Vec::new(), Vec::new());
        bad.fail = Some("enter");
        let mut app = app(bad);
        let globals = Cell::new(0);
        assert!(run(&mut app, empty(), &globals).is_err());
        let mut b = Probe::new("b", log.clone(), Vec::new(), Vec::new());
        b.enter = Some(Box::new({
            let log = log.clone();
            move |ctx| {
                if exit {
                    ctx.exit();
                } else {
                    ctx.replace(Probe::new("c", log.clone(), Vec::new(), Vec::new()));
                }
                Ok(())
            }
        }));
        app.replace_scene(b);
        run(&mut app, tap(1, 10., 10.), &globals).unwrap();
        assert!(!app.is_exited());
        assert_eq!(count(&log, "b", "enter"), 1);
        assert_eq!(count(&log, "b", "render"), 1);
        assert_eq!(count(&log, "b", "drop"), 0);
        assert_eq!(count(&log, "b", "input"), 0);
        assert_eq!(count(&log, "c", "enter"), 0);
        run(&mut app, empty(), &globals).unwrap();
        assert_eq!(count(&log, "b", "enter"), 1);
        assert_eq!(count(&log, "b", "drop"), 1);
        if exit {
            assert!(app.is_exited());
        } else {
            assert_eq!(count(&log, "c", "enter"), 1);
            assert_eq!(count(&log, "c", "input"), 0);
            assert_eq!(count(&log, "c", "render"), 1);
        }
    }
}

#[test]
fn replacement_and_exit_complete_old_input_then_cancel_before_releasing_resources() {
    for exit in [false, true] {
        let log = Log::default();
        let resource = Rc::new(());
        let weak_resource = Rc::downgrade(&resource);
        let local = Rc::new(Cell::new(None::<ReadSignal<u8>>));
        let old = Node::leaf(Style::new().width(60.).height(60.), {
            let log = log.clone();
            let local = local.clone();
            let weak_resource = weak_resource.clone();
            move |style| {
                use_layout(style);
                let value = *signal(7_u8);
                local.set(Some(value));
                style.on_pointer_event.append(move |event| {
                    assert!(!value.is_disposed());
                    assert!(weak_resource.upgrade().is_some());
                    record_pointer(log.clone(), "old-node")(event);
                });
            }
        });
        let old_ref = *old;
        let mut old_scene = Probe::new(
            "old",
            log.clone(),
            vec![old],
            vec![SceneRoot::new(old_ref, Rect::new(20., 30., 60., 60.))],
        );
        old_scene.resource = Some(resource);
        let replace = Rc::new(Cell::new(false));
        old_scene.input = Some(Box::new({
            let log = log.clone();
            let replace = replace.clone();
            move |_, ctx| {
                if replace.get() {
                    if exit {
                        ctx.exit();
                    } else {
                        let new = full_leaf(
                            Style::new().on_pointer_event(record_pointer(log.clone(), "new-node")),
                        );
                        let roots = vec![SceneRoot::new(*new, Rect::new(20., 30., 60., 60.))];
                        ctx.replace(Probe::new("new", log.clone(), vec![new], roots));
                    }
                }
                Ok(())
            }
        }));
        let mut app = app(old_scene);
        let globals = Cell::new(0);
        run(&mut app, empty(), &globals).unwrap();
        log.borrow_mut().clear();
        replace.set(true);
        run(
            &mut app,
            InputBatch::recorded(|events| {
                events.touch_event(TouchPhase::Started, 1, 30., 40., 1.);
                events.touch_event(TouchPhase::Moved, 1, 31., 41., 1.1);
                events.touch_event(TouchPhase::Started, 2, 35., 45., 1.2);
                events.touch_event(TouchPhase::Ended, 2, 35., 45., 1.3);
            }),
            &globals,
        )
        .unwrap();
        let input_order: Vec<_> = log
            .borrow()
            .iter()
            .copied()
            .filter(|(name, phase)| *name == "old-node" || *phase == "drop" || *phase == "enter")
            .collect();
        let mut expected = vec![
            ("old-node", "down"),
            ("old-node", "move"),
            ("old-node", "down"),
            ("old-node", "up"),
            ("old-node", "cancel"),
            ("old", "drop"),
        ];
        if !exit {
            expected.push(("new", "enter"));
        }
        assert_eq!(input_order, expected);
        assert!(!alive(old_ref));
        assert!(local.get().unwrap().is_disposed());
        assert!(weak_resource.upgrade().is_none());
        assert_eq!(count(&log, "new-node", "down"), 0);
        assert_eq!(count(&log, "new", "input"), 0);
        assert_eq!(globals.get(), 2);
        if exit {
            assert!(app.is_exited());
            let stopped = log.borrow().clone();
            run(&mut app, tap(3, 30., 40.), &globals).unwrap();
            assert_eq!(*log.borrow(), stopped);
            assert_eq!(globals.get(), 2);
        } else {
            run(&mut app, tap(3, 30., 40.), &globals).unwrap();
            assert_eq!(count(&log, "new-node", "down"), 1);
            assert_eq!(count(&log, "new", "enter"), 1);
        }
    }
}

#[test]
fn held_mouse_replacement_keeps_raw_chord_and_accepts_a_new_physical_down() {
    for old_ui_owner in [false, true] {
        let log = Log::default();
        let a = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "a-node")));
        let roots = vec![SceneRoot::new(*a, Rect::new(20., 30., 60., 60.))];
        let old_raw = RawInput::default();
        let mut old = Probe::new("a", log.clone(), vec![a], roots);
        old.input = Some(record_input(old_raw.clone()));
        let mut app = app(old);
        let globals = Cell::new(0);
        run(&mut app, empty(), &globals).unwrap();
        let input = InputBatch::recorded(|events| {
            let x = if old_ui_owner { 30. } else { 200. };
            events.mouse_button_down_event(MouseButton::Left, x, 40.);
        });
        let expected_old = input.events().to_vec();
        run(&mut app, input, &globals).unwrap();
        assert_eq!(count(&log, "a-node", "down"), usize::from(old_ui_owner));
        assert_raw_events(&old_raw.borrow(), &expected_old);

        let pointer_events = Rc::new(RefCell::new(Vec::new()));
        let taps = Rc::new(Cell::new(0));
        let b = full_leaf(
            Style::new()
                .on_pointer_event({
                    let log = log.clone();
                    let pointer_events = pointer_events.clone();
                    move |event| {
                        record_pointer(log.clone(), "b-node")(event);
                        pointer_events.borrow_mut().push((
                            event.action(),
                            event.position(),
                            event.button_state(),
                        ));
                    }
                })
                .on_tap({
                    let taps = taps.clone();
                    move |_| taps.set(taps.get() + 1)
                }),
        );
        let roots = vec![SceneRoot::new(*b, Rect::new(20., 30., 60., 60.))];
        let mut next = Probe::new("b", log.clone(), vec![b], roots);
        let raw = RawInput::default();
        next.input = Some(record_input(raw.clone()));
        app.replace_scene(next);
        run(&mut app, empty(), &globals).unwrap();
        assert_eq!(count(&log, "a-node", "cancel"), usize::from(old_ui_owner));
        assert_eq!(count(&log, "b", "input"), 0, "old batch is not replayed");
        let chord = InputBatch::recorded(|events| {
            events.mouse_button_down_event(MouseButton::Right, 30.25, 40.5);
            events.mouse_button_up_event(MouseButton::Left, 30.5, 40.75);
            events.mouse_button_up_event(MouseButton::Right, 31., 41.);
        });
        let mut expected = chord.events().to_vec();
        run(&mut app, chord, &globals).unwrap();
        assert_raw_events(&raw.borrow(), &expected);
        assert_eq!(
            *pointer_events.borrow(),
            vec![
                (
                    PointerAction::Down,
                    Vector::new(30.25, 40.5),
                    ButtonState::SECONDARY
                ),
                (
                    PointerAction::Move,
                    Vector::new(31., 41.),
                    ButtonState::SECONDARY
                ),
                (
                    PointerAction::Up,
                    Vector::new(31., 41.),
                    ButtonState::empty()
                ),
            ],
            "a new physical RightDown starts UI capture; old LeftUp cannot move or release it"
        );
        assert_eq!(
            taps.get(),
            1,
            "only the matching RightUp completes the new tap"
        );

        let input = InputBatch::recorded(|events| {
            events.mouse_button_down_event(MouseButton::Left, 30., 40.);
            events.mouse_button_up_event(MouseButton::Left, 30., 40.);
        });
        expected.extend_from_slice(input.events());
        run(&mut app, input, &globals).unwrap();
        assert_eq!(count(&log, "b-node", "down"), 2);
        assert_eq!(count(&log, "b-node", "up"), 2);
        assert_eq!(taps.get(), 2);
        let input = InputBatch::recorded(|events| {
            events.mouse_button_down_event(MouseButton::Left, 200., 40.);
            events.mouse_button_up_event(MouseButton::Left, 200., 40.);
        });
        expected.extend_from_slice(input.events());
        run(&mut app, input, &globals).unwrap();
        assert_eq!(count(&log, "b-node", "down"), 2);
        assert_eq!(count(&log, "b-node", "up"), 2);
        assert_eq!(taps.get(), 2);
        assert_raw_events(&raw.borrow(), &expected);
        assert_eq!(globals.get(), 6);
    }
}

#[test]
fn faulted_frames_stop_callbacks_without_hiding_continuations_after_recovery() {
    for old_ui_owner in [false, true] {
        for release_while_faulted in [false, true] {
            let log = Log::default();
            let fail = Rc::new(Cell::new(false));
            let a = full_leaf(Style::new().on_pointer_event(record_pointer(log.clone(), "a-node")));
            let roots = vec![SceneRoot::new(*a, Rect::new(20., 30., 60., 60.))];
            let mut scene = Probe::new("a", log.clone(), vec![a], roots);
            let old_raw = RawInput::default();
            scene.input = Some(Box::new({
                let fail = fail.clone();
                let old_raw = old_raw.clone();
                move |input, _| {
                    old_raw.borrow_mut().extend_from_slice(input.events());
                    ensure!(!fail.get(), "deliberate input failure");
                    Ok(())
                }
            }));
            let mut app = app(scene);
            let globals = Cell::new(0);
            run(&mut app, empty(), &globals).unwrap();
            fail.set(true);
            let failed_input = InputBatch::recorded(|events| {
                let x = if old_ui_owner { 30. } else { 200. };
                events.mouse_button_down_event(MouseButton::Left, x, 40.);
                events.touch_event(TouchPhase::Started, 7, x, 40., 3.125);
            });
            let expected_old = failed_input.events().to_vec();
            assert!(run(&mut app, failed_input, &globals).is_err());
            assert_raw_events(&old_raw.borrow(), &expected_old);
            assert!(app.is_faulted());
            assert_eq!(
                count(&log, "a-node", "cancel"),
                2 * usize::from(old_ui_owner)
            );
            let stopped = log.borrow().clone();
            let global_count = globals.get();
            let stopped_input = InputBatch::recorded(|events| {
                events.mouse_motion_event(30.25, 40.5);
                events.touch_event(TouchPhase::Moved, 7, 30.25, 40.5, 3.25);
                events.touch_event(TouchPhase::Started, 9, 31., 41., 3.375);
                if release_while_faulted {
                    events.mouse_button_up_event(MouseButton::Left, 30.5, 40.75);
                    events.touch_event(TouchPhase::Ended, 7, 30.5, 40.75, 3.5);
                }
            });
            assert!(run(&mut app, stopped_input, &globals).is_err());
            assert_eq!(*log.borrow(), stopped);
            assert_eq!(globals.get(), global_count);
            assert_raw_events(&old_raw.borrow(), &expected_old);

            let taps = Rc::new(Cell::new(0));
            let b = full_leaf(
                Style::new()
                    .on_pointer_event(record_pointer(log.clone(), "b-node"))
                    .on_tap({
                        let taps = taps.clone();
                        move |_| taps.set(taps.get() + 1)
                    }),
            );
            let roots = vec![SceneRoot::new(*b, Rect::new(20., 30., 60., 60.))];
            let mut recovered = Probe::new("b", log.clone(), vec![b], roots);
            let raw = RawInput::default();
            recovered.input = Some(record_input(raw.clone()));
            app.replace_scene(recovered);
            let drained_input = InputBatch::recorded(|events| {
                events.mouse_motion_event(31., 41.);
                events.touch_event(TouchPhase::Moved, 9, 31.25, 41.5, 3.625);
                events.touch_event(TouchPhase::Started, 11, 32., 42., 3.75);
            });
            run(&mut app, drained_input, &globals).unwrap();
            assert_eq!(
                count(&log, "b", "input"),
                0,
                "recovery still drains its first batch"
            );
            assert!(raw.borrow().is_empty());
            assert_eq!(count(&log, "b-node", "down"), 0);
            let continuation = InputBatch::recorded(|events| {
                events.mouse_motion_event(32.25, 42.5);
                events.mouse_button_up_event(MouseButton::Left, 32.5, 42.75);
                events.touch_event(TouchPhase::Moved, 7, 32.25, 42.5, 3.875);
                events.touch_event(TouchPhase::Ended, 7, 32.5, 42.75, 4.);
                events.touch_event(TouchPhase::Moved, 9, 33.25, 43.5, 4.125);
                events.touch_event(TouchPhase::Ended, 9, 33.5, 43.75, 4.25);
                events.touch_event(TouchPhase::Moved, 11, 34.25, 44.5, 4.375);
                events.touch_event(TouchPhase::Ended, 11, 34.5, 44.75, 4.5);
            });
            let mut expected = continuation.events().to_vec();
            run(&mut app, continuation, &globals).unwrap();
            assert_raw_events(&raw.borrow(), &expected);
            assert_eq!(count(&log, "b-node", "down"), 0);
            assert_eq!(count(&log, "b-node", "move"), 0);
            assert_eq!(count(&log, "b-node", "up"), 0);
            assert_eq!(taps.get(), 0, "old releases do not activate new UI");

            let fresh = InputBatch::recorded(|events| {
                events.touch_event(TouchPhase::Started, 7, 30., 40., 4.625);
                events.touch_event(TouchPhase::Ended, 7, 30., 40., 4.75);
                events.mouse_button_down_event(MouseButton::Left, 30., 40.);
                events.mouse_button_up_event(MouseButton::Left, 30., 40.);
            });
            expected.extend_from_slice(fresh.events());
            run(&mut app, fresh, &globals).unwrap();
            assert_eq!(count(&log, "b-node", "down"), 2);
            assert_eq!(count(&log, "b-node", "up"), 2);
            assert_eq!(
                taps.get(),
                2,
                "fresh physical presses are not permanently suppressed"
            );
            let outside = InputBatch::recorded(|events| {
                events.mouse_button_down_event(MouseButton::Left, 200., 40.);
                events.mouse_button_up_event(MouseButton::Left, 200., 40.);
            });
            expected.extend_from_slice(outside.events());
            run(&mut app, outside, &globals).unwrap();
            assert_eq!(count(&log, "b-node", "down"), 2);
            assert_eq!(count(&log, "b-node", "up"), 2);
            assert_eq!(taps.get(), 2);
            assert_raw_events(&raw.borrow(), &expected);
            assert_eq!(globals.get(), global_count + 4);
        }
    }
}

#[test]
fn pause_and_resume_are_once_per_transition_and_pause_cancels_before_callback() {
    let log = Log::default();
    let root = full_leaf(
        Style::new()
            .on_pointer_event(record_pointer(log.clone(), "node"))
            .on_update({
                let log = log.clone();
                move |_| note(&log, "node", "update")
            }),
    );
    let roots = vec![SceneRoot::new(*root, Rect::new(20., 30., 60., 60.))];
    let mut app = app(Probe::new("scene", log.clone(), vec![root], roots));
    let globals = Cell::new(0);
    run(&mut app, empty(), &globals).unwrap();
    run(
        &mut app,
        InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Started, 1, 30., 40., 1.);
        }),
        &globals,
    )
    .unwrap();
    log.borrow_mut().clear();
    // The public pause path additionally drains the host subscriber and tasks.
    // Reproduce its input cancellation without touching macroquad or its clock.
    for _ in 0..2 {
        app.pointer_manager.cancel_input();
        app.pause_scene(frame()).unwrap();
    }
    assert_eq!(*log.borrow(), vec![("node", "cancel"), ("scene", "pause")]);
    assert_eq!(globals.get(), 2);
    assert!(app.host_paused);
    assert!(app.scenes.last().unwrap().state == SceneState::Paused);
    run(&mut app, empty(), &globals).unwrap();
    run(&mut app, empty(), &globals).unwrap();
    assert_eq!(count(&log, "scene", "resume"), 1);
    assert_eq!(count(&log, "scene", "enter"), 0);
    assert_eq!(count(&log, "node", "update"), 2);
    assert_eq!(globals.get(), 4);
    assert!(
        log.borrow()
            .iter()
            .position(|entry| *entry == ("scene", "resume"))
            .unwrap()
            < log
                .borrow()
                .iter()
                .position(|entry| *entry == ("scene", "input"))
                .unwrap()
    );
}

#[test]
fn draw_rejects_unsubmitted_duplicate_and_disposed_roots_without_drawing_them() {
    let rendered = Rc::new(Cell::new(0));
    let valid = leaf(Style::new().width(20.).height(20.).on_render({
        let rendered = rendered.clone();
        move |_| rendered.set(rendered.get() + 1)
    }));
    let unsubmitted = leaf(Style::new());
    let disposed = leaf(Style::new());
    let disposed_ref = *disposed;
    let area = Rect::new(10., 20., 40., 40.);
    valid.prepare_layout_in(area);
    let prepared = [
        SceneRoot::new(*valid, area),
        SceneRoot::new(disposed_ref, area),
    ];
    let mut render = SceneRender {
        frame: frame(),
        prepared: &prepared,
        drawn: Vec::new(),
    };
    assert!(render.draw(*unsubmitted).is_err());
    assert_eq!(rendered.get(), 0);
    render.draw(*valid).unwrap();
    assert!(render.draw(*valid).is_err());
    assert_eq!(rendered.get(), 1);
    drop(disposed);
    assert!(render.draw(disposed_ref).is_err());
    assert_eq!(render.drawn.len(), 1);
    assert!(render.drawn[0].node == *valid);
}
