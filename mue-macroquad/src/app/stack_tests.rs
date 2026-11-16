//! Headless scene-stack scheduling regressions. Nodes use layout but never paint.

use super::*;
use crate::{
    SceneEvent,
    event::pointer::{PointerAction, PointerEvent},
    input::InputEvent,
    layout::use_layout,
    node::Node,
    scene::SceneResult,
    style::{Style, Styleable},
};
use miniquad::TouchPhase;
use mue_core::{
    effect::{on_cleanup, watch_effect},
    scope::{OwnedScope, scope_future},
    signal::{Access, Signal, signal},
    task::{self, Task},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};

type Log = Rc<RefCell<Vec<(&'static str, &'static str)>>>;
type ContextHook = Box<dyn FnMut(&mut SceneContext<'_>) -> Result<()>>;
type InputHook = Box<dyn FnMut(&InputBatch, &mut SceneContext<'_>) -> Result<()>>;
type ResultHook = Box<dyn FnMut(SceneResult, &mut SceneContext<'_>) -> Result<()>>;

#[derive(Clone, Default)]
struct Stats {
    raw: Rc<RefCell<Vec<InputEvent>>>,
    scene_dt: Rc<RefCell<Vec<f32>>>,
    node_dt: Rc<RefCell<Vec<f32>>>,
    result_dt: Rc<RefCell<Vec<f32>>>,
    resume_dt: Rc<RefCell<Vec<f32>>>,
    results: Rc<RefCell<Vec<u32>>>,
    instance_updates: Rc<RefCell<Vec<usize>>>,
}

struct Probe {
    name: &'static str,
    log: Log,
    stats: Stats,
    nodes: Vec<Node>,
    roots: Vec<SceneRoot>,
    enter: Option<ContextHook>,
    input: Option<InputHook>,
    update: Option<ContextHook>,
    pause: Option<ContextHook>,
    resume: Option<ContextHook>,
    result: Option<ResultHook>,
    fail: Option<&'static str>,
    resource: Option<Rc<()>>,
    task: Option<Task<()>>,
    drop_hook: Option<Box<dyn FnMut()>>,
    updates: usize,
}

impl Probe {
    fn new(name: &'static str, log: Log) -> Self {
        Self {
            name,
            log,
            stats: Stats::default(),
            nodes: Vec::new(),
            roots: Vec::new(),
            enter: None,
            input: None,
            update: None,
            pause: None,
            resume: None,
            result: None,
            fail: None,
            resource: None,
            task: None,
            drop_hook: None,
            updates: 0,
        }
    }

    fn with_node(mut self, node: Node) -> Self {
        self.roots
            .push(SceneRoot::new(*node, frame(0., 0.).viewport));
        self.nodes.push(node);
        self
    }

    fn check(&self, phase: &'static str) -> Result<()> {
        ensure!(self.fail != Some(phase), "deliberate {phase} failure");
        Ok(())
    }

    fn lifecycle(&mut self, phase: &'static str, ctx: &mut SceneContext<'_>) -> Result<()> {
        note(&self.log, self.name, phase);
        let hook = match phase {
            "enter" => &mut self.enter,
            "pause" => &mut self.pause,
            "resume" => &mut self.resume,
            _ => unreachable!(),
        };
        if let Some(hook) = hook {
            hook(ctx)?;
        }
        self.check(phase)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(hook) = &mut self.drop_hook {
            hook();
        }
        note(&self.log, self.name, "drop");
    }
}

impl Scene for Probe {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => self.lifecycle("enter", ctx),
            SceneEvent::Pause => self.lifecycle("pause", ctx),
            SceneEvent::Resume => {
                self.stats.resume_dt.borrow_mut().push(ctx.frame.dt);
                self.lifecycle("resume", ctx)
            }
            SceneEvent::Input(input) => {
                note(&self.log, self.name, "input");
                self.stats
                    .raw
                    .borrow_mut()
                    .extend_from_slice(input.events());
                if let Some(hook) = &mut self.input {
                    hook(input, ctx)?;
                }
                self.check("input")
            }
            SceneEvent::Returned(result) => {
                note(&self.log, self.name, "result");
                self.stats.result_dt.borrow_mut().push(ctx.frame.dt);
                if let Some(hook) = &mut self.result {
                    hook(result, ctx)?;
                } else if let Ok(value) = result.downcast::<u32>() {
                    self.stats.results.borrow_mut().push(*value);
                }
                self.check("result")
            }
        }
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        note(&self.log, self.name, "update");
        self.updates += 1;
        self.stats.instance_updates.borrow_mut().push(self.updates);
        self.stats.scene_dt.borrow_mut().push(ctx.frame.dt);
        if let Some(hook) = &mut self.update {
            hook(ctx)?;
        }
        self.check("update")
    }

    fn layout(&mut self, _: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        note(&self.log, self.name, "layout");
        self.check("layout")?;
        roots.extend(self.roots.iter().copied());
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        note(&self.log, self.name, "render");
        for root in &self.roots {
            render.draw(root.node)?;
        }
        self.check("render")
    }
}

fn frame(dt: f32, time: f64) -> SceneFrame {
    SceneFrame {
        dt,
        time,
        viewport: Rect::new(0., 0., 320., 240.),
    }
}

fn app(scene: impl Scene + 'static) -> App {
    App {
        scenes: vec![SceneEntry::new(Box::new(scene))],
        state: AppState::Active,
        host_paused: false,
        command: None,
        active_transition: None,
        default_transition: None,
        pictures: PictureCache::headless(),
        displayed: Vec::new(),
        pointer_manager: PointerManager::headless(),
    }
}

fn empty() -> InputBatch {
    InputBatch::recorded(|_| {})
}

fn run_input(app: &mut App, input: InputBatch, dt: f32, time: f64) -> Result<()> {
    app.run_checked_frame(input, frame(dt, time), || {}, |_, _| {})
}

fn run(app: &mut App) -> Result<()> {
    run_input(app, empty(), 0.125, 42.)
}

fn paused(app: &mut App, time: f64) -> Result<()> {
    app.run_checked_paused_frame(frame(0., time), |_| {})
}

fn request_exit(app: &mut App) {
    SceneContext {
        frame: frame(0., 42.),
        command: &mut app.command,
        transition: None,
    }
    .exit();
}

fn leaf(style: Style) -> Node {
    Node::leaf(style.width(80.).height(80.), |style| {
        use_layout(style);
    })
}

fn ticking_node(log: Log, name: &'static str, stats: Stats) -> Node {
    leaf(
        Style::new()
            .on_pointer_event(record_pointer(log.clone(), name))
            .on_update({
                let log = log.clone();
                move |dt| {
                    stats.node_dt.borrow_mut().push(*dt);
                    note(&log, name, "update");
                }
            })
            .on_render(move |_| note(&log, name, "render")),
    )
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

fn lifecycle(log: &Log) -> Vec<(&'static str, &'static str)> {
    log.borrow()
        .iter()
        .copied()
        .filter(|(_, phase)| matches!(*phase, "enter" | "pause" | "result" | "resume" | "drop"))
        .collect()
}

fn scheduled(log: &Log) -> Vec<(&'static str, &'static str)> {
    log.borrow()
        .iter()
        .copied()
        .filter(|(_, phase)| *phase != "drop")
        .collect()
}

fn record_pointer(log: Log, name: &'static str) -> impl FnMut(&PointerEvent) {
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

fn assert_raw(actual: &[InputEvent], expected: &[InputEvent]) {
    // Debug includes the original physical positions, IDs, timestamps and order.
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

struct Answer {
    value: String,
    dropped: Rc<Cell<usize>>,
}

impl Drop for Answer {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}

#[test]
fn one_event_method_receives_every_variant_and_returns_before_resume_once() {
    let log = Log::default();
    let scene = Probe::new("events", log.clone());
    let stats = scene.stats.clone();
    let mut app = app(scene);
    run(&mut app).unwrap();
    app.push_scene(Probe::new("child", log.clone()));
    run(&mut app).unwrap();
    app.pop_scene_with_result(17_u32);
    run(&mut app).unwrap();
    run(&mut app).unwrap();

    let notifications: Vec<_> = log
        .borrow()
        .iter()
        .filter(|(name, phase)| {
            *name == "events" && matches!(*phase, "enter" | "input" | "pause" | "result" | "resume")
        })
        .map(|(_, phase)| *phase)
        .collect();
    assert_eq!(
        notifications,
        vec![
            "enter", "input", "input", "pause", "result", "resume", "input"
        ]
    );
    assert_eq!(*stats.results.borrow(), vec![17]);
    assert!(app.scenes[0].pending_results.is_empty());
}

#[test]
fn default_event_accepts_input_and_drops_returns_without_other_handlers() {
    struct DefaultScene;
    impl Scene for DefaultScene {}

    let dropped = Rc::new(Cell::new(0));
    let downs = Rc::new(Cell::new(0));
    let mut app = app(DefaultScene);
    run(&mut app).unwrap();
    app.push_scene(crate::scene::NodeScene {
        root: leaf(Style::new().on_pointer_event({
            let downs = downs.clone();
            move |event| {
                if event.action() == PointerAction::Down {
                    downs.set(downs.get() + 1);
                }
            }
        })),
    });
    run(&mut app).unwrap();
    let input = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 7, 30.25, 40.5, 1.25);
        events.char_event('界', Default::default(), false);
        events.touch_event(TouchPhase::Ended, 7, 31.5, 41.25, 1.5);
    });
    run_input(&mut app, input, 0.125, 42.).unwrap();
    assert_eq!(downs.get(), 1, "Node-only Scene still dispatches UI input");
    app.pop_scene_with_result(Answer {
        value: "ignored by the default event".into(),
        dropped: dropped.clone(),
    });
    run(&mut app).unwrap();
    assert_eq!(dropped.get(), 1);
    assert!(app.scenes[0].pending_results.is_empty());
    assert!(app.scenes[0].state == SceneState::Running);
    paused(&mut app, 100.).unwrap();
    run(&mut app).unwrap();
    assert_eq!(dropped.get(), 1, "ignored return values are consumed once");
}

#[test]
fn push_retains_scene_instance_nodes_and_signals_while_only_top_is_scheduled() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let log = Log::default();
        let local = Rc::new(Cell::new(None::<Signal<u32>>));
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let mut lower = Probe::new("lower", log.clone());
        let lower_stats = lower.stats.clone();
        let lower_node = Node::leaf(Style::new().width(80.).height(80.), {
            let local = local.clone();
            let rendered = rendered.clone();
            let stats = lower_stats.clone();
            move |style| {
                use_layout(style);
                let value = signal(7_u32);
                local.set(Some(value));
                style.on_update.append({
                    let stats = stats.clone();
                    move |dt| {
                        stats.node_dt.borrow_mut().push(*dt);
                        value.set(value.get() + 1);
                    }
                });
                style
                    .on_render
                    .append(move |_| rendered.borrow_mut().push(value.get()));
            }
        });
        let lower_ref = *lower_node;
        let resource = Rc::new(());
        let weak = Rc::downgrade(&resource);
        lower.resource = Some(resource);
        let mut app = app(lower.with_node(lower_node));
        run(&mut app).unwrap();
        assert_eq!(*lower_stats.instance_updates.borrow(), vec![1]);
        assert_eq!(local.get().unwrap().get(), 8);

        let upper = Probe::new("upper", log.clone());
        let upper_stats = upper.stats.clone();
        let upper_node = ticking_node(log.clone(), "upper-node", upper_stats.clone());
        let upper_ref = *upper_node;
        app.push_scene(upper.with_node(upper_node));
        log.borrow_mut().clear();
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), 2);
        assert!(app.scenes[0].state == SceneState::Paused);
        assert!(app.scenes[1].state == SceneState::Running);
        assert_eq!(
            lifecycle(&log),
            vec![("lower", "pause"), ("upper", "enter")]
        );
        assert!(alive(lower_ref));
        assert!(weak.upgrade().is_some());
        assert!(!local.get().unwrap().is_disposed());
        assert_eq!(local.get().unwrap().get(), 8);
        assert_eq!(count(&log, "lower", "update"), 0);
        assert_eq!(count(&log, "lower", "layout"), 0);
        assert_eq!(count(&log, "lower", "render"), 0);
        assert_eq!(count(&log, "upper", "input"), 0);
        assert_eq!(count(&log, "upper", "update"), 0);
        assert_eq!(upper_stats.node_dt.borrow().len(), 1);

        let covered = log.borrow().len();
        for (dt, time) in [(0.25, 100.), (0.5, 1000.), (0.75, 10000.)] {
            run_input(&mut app, empty(), dt, time).unwrap();
        }
        assert!(
            log.borrow()[covered..]
                .iter()
                .all(|(name, _)| *name != "lower")
        );
        assert_eq!(*lower_stats.instance_updates.borrow(), vec![1]);
        assert_eq!(local.get().unwrap().get(), 8);
        assert_eq!(*rendered.borrow(), vec![8]);
        assert_eq!(*upper_stats.instance_updates.borrow(), vec![1, 2, 3]);

        let lower_inputs = count(&log, "lower", "input");
        let lower_ticks = lower_stats.node_dt.borrow().len();
        app.pop_scene();
        log.borrow_mut().clear();
        run_input(&mut app, empty(), 9., 20000.).unwrap();
        assert_eq!(app.scene_count(), 1);
        assert_eq!(
            lifecycle(&log),
            vec![("upper", "pause"), ("upper", "drop"), ("lower", "resume")]
        );
        assert!(alive(lower_ref));
        assert!(!alive(upper_ref));
        assert!(weak.upgrade().is_some());
        assert!(app.displayed[0].node == lower_ref);
        assert_eq!(*lower_stats.instance_updates.borrow(), vec![1]);
        assert_eq!(lower_stats.node_dt.borrow().len(), lower_ticks + 1);
        assert_eq!(*lower_stats.node_dt.borrow().last().unwrap(), 0.);
        assert_eq!(local.get().unwrap().get(), 9);
        assert_eq!(*rendered.borrow(), vec![8, 9]);
        assert_eq!(count(&log, "lower", "input"), 0);
        assert_eq!(count(&log, "lower", "update"), 0);
        assert_eq!(count(&log, "lower", "layout"), 2);
        assert_eq!(count(&log, "lower", "render"), 1);
        assert!(lower_inputs >= 1);
        run_input(&mut app, empty(), 0.375, 20000.375).unwrap();
        assert_eq!(*lower_stats.instance_updates.borrow(), vec![1, 2]);
        assert_eq!(*lower_stats.node_dt.borrow().last().unwrap(), 0.375);
        assert_eq!(
            count(&log, "lower", "enter"),
            0,
            "resumption never reconstructs/re-enters"
        );
    });
}

#[test]
fn pop_delivers_owned_result_once_after_drop_before_resume_without_old_batch_replay() {
    let log = Log::default();
    let received = Rc::new(RefCell::new(Vec::new()));
    let dropped = Rc::new(Cell::new(0));
    let mut lower = Probe::new("lower", log.clone());
    let lower_stats = lower.stats.clone();
    lower.result = Some(Box::new({
        let received = received.clone();
        move |result, ctx| {
            assert_eq!(ctx.frame.dt, 0.);
            let answer = result
                .downcast::<Answer>()
                .expect("result keeps its concrete Any type");
            received.borrow_mut().push(answer.value.clone());
            Ok(())
        }
    }));
    lower.resume = Some(Box::new({
        let dropped = dropped.clone();
        let received = received.clone();
        move |ctx| {
            assert_eq!(ctx.frame.dt, 0.);
            assert_eq!(*received.borrow(), vec!["finished".to_owned()]);
            assert_eq!(dropped.get(), 1);
            Ok(())
        }
    }));
    let node = ticking_node(log.clone(), "lower-node", lower_stats.clone());
    let mut app = app(lower.with_node(node));
    run(&mut app).unwrap();
    let mut upper = Probe::new("upper", log.clone());
    let upper_stats = upper.stats.clone();
    upper.input = Some(Box::new({
        let mut answer = Some(Answer {
            value: "finished".into(),
            dropped: dropped.clone(),
        });
        move |_, ctx| {
            if let Some(answer) = answer.take() {
                ctx.pop_with_result(answer);
            }
            Ok(())
        }
    }));
    app.push_scene(upper);
    run(&mut app).unwrap();
    let lower_inputs = count(&log, "lower", "input");
    let lower_updates = lower_stats.scene_dt.borrow().len();
    let lower_ticks = lower_stats.node_dt.borrow().len();
    log.borrow_mut().clear();
    let batch = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 11, 30., 40., 1.25);
        events.touch_event(TouchPhase::Ended, 11, 31., 41., 1.5);
        events.char_event('终', Default::default(), false);
    });
    let expected = batch.events().to_vec();
    run_input(&mut app, batch, 3., 500.).unwrap();
    assert_raw(&upper_stats.raw.borrow(), &expected);
    assert!(lower_stats.raw.borrow().is_empty());
    assert_eq!(
        lifecycle(&log),
        vec![
            ("upper", "pause"),
            ("upper", "drop"),
            ("lower", "result"),
            ("lower", "resume")
        ]
    );
    assert_eq!(count(&log, "lower", "input"), 0);
    assert_eq!(lower_stats.scene_dt.borrow().len(), lower_updates);
    assert_eq!(lower_stats.node_dt.borrow().len(), lower_ticks + 1);
    assert_eq!(*lower_stats.node_dt.borrow().last().unwrap(), 0.);
    assert_eq!(app.scenes[0].pending_results.len(), 0);
    run(&mut app).unwrap();
    run(&mut app).unwrap();
    assert_eq!(*received.borrow(), vec!["finished".to_owned()]);
    assert_eq!(dropped.get(), 1);
    assert_eq!(count(&log, "lower", "result"), 1);
    assert_eq!(count(&log, "lower", "resume"), 1);
    assert_eq!(count(&log, "lower", "enter"), 0);
    assert!(lower_inputs >= 1);
}

#[test]
fn root_pop_drops_unreceived_result_exits_and_empty_pop_is_idempotent() {
    let log = Log::default();
    let dropped = Rc::new(Cell::new(0));
    let mut root = Probe::new("root", log.clone());
    root.pause = Some(Box::new({
        let log = log.clone();
        move |ctx| {
            ctx.push(Probe::new("discarded", log.clone()));
            Ok(())
        }
    }));
    let mut app = app(root);
    run(&mut app).unwrap();
    log.borrow_mut().clear();
    app.pop_scene_with_result(Answer {
        value: "no receiver".into(),
        dropped: dropped.clone(),
    });
    run(&mut app).unwrap();
    assert!(app.is_exited());
    assert_eq!(app.scene_count(), 0);
    assert_eq!(dropped.get(), 1);
    assert_eq!(count(&log, "root", "pause"), 1);
    assert_eq!(count(&log, "root", "drop"), 1);
    assert_eq!(count(&log, "root", "result"), 0);
    assert_eq!(count(&log, "discarded", "enter"), 0);
    assert_eq!(count(&log, "discarded", "drop"), 1);
    assert!(
        app.command.is_none(),
        "leaving callbacks cannot undo an empty-stack exit"
    );
    let stopped = log.borrow().clone();
    app.pop_scene();
    run(&mut app).unwrap();
    app.pop_scene_with_result(Answer {
        value: "still empty".into(),
        dropped: dropped.clone(),
    });
    run(&mut app).unwrap();
    assert_eq!(dropped.get(), 2);
    assert_eq!(*log.borrow(), stopped);
    assert!(app.is_exited());
}

#[test]
fn exit_cancels_ui_then_pauses_top_and_releases_the_whole_stack_top_down() {
    let log = Log::default();
    let mut app = app(Probe::new("root", log.clone()));
    run(&mut app).unwrap();
    app.push_scene(Probe::new("middle", log.clone()));
    run(&mut app).unwrap();
    let mut top = Probe::new("top", log.clone());
    top.pause = Some(Box::new({
        let log = log.clone();
        move |ctx| {
            ctx.push(Probe::new("discarded", log.clone()));
            Ok(())
        }
    }));
    let stats = top.stats.clone();
    let node = ticking_node(log.clone(), "top-node", stats);
    let node_ref = *node;
    app.push_scene(top.with_node(node));
    run(&mut app).unwrap();
    run_input(
        &mut app,
        InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Started, 7, 30., 40., 1.);
        }),
        0.25,
        43.,
    )
    .unwrap();
    log.borrow_mut().clear();
    request_exit(&mut app);
    run(&mut app).unwrap();
    assert_eq!(app.scene_count(), 0);
    assert!(app.is_exited());
    assert!(!alive(node_ref));
    assert!(!app.hit_test_ui(Vector::new(30., 40.)));
    let order = log.borrow();
    let cancel = order
        .iter()
        .position(|entry| *entry == ("top-node", "cancel"))
        .unwrap();
    let pause = order
        .iter()
        .position(|entry| *entry == ("top", "pause"))
        .unwrap();
    let drop_top = order
        .iter()
        .position(|entry| *entry == ("top", "drop"))
        .unwrap();
    assert!(cancel < pause && pause < drop_top);
    drop(order);
    let drops: Vec<_> = log
        .borrow()
        .iter()
        .filter(|(name, phase)| *phase == "drop" && *name != "discarded")
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(drops, vec!["top", "middle", "root"]);
    assert_eq!(count(&log, "root", "pause"), 0);
    assert_eq!(count(&log, "middle", "pause"), 0);
    assert_eq!(count(&log, "root", "resume"), 0);
    assert_eq!(count(&log, "middle", "resume"), 0);
    assert_eq!(count(&log, "discarded", "enter"), 0);
    assert!(app.command.is_none());
    let stopped = log.borrow().clone();
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    assert_eq!(*log.borrow(), stopped);
}

#[test]
fn replace_only_changes_top_while_reset_releases_the_entire_stack() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let log = Log::default();
        let root_node = leaf(Style::new());
        let root_ref = *root_node;
        let mut app = app(Probe::new("root", log.clone()).with_node(root_node));
        run(&mut app).unwrap();
        app.push_scene(Probe::new("old-top", log.clone()));
        run(&mut app).unwrap();
        log.borrow_mut().clear();
        app.replace_scene(Probe::new("replacement", log.clone()));
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), 2);
        assert!(alive(root_ref));
        assert_eq!(
            lifecycle(&log),
            vec![
                ("old-top", "pause"),
                ("old-top", "drop"),
                ("replacement", "enter")
            ]
        );
        app.pop_scene();
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), 1);
        assert_eq!(count(&log, "root", "resume"), 1);
        assert_eq!(count(&log, "root", "enter"), 0);
        assert!(app.displayed[0].node == root_ref);
        app.push_scene(Probe::new("middle", log.clone()));
        run(&mut app).unwrap();
        app.push_scene(Probe::new("top", log.clone()));
        run(&mut app).unwrap();
        log.borrow_mut().clear();
        app.reset_scene(Probe::new("new-root", log.clone()));
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), 1);
        assert!(!alive(root_ref));
        assert_eq!(
            lifecycle(&log),
            vec![
                ("top", "pause"),
                ("top", "drop"),
                ("middle", "drop"),
                ("root", "drop"),
                ("new-root", "enter")
            ]
        );
        request_exit(&mut app);
        run(&mut app).unwrap();
        assert!(app.is_exited());
        app.reset_scene(Probe::new("restarted", log.clone()));
        run(&mut app).unwrap();
        assert!(!app.is_exited());
        assert_eq!(app.scene_count(), 1);
        assert_eq!(count(&log, "restarted", "enter"), 1);
    });
}

#[test]
fn dropping_app_releases_stack_top_down_without_new_lifecycle_callbacks() {
    let log = Log::default();
    let mut app = app(Probe::new("root", log.clone()));
    run(&mut app).unwrap();
    app.push_scene(Probe::new("middle", log.clone()));
    run(&mut app).unwrap();
    app.push_scene(Probe::new("top", log.clone()));
    run(&mut app).unwrap();
    log.borrow_mut().clear();
    drop(app);
    assert_eq!(
        *log.borrow(),
        vec![("top", "drop"), ("middle", "drop"), ("root", "drop")]
    );
}

#[test]
fn host_pause_and_cover_do_not_double_pause_and_pop_result_waits_for_normal_resume() {
    let log = Log::default();
    let lower = Probe::new("lower", log.clone());
    let stats = lower.stats.clone();
    let node = ticking_node(log.clone(), "lower-node", stats.clone());
    let mut app = app(lower.with_node(node));
    run(&mut app).unwrap();
    app.push_scene(Probe::new("upper", log.clone()));
    run(&mut app).unwrap();
    assert_eq!(count(&log, "lower", "pause"), 1);
    paused(&mut app, 100.).unwrap();
    paused(&mut app, 1000.).unwrap();
    assert!(app.host_paused);
    assert_eq!(count(&log, "lower", "pause"), 1);
    assert_eq!(count(&log, "upper", "pause"), 1);
    let updates = stats.scene_dt.borrow().len();
    let node_updates = stats.node_dt.borrow().len();
    log.borrow_mut().clear();
    app.pop_scene_with_result(41_u32);
    paused(&mut app, 10000.).unwrap();
    assert_eq!(app.scene_count(), 1);
    assert!(app.scenes[0].state == SceneState::Paused);
    assert_eq!(app.scenes[0].pending_results.len(), 1);
    assert_eq!(lifecycle(&log), vec![("upper", "drop")]);
    paused(&mut app, 20000.).unwrap();
    assert!(stats.results.borrow().is_empty());
    assert_eq!(stats.scene_dt.borrow().len(), updates);
    assert_eq!(stats.node_dt.borrow().len(), node_updates);
    assert_eq!(count(&log, "lower", "layout"), 0);
    assert_eq!(count(&log, "lower", "render"), 0);
    log.borrow_mut().clear();
    run_input(&mut app, empty(), 10., 30000.).unwrap();
    assert!(!app.host_paused);
    assert_eq!(*stats.results.borrow(), vec![41]);
    assert_eq!(*stats.result_dt.borrow(), vec![0.]);
    assert_eq!(*stats.resume_dt.borrow(), vec![0.]);
    assert_eq!(*stats.scene_dt.borrow().last().unwrap(), 0.);
    assert_eq!(*stats.node_dt.borrow().last().unwrap(), 0.);
    assert_eq!(stats.scene_dt.borrow().len(), updates + 1);
    assert_eq!(stats.node_dt.borrow().len(), node_updates + 1);
    assert_eq!(
        lifecycle(&log),
        vec![("lower", "result"), ("lower", "resume")]
    );
    let order = log.borrow();
    let result = order
        .iter()
        .position(|entry| *entry == ("lower", "result"))
        .unwrap();
    let resume = order
        .iter()
        .position(|entry| *entry == ("lower", "resume"))
        .unwrap();
    let input = order
        .iter()
        .position(|entry| *entry == ("lower", "input"))
        .unwrap();
    assert!(result < resume && resume < input);
    drop(order);
    run_input(&mut app, empty(), 0.375, 30000.375).unwrap();
    assert_eq!(*stats.scene_dt.borrow().last().unwrap(), 0.375);
    assert_eq!(*stats.node_dt.borrow().last().unwrap(), 0.375);
    assert_eq!(count(&log, "lower", "result"), 1);
    assert_eq!(count(&log, "lower", "resume"), 1);
}

#[test]
fn paused_new_layers_have_no_fake_lifecycle_and_their_unhandled_results_drop_with_them() {
    let log = Log::default();
    let dropped = Rc::new(Cell::new(0));
    let root = Probe::new("root", log.clone());
    let stats = root.stats.clone();
    let mut app = app(root);
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    log.borrow_mut().clear();
    app.push_scene(Probe::new("new-middle", log.clone()));
    paused(&mut app, 200.).unwrap();
    app.push_scene(Probe::new("new-top", log.clone()));
    paused(&mut app, 300.).unwrap();
    assert_eq!(app.scene_count(), 3);
    assert!(app.scenes[1].state == SceneState::New);
    assert!(app.scenes[2].state == SceneState::New);
    assert!(log.borrow().is_empty());
    app.pop_scene_with_result(Answer {
        value: "never handled".into(),
        dropped: dropped.clone(),
    });
    paused(&mut app, 400.).unwrap();
    assert_eq!(app.scenes[1].pending_results.len(), 1);
    assert_eq!(dropped.get(), 0);
    app.pop_scene_with_result(9_u32);
    paused(&mut app, 500.).unwrap();
    assert_eq!(dropped.get(), 1);
    assert_eq!(app.scene_count(), 1);
    assert_eq!(
        lifecycle(&log),
        vec![("new-top", "drop"), ("new-middle", "drop")]
    );
    assert!(stats.results.borrow().is_empty());
    run(&mut app).unwrap();
    assert_eq!(*stats.results.borrow(), vec![9]);
    assert_eq!(count(&log, "root", "resume"), 1);
    assert_eq!(count(&log, "root", "enter"), 0);
    assert_eq!(count(&log, "root", "pause"), 0);
    for name in ["new-middle", "new-top"] {
        for phase in [
            "enter", "pause", "resume", "result", "input", "update", "layout", "render",
        ] {
            assert_eq!(count(&log, name, phase), 0, "unentered {name}: {phase}");
        }
    }
}

#[test]
fn multiple_results_queued_while_paused_are_fifo_once_and_all_before_resume() {
    let log = Log::default();
    let root = Probe::new("root", log.clone());
    let stats = root.stats.clone();
    let mut app = app(root);
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    log.borrow_mut().clear();
    for value in [3_u32, 1, 4] {
        app.push_scene(Probe::new("unentered", log.clone()));
        paused(&mut app, 200.).unwrap();
        app.pop_scene_with_result(value);
        paused(&mut app, 300.).unwrap();
    }
    assert_eq!(app.scene_count(), 1);
    assert_eq!(app.scenes[0].pending_results.len(), 3);
    assert!(stats.results.borrow().is_empty());
    assert_eq!(count(&log, "root", "resume"), 0);
    run_input(&mut app, empty(), 99., 10000.).unwrap();
    assert_eq!(*stats.results.borrow(), vec![3, 1, 4]);
    assert_eq!(*stats.result_dt.borrow(), vec![0., 0., 0.]);
    assert_eq!(app.scenes[0].pending_results.len(), 0);
    let callbacks: Vec<_> = lifecycle(&log)
        .into_iter()
        .filter(|(name, _)| *name == "root")
        .collect();
    assert_eq!(
        callbacks,
        vec![
            ("root", "result"),
            ("root", "result"),
            ("root", "result"),
            ("root", "resume")
        ]
    );
    run(&mut app).unwrap();
    assert_eq!(*stats.results.borrow(), vec![3, 1, 4]);
    assert_eq!(count(&log, "root", "resume"), 1);
    assert_eq!(count(&log, "unentered", "enter"), 0);
}

#[test]
fn continuing_raw_touch_survives_push_pop_but_canceled_ui_capture_never_revives() {
    let log = Log::default();
    let push_now = Rc::new(Cell::new(false));
    let pop_now = Rc::new(Cell::new(false));
    let mut lower = Probe::new("lower", log.clone());
    let lower_stats = lower.stats.clone();
    let lower_node = ticking_node(log.clone(), "lower-node", lower_stats.clone());
    let mut upper = Probe::new("upper", log.clone());
    let upper_stats = upper.stats.clone();
    let upper_node = ticking_node(log.clone(), "upper-node", upper_stats.clone());
    upper.input = Some(Box::new({
        let pop_now = pop_now.clone();
        move |_, ctx| {
            if pop_now.replace(false) {
                ctx.pop();
            }
            Ok(())
        }
    }));
    lower.input = Some(Box::new({
        let push_now = push_now.clone();
        let mut upper = Some(upper.with_node(upper_node));
        move |_, ctx| {
            if push_now.replace(false) {
                ctx.push(upper.take().unwrap());
            }
            Ok(())
        }
    }));
    let mut app = app(lower.with_node(lower_node));
    run(&mut app).unwrap();
    push_now.set(true);
    let down = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 1, 31.25, 42.5, 12.125);
        events.char_event('a', Default::default(), false);
    });
    let expected_down = down.events().to_vec();
    run_input(&mut app, down, 0.25, 43.).unwrap();
    assert_raw(&lower_stats.raw.borrow(), &expected_down);
    assert!(upper_stats.raw.borrow().is_empty());
    assert_eq!(count(&log, "lower-node", "down"), 1);
    assert_eq!(count(&log, "lower-node", "cancel"), 1);
    assert_eq!(count(&log, "upper-node", "down"), 0);
    let order = log.borrow();
    let cancel = order
        .iter()
        .position(|entry| *entry == ("lower-node", "cancel"))
        .unwrap();
    let pause = order
        .iter()
        .position(|entry| *entry == ("lower", "pause"))
        .unwrap();
    let enter = order
        .iter()
        .position(|entry| *entry == ("upper", "enter"))
        .unwrap();
    assert!(cancel < pause && pause < enter);
    drop(order);

    let continuation = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 53.75, 54.25, 12.375);
        events.touch_event(TouchPhase::Started, 2, 35., 45., 12.5);
        events.char_event('b', Default::default(), true);
    });
    let expected_continuation = continuation.events().to_vec();
    run_input(&mut app, continuation, 0.375, 44.).unwrap();
    assert_raw(&upper_stats.raw.borrow(), &expected_continuation);
    assert_eq!(
        count(&log, "upper-node", "down"),
        1,
        "only actual new touch 2 may capture UI"
    );
    assert_eq!(
        count(&log, "upper-node", "move"),
        0,
        "touch 1 never had a Down in upper UI"
    );
    assert_eq!(count(&log, "lower-node", "move"), 0);
    pop_now.set(true);
    let pop_batch = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 60.5, 61.25, 12.75);
        events.touch_event(TouchPhase::Moved, 2, 36., 46., 12.875);
    });
    let mut expected_upper = expected_continuation;
    expected_upper.extend_from_slice(pop_batch.events());
    run_input(&mut app, pop_batch, 2., 100.).unwrap();
    assert_raw(&upper_stats.raw.borrow(), &expected_upper);
    assert_raw(&lower_stats.raw.borrow(), &expected_down);
    assert_eq!(count(&log, "upper-node", "cancel"), 1);
    assert_eq!(count(&log, "lower-node", "cancel"), 1);
    assert_eq!(count(&log, "lower-node", "down"), 1);
    assert_eq!(count(&log, "lower-node", "move"), 0);
    assert_eq!(count(&log, "lower-node", "up"), 0);
    let resumed_batch = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 73.75, 74.25, 13.125);
        events.touch_event(TouchPhase::Ended, 1, 76.5, 77.25, 13.25);
        events.touch_event(TouchPhase::Ended, 2, 37., 47., 13.375);
        events.touch_event(TouchPhase::Moved, 42, 11.5, 12.75, 13.5);
        events.touch_event(TouchPhase::Ended, 42, 13.5, 14.75, 13.625);
        events.char_event('续', Default::default(), false);
    });
    let mut expected_lower = expected_down;
    expected_lower.extend_from_slice(resumed_batch.events());
    run_input(&mut app, resumed_batch, 0.5, 101.).unwrap();
    assert_raw(&lower_stats.raw.borrow(), &expected_lower);
    assert_eq!(
        count(&log, "lower-node", "down"),
        1,
        "no synthetic Down after uncovering"
    );
    assert_eq!(
        count(&log, "lower-node", "move"),
        0,
        "old UI capture stays canceled"
    );
    assert_eq!(count(&log, "lower-node", "up"), 0);
    run_input(
        &mut app,
        InputBatch::recorded(|events| {
            events.touch_event(TouchPhase::Started, 3, 30., 40., 14.);
            events.touch_event(TouchPhase::Ended, 3, 30., 40., 14.125);
        }),
        0.25,
        102.,
    )
    .unwrap();
    assert_eq!(count(&log, "lower-node", "down"), 2);
    assert_eq!(count(&log, "lower-node", "up"), 1);
}

#[test]
fn commands_from_commit_pause_enter_result_resume_are_deferred_and_last_request_wins() {
    for phase in ["pause", "enter", "result", "resume"] {
        let log = Log::default();
        let mut lower = Probe::new("lower", log.clone());
        let mut upper = Probe::new("upper", log.clone());
        let request = || -> ContextHook {
            Box::new({
                let log = log.clone();
                move |ctx| {
                    ctx.exit();
                    ctx.replace(Probe::new("obsolete", log.clone()));
                    ctx.push(Probe::new("deferred", log.clone()));
                    Ok(())
                }
            })
        };
        if phase == "resume" {
            lower.resume = Some(request());
        }
        if phase == "result" {
            let mut hook = request();
            lower.result = Some(Box::new(move |_, ctx| hook(ctx)));
        }
        if phase == "pause" {
            lower.pause = Some(request());
        }
        let mut app = app(lower);
        run(&mut app).unwrap();
        if phase == "enter" {
            upper.enter = Some(request());
        }
        app.push_scene(upper);
        log.borrow_mut().clear();
        run(&mut app).unwrap();
        if matches!(phase, "result" | "resume") {
            app.pop_scene_with_result(7_u32);
            log.borrow_mut().clear();
            run(&mut app).unwrap();
        }
        let current_count = if matches!(phase, "pause" | "enter") {
            2
        } else {
            1
        };
        assert_eq!(
            app.scene_count(),
            current_count,
            "{phase}: one topology commit per frame"
        );
        assert_eq!(
            count(&log, "deferred", "enter"),
            0,
            "{phase}: boundary requests wait"
        );
        assert_eq!(count(&log, "obsolete", "enter"), 0);
        assert_eq!(count(&log, "obsolete", "drop"), 1);
        assert!(
            app.command.is_some(),
            "{phase}: keep the last boundary request"
        );
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), current_count + 1);
        assert_eq!(count(&log, "deferred", "enter"), 1);
        assert_eq!(count(&log, "deferred", "input"), 0);
        assert_eq!(count(&log, "deferred", "update"), 0);
    }
}

#[test]
fn input_phase_enter_or_host_resume_can_request_one_commit_but_nested_enter_waits() {
    // Initial activation is before raw input, unlike activation inside an already
    // selected topology commit. It can select this frame's one safe commit.
    for phase in ["enter", "resume"] {
        let log = Log::default();
        let mut root = Probe::new("root", log.clone());
        let request: ContextHook = Box::new({
            let log = log.clone();
            move |ctx| {
                let mut activated = Probe::new("activated", log.clone());
                activated.enter = Some(Box::new({
                    let log = log.clone();
                    move |ctx| {
                        ctx.push(Probe::new("deferred", log.clone()));
                        Ok(())
                    }
                }));
                ctx.push(activated);
                Ok(())
            }
        });
        if phase == "enter" {
            root.enter = Some(request);
        } else {
            root.resume = Some(request);
        }
        let mut app = app(root);
        if phase == "resume" {
            run(&mut app).unwrap();
            paused(&mut app, 100.).unwrap();
        }
        log.borrow_mut().clear();
        run_input(&mut app, empty(), 4., 1000.).unwrap();
        assert_eq!(app.scene_count(), 2, "{phase}: only one topology commit");
        assert_eq!(count(&log, "root", "input"), 1);
        assert_eq!(count(&log, "activated", "enter"), 1);
        assert_eq!(count(&log, "activated", "input"), 0);
        assert_eq!(count(&log, "activated", "update"), 0);
        assert_eq!(count(&log, "deferred", "enter"), 0);
        assert!(app.command.is_some());
        run(&mut app).unwrap();
        assert_eq!(app.scene_count(), 3);
        assert_eq!(count(&log, "deferred", "enter"), 1);
        assert_eq!(count(&log, "deferred", "input"), 0);
    }
}

#[test]
fn host_activation_results_and_resume_select_one_commit_and_the_later_resume_wins() {
    let log = Log::default();
    let mut root = Probe::new("root", log.clone());
    root.result = Some(Box::new({
        let log = log.clone();
        move |_, ctx| {
            ctx.push(Probe::new("from-result", log.clone()));
            Ok(())
        }
    }));
    root.resume = Some(Box::new({
        let log = log.clone();
        move |ctx| {
            ctx.push(Probe::new("from-resume", log.clone()));
            Ok(())
        }
    }));
    let mut app = app(root);
    run(&mut app).unwrap();
    app.push_scene(Probe::new("upper", log.clone()));
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    app.pop_scene_with_result(1_u32);
    paused(&mut app, 200.).unwrap();
    log.borrow_mut().clear();
    run(&mut app).unwrap();
    assert_eq!(
        app.scene_count(),
        2,
        "input-phase activation selects only one commit"
    );
    assert_eq!(count(&log, "root", "result"), 1);
    assert_eq!(count(&log, "root", "resume"), 1);
    assert_eq!(count(&log, "root", "input"), 1);
    assert_eq!(count(&log, "from-result", "drop"), 1);
    assert_eq!(count(&log, "from-result", "enter"), 0);
    assert_eq!(count(&log, "from-resume", "enter"), 1);
    assert_eq!(count(&log, "from-resume", "input"), 0);
    assert_eq!(count(&log, "from-resume", "update"), 0);
    run(&mut app).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(count(&log, "from-resume", "enter"), 1);
    assert_eq!(count(&log, "from-resume", "update"), 1);
}

#[test]
fn a_paused_pause_request_waits_even_when_an_earlier_host_command_commits() {
    for already_queued in [false, true] {
        let log = Log::default();
        let mut root = Probe::new("root", log.clone());
        root.pause = Some(Box::new({
            let log = log.clone();
            move |ctx| {
                ctx.push(Probe::new("deferred", log.clone()));
                Ok(())
            }
        }));
        let mut app = app(root);
        run(&mut app).unwrap();
        log.borrow_mut().clear();
        if already_queued {
            app.push_scene(Probe::new("selected", log.clone()));
        }
        paused(&mut app, 100.).unwrap();
        assert_eq!(app.scene_count(), if already_queued { 2 } else { 1 });
        assert_eq!(count(&log, "deferred", "enter"), 0);
        assert_eq!(count(&log, "selected", "enter"), 0);
        assert!(app.command.is_some());
        paused(&mut app, 200.).unwrap();
        assert_eq!(app.scene_count(), if already_queued { 3 } else { 2 });
        assert_eq!(count(&log, "deferred", "enter"), 0);
        assert_eq!(count(&log, "root", "pause"), 1);
        run(&mut app).unwrap();
        assert_eq!(count(&log, "deferred", "enter"), 1);
        assert_eq!(
            count(&log, "selected", "enter"),
            0,
            "covered New scene stays unentered"
        );
    }
}

#[test]
fn the_last_input_request_overwrites_prior_host_and_input_requests() {
    let log = Log::default();
    let mut root = Probe::new("root", log.clone());
    root.input = Some(Box::new({
        let log = log.clone();
        let mut once = true;
        move |_, ctx| {
            if std::mem::take(&mut once) {
                ctx.push(Probe::new("first-input", log.clone()));
                ctx.replace(Probe::new("last-input", log.clone()));
            }
            Ok(())
        }
    }));
    let mut app = app(root);
    app.push_scene(Probe::new("host-request", log.clone()));
    run(&mut app).unwrap();
    assert_eq!(app.scene_count(), 1);
    assert_eq!(count(&log, "host-request", "drop"), 1);
    assert_eq!(count(&log, "host-request", "enter"), 0);
    assert_eq!(count(&log, "first-input", "drop"), 1);
    assert_eq!(count(&log, "first-input", "enter"), 0);
    assert_eq!(count(&log, "last-input", "enter"), 1);
}

#[test]
fn host_resume_accepts_new_ui_down_and_full_raw_continuations_without_reviving_capture() {
    let log = Log::default();
    let root = Probe::new("root", log.clone());
    let stats = root.stats.clone();
    let node = ticking_node(log.clone(), "node", stats.clone());
    let mut app = app(root.with_node(node));
    run(&mut app).unwrap();
    let down = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Started, 1, 30., 40., 1.);
    });
    let mut expected = down.events().to_vec();
    run_input(&mut app, down, 0.25, 43.).unwrap();
    paused(&mut app, 1000.).unwrap();
    paused(&mut app, 2000.).unwrap();
    assert_eq!(count(&log, "node", "cancel"), 1);
    let batch = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 1, 31.25, 41.5, 1.25);
        events.touch_event(TouchPhase::Ended, 1, 32.25, 42.5, 1.5);
        events.touch_event(TouchPhase::Started, 2, 35., 45., 1.75);
        events.touch_event(TouchPhase::Ended, 2, 35., 45., 2.);
    });
    expected.extend_from_slice(batch.events());
    app.run_checked_frame(
        batch,
        frame(17., 10000.),
        || {},
        |_, actual| {
            assert_eq!(
                actual.dt, 17.,
                "only Scene/Node time is reset, not global time"
            );
            assert_eq!(actual.time, 10000.);
        },
    )
    .unwrap();
    assert_raw(&stats.raw.borrow(), &expected);
    assert_eq!(
        count(&log, "node", "down"),
        2,
        "first resumed frame accepts fresh UI Down"
    );
    assert_eq!(count(&log, "node", "up"), 1);
    assert_eq!(count(&log, "node", "move"), 0, "old capture cannot revive");
    assert_eq!(count(&log, "node", "cancel"), 1);
    assert_eq!(*stats.resume_dt.borrow(), vec![0.]);
    assert_eq!(*stats.scene_dt.borrow().last().unwrap(), 0.);
    assert_eq!(*stats.node_dt.borrow().last().unwrap(), 0.);
}

#[test]
fn scenes_created_before_host_resume_enter_only_when_top_and_never_fake_pause_resume() {
    let log = Log::default();
    let mut app = app(Probe::new("new-root", log.clone()));
    paused(&mut app, 100.).unwrap();
    paused(&mut app, 200.).unwrap();
    assert!(log.borrow().is_empty());
    app.push_scene(Probe::new("new-top", log.clone()));
    paused(&mut app, 300.).unwrap();
    assert!(log.borrow().is_empty());
    assert!(
        app.scenes
            .iter()
            .all(|entry| entry.state == SceneState::New)
    );
    run(&mut app).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(count(&log, "new-top", "enter"), 1);
    assert_eq!(count(&log, "new-top", "resume"), 0);
    assert_eq!(count(&log, "new-top", "pause"), 0);
    assert_eq!(count(&log, "new-root", "enter"), 0);
    assert_eq!(count(&log, "new-root", "pause"), 0);
    assert!(app.scenes[0].state == SceneState::New);
    app.pop_scene();
    run(&mut app).unwrap();
    assert_eq!(app.scene_count(), 1);
    assert_eq!(count(&log, "new-root", "enter"), 1);
    assert_eq!(count(&log, "new-root", "resume"), 0);
    assert_eq!(count(&log, "new-root", "pause"), 0);
}

#[test]
fn pause_errors_abort_push_pop_replace_reset_exit_before_mutation_and_never_retry_pause() {
    for command in ["push", "pop", "replace", "reset", "exit", "host-pause"] {
        let log = Log::default();
        let mut root = Probe::new("failed", log.clone());
        root.fail = Some("pause");
        root.pause = Some(Box::new({
            let log = log.clone();
            move |ctx| {
                ctx.push(Probe::new("callback-request", log.clone()));
                Ok(())
            }
        }));
        let stats = root.stats.clone();
        let node = ticking_node(log.clone(), "node", stats);
        let node_ref = *node;
        let mut app = app(root.with_node(node));
        run(&mut app).unwrap();
        run_input(
            &mut app,
            InputBatch::recorded(|events| {
                events.touch_event(TouchPhase::Started, 1, 30., 40., 1.);
            }),
            0.25,
            43.,
        )
        .unwrap();
        let result_drops = Rc::new(Cell::new(0));
        match command {
            "push" => {
                app.push_scene(Probe::new("selected", log.clone()));
            }
            "pop" => {
                app.pop_scene_with_result(Answer {
                    value: "aborted".into(),
                    dropped: result_drops.clone(),
                });
            }
            "replace" => {
                app.replace_scene(Probe::new("selected", log.clone()));
            }
            "reset" => {
                app.reset_scene(Probe::new("selected", log.clone()));
            }
            "exit" => request_exit(&mut app),
            "host-pause" => {}
            _ => unreachable!(),
        }
        log.borrow_mut().clear();
        let globals = Cell::new(0);
        let result = if command == "host-pause" {
            app.run_checked_paused_frame(frame(4., 100.), |_| globals.set(globals.get() + 1))
        } else {
            app.run_checked_frame(
                empty(),
                frame(0.25, 100.),
                || {},
                |_, _| globals.set(globals.get() + 1),
            )
        };
        let error = result.unwrap_err();
        assert!(
            format!("{error:#}").contains("pause"),
            "{command}: {error:#}"
        );
        assert!(app.is_faulted(), "{command}");
        assert_eq!(
            app.scene_count(),
            1,
            "{command}: no implicit rollback or partial topology mutation"
        );
        assert!(app.scenes[0].state == SceneState::Paused);
        assert!(alive(node_ref));
        assert!(app.command.is_none());
        assert!(app.displayed.is_empty());
        assert!(!app.hit_test_ui(Vector::new(30., 40.)));
        assert_eq!(
            globals.get(),
            1,
            "{command}: global work once, including failed paused frame"
        );
        assert_eq!(count(&log, "node", "cancel"), 1);
        assert_eq!(count(&log, "failed", "pause"), 1);
        assert_eq!(count(&log, "callback-request", "enter"), 0);
        assert_eq!(count(&log, "callback-request", "drop"), 1);
        if command == "pop" {
            assert_eq!(result_drops.get(), 1);
        }
        let stopped = scheduled(&log);
        assert!(run(&mut app).is_err());
        assert!(paused(&mut app, 200.).is_err());
        assert_eq!(scheduled(&log), stopped);
        app.replace_scene(Probe::new("recovered", log.clone()));
        run(&mut app).unwrap();
        assert!(!app.is_faulted());
        assert_eq!(app.scene_count(), 1);
        assert_eq!(
            count(&log, "failed", "pause"),
            1,
            "fault recovery must not call the failed old pause"
        );
        assert_eq!(count(&log, "failed", "drop"), 1);
        assert_eq!(count(&log, "recovered", "enter"), 1);
        assert!(!alive(node_ref));
    }
}

#[test]
fn boundary_enter_result_resume_errors_stop_all_scenes_until_replace_reset_or_exit() {
    for phase in ["enter", "result", "resume"] {
        for recovery in ["replace", "reset", "exit"] {
            let log = Log::default();
            let mut lower = Probe::new("lower", log.clone());
            if phase != "enter" {
                lower.fail = Some(phase);
            }
            let lower_node = ticking_node(log.clone(), "lower-node", lower.stats.clone());
            let lower_ref = *lower_node;
            let mut upper = Probe::new("upper", log.clone());
            if phase == "enter" {
                upper.fail = Some("enter");
                upper.enter = Some(Box::new({
                    let log = log.clone();
                    move |ctx| {
                        ctx.replace(Probe::new("discarded", log.clone()));
                        Ok(())
                    }
                }));
            } else if phase == "resume" {
                lower.resume = Some(Box::new({
                    let log = log.clone();
                    move |ctx| {
                        ctx.replace(Probe::new("discarded", log.clone()));
                        Ok(())
                    }
                }));
            } else {
                lower.result = Some(Box::new({
                    let log = log.clone();
                    move |_, ctx| {
                        ctx.replace(Probe::new("discarded", log.clone()));
                        Ok(())
                    }
                }));
            }
            let upper_node = ticking_node(log.clone(), "upper-node", upper.stats.clone());
            let upper_ref = *upper_node;
            let mut app = app(lower.with_node(lower_node));
            run(&mut app).unwrap();
            app.push_scene(upper.with_node(upper_node));
            let failed = if phase == "enter" {
                run(&mut app)
            } else {
                run(&mut app).unwrap();
                app.pop_scene_with_result(1_u32);
                run(&mut app)
            };
            let error = failed.unwrap_err();
            assert!(
                format!("{error:#}").contains(phase),
                "{phase}/{recovery}: {error:#}"
            );
            assert!(app.is_faulted());
            assert_eq!(app.scene_count(), if phase == "enter" { 2 } else { 1 });
            assert!(alive(lower_ref));
            assert_eq!(alive(upper_ref), phase == "enter");
            assert!(app.command.is_none(), "fault discards lifecycle requests");
            assert!(app.displayed.is_empty());
            assert_eq!(count(&log, "discarded", "drop"), 1);
            assert_eq!(count(&log, "discarded", "enter"), 0);
            if phase == "result" {
                assert!(app.scenes[0].state == SceneState::Paused);
                assert_eq!(count(&log, "lower", "resume"), 0);
                assert_eq!(
                    app.scenes[0].pending_results.len(),
                    0,
                    "failed result is consumed, not retried"
                );
            } else if phase == "resume" {
                assert!(app.scenes[0].state == SceneState::Running);
                assert_eq!(count(&log, "lower", "result"), 1);
                assert_eq!(count(&log, "lower", "resume"), 1);
            } else {
                assert!(app.scenes[0].state == SceneState::Paused);
                assert!(app.scenes[1].state == SceneState::Running);
                assert_eq!(count(&log, "upper", "enter"), 1);
            }
            let stopped = scheduled(&log);
            let scene_count = app.scene_count();
            assert!(run(&mut app).is_err());
            let paused_globals = Cell::new(0);
            assert!(
                app.run_checked_paused_frame(frame(0., 1000.), |_| paused_globals
                    .set(paused_globals.get() + 1))
                    .is_err()
            );
            assert_eq!(paused_globals.get(), 1);
            app.push_scene(Probe::new("forbidden-push", log.clone()));
            assert!(run(&mut app).is_err());
            app.pop_scene();
            assert!(run(&mut app).is_err());
            assert_eq!(app.scene_count(), scene_count);
            assert!(app.is_faulted());
            assert_eq!(
                scheduled(&log),
                stopped,
                "fault blocks every Scene/Node callback"
            );
            assert_eq!(count(&log, "forbidden-push", "enter"), 0);
            assert_eq!(count(&log, "forbidden-push", "drop"), 1);
            let pauses = count(&log, "lower", "pause") + count(&log, "upper", "pause");
            match recovery {
                "replace" => {
                    app.replace_scene(Probe::new("recovered", log.clone()));
                }
                "reset" => {
                    app.reset_scene(Probe::new("recovered", log.clone()));
                }
                "exit" => request_exit(&mut app),
                _ => unreachable!(),
            }
            run(&mut app).unwrap();
            assert!(!app.is_faulted());
            assert_eq!(
                count(&log, "lower", "pause") + count(&log, "upper", "pause"),
                pauses
            );
            if recovery == "exit" {
                assert!(app.is_exited());
                assert_eq!(app.scene_count(), 0);
            } else {
                assert_eq!(count(&log, "recovered", "enter"), 1);
                assert_eq!(
                    app.scene_count(),
                    if recovery == "replace" {
                        scene_count
                    } else {
                        1
                    }
                );
                assert_eq!(count(&log, "recovered", "input"), 0);
                assert_eq!(count(&log, "recovered", "update"), 0);
            }
            assert_eq!(alive(lower_ref), phase == "enter" && recovery == "replace");
            assert!(!alive(upper_ref));
        }
    }
}

#[test]
fn failed_result_is_consumed_once_remaining_fifo_results_wait_and_drop_on_reset() {
    let log = Log::default();
    let received = Rc::new(RefCell::new(Vec::new()));
    let dropped = Rc::new(Cell::new(0));
    let mut root = Probe::new("root", log.clone());
    root.result = Some(Box::new({
        let received = received.clone();
        move |result, _| {
            let result = result.downcast::<Answer>().ok().unwrap();
            received.borrow_mut().push(result.value.clone());
            bail!("deliberate result failure");
        }
    }));
    let mut app = app(root);
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    for value in ["first", "second", "third"] {
        app.push_scene(Probe::new("unentered", log.clone()));
        paused(&mut app, 200.).unwrap();
        app.pop_scene_with_result(Answer {
            value: value.into(),
            dropped: dropped.clone(),
        });
        paused(&mut app, 300.).unwrap();
    }
    assert_eq!(app.scenes[0].pending_results.len(), 3);
    assert!(run(&mut app).is_err());
    assert!(app.is_faulted());
    assert_eq!(*received.borrow(), vec!["first".to_owned()]);
    assert_eq!(dropped.get(), 1);
    assert_eq!(app.scenes[0].pending_results.len(), 2);
    assert_eq!(count(&log, "root", "resume"), 0);
    assert!(run(&mut app).is_err());
    assert_eq!(dropped.get(), 1);
    assert_eq!(*received.borrow(), vec!["first".to_owned()]);
    app.reset_scene(Probe::new("recovered", log.clone()));
    run(&mut app).unwrap();
    assert_eq!(dropped.get(), 3);
    assert_eq!(*received.borrow(), vec!["first".to_owned()]);
    assert_eq!(count(&log, "root", "result"), 1);
    assert_eq!(count(&log, "root", "resume"), 0);
}

struct EveryTick {
    polls: Rc<Cell<usize>>,
    dropped: Rc<Cell<usize>>,
}

impl Future for EveryTick {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        self.polls.set(self.polls.get() + 1);
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

impl Drop for EveryTick {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}

#[test]
fn caller_owned_task_continues_under_cover_and_pause_and_scene_drop_cancels_it() {
    let log = Log::default();
    let polls = Rc::new(Cell::new(0));
    let task_drops = Rc::new(Cell::new(0));
    let globals = Cell::new(0);
    let host_updates = Cell::new(0);
    let mut lower = Probe::new("lower", log.clone());
    lower.task = Some(task::spawn(EveryTick {
        polls: polls.clone(),
        dropped: task_drops.clone(),
    }));
    let stats = lower.stats.clone();
    let node = ticking_node(log.clone(), "lower-node", stats.clone());
    let mut app = app(lower.with_node(node));
    let normal = |app: &mut App, dt, time| {
        app.run_checked_frame(
            empty(),
            frame(dt, time),
            || host_updates.set(host_updates.get() + 1),
            |_, actual| {
                assert_eq!(actual.dt, dt);
                assert_eq!(actual.time, time);
                globals.set(globals.get() + 1);
                task::tick(32);
            },
        )
    };
    let suspended = |app: &mut App, time| {
        app.run_checked_paused_frame(frame(0., time), |_| {
            globals.set(globals.get() + 1);
            task::tick(32);
        })
    };
    normal(&mut app, 0.125, 1.).unwrap();
    assert_eq!(polls.get(), 1);
    app.push_scene(Probe::new("upper", log.clone()));
    normal(&mut app, 0.25, 2.).unwrap();
    normal(&mut app, 0.375, 3.).unwrap();
    assert_eq!(
        polls.get(),
        3,
        "coverage does not cancel the caller-owned task"
    );
    assert_eq!(*stats.scene_dt.borrow(), vec![0.125]);
    assert_eq!(*stats.node_dt.borrow(), vec![0.125]);
    suspended(&mut app, 100.).unwrap();
    suspended(&mut app, 1000.).unwrap();
    app.pop_scene();
    suspended(&mut app, 10000.).unwrap();
    assert_eq!(polls.get(), 6);
    assert_eq!(
        globals.get(),
        6,
        "global task tick is per App, not per Scene"
    );
    assert_eq!(
        host_updates.get(),
        3,
        "paused frames do not invoke normal host update"
    );
    assert_eq!(*stats.scene_dt.borrow(), vec![0.125]);
    assert_eq!(*stats.node_dt.borrow(), vec![0.125]);
    normal(&mut app, 9., 20000.).unwrap();
    assert_eq!(polls.get(), 7);
    assert_eq!(*stats.scene_dt.borrow(), vec![0.125, 0.]);
    assert_eq!(*stats.node_dt.borrow(), vec![0.125, 0.]);
    assert_eq!(task_drops.get(), 0);
    app.reset_scene(Probe::new("replacement", log.clone()));
    normal(&mut app, 0.5, 20001.).unwrap();
    assert_eq!(
        polls.get(),
        8,
        "selected reset follows this frame's one global tick"
    );
    assert_eq!(
        task_drops.get(),
        1,
        "dropping its owner destroys/cancels the future"
    );
    normal(&mut app, 0.75, 20002.).unwrap();
    suspended(&mut app, 20003.).unwrap();
    assert_eq!(globals.get(), 10);
    assert_eq!(polls.get(), 8, "a canceled task cannot run again");
    assert_eq!(task_drops.get(), 1);
}

#[test]
fn failed_paused_frame_still_ticks_caller_tasks_once_and_recovery_drop_cancels_them() {
    let log = Log::default();
    let polls = Rc::new(Cell::new(0));
    let task_drops = Rc::new(Cell::new(0));
    let globals = Cell::new(0);
    let mut root = Probe::new("root", log.clone());
    root.fail = Some("pause");
    root.task = Some(task::spawn(EveryTick {
        polls: polls.clone(),
        dropped: task_drops.clone(),
    }));
    let mut app = app(root);
    run(&mut app).unwrap();
    for time in [100., 200.] {
        assert!(
            app.run_checked_paused_frame(frame(0., time), |_| {
                globals.set(globals.get() + 1);
                task::tick(32);
            })
            .is_err()
        );
    }
    assert_eq!(globals.get(), 2);
    assert_eq!(polls.get(), 2, "each failed paused frame has one task pass");
    assert_eq!(count(&log, "root", "pause"), 1);
    assert_eq!(task_drops.get(), 0);
    app.replace_scene(Probe::new("recovered", log.clone()));
    app.run_checked_paused_frame(frame(0., 300.), |_| {
        globals.set(globals.get() + 1);
        task::tick(32);
    })
    .unwrap();
    assert_eq!(polls.get(), 3);
    assert_eq!(task_drops.get(), 1);
    assert_eq!(count(&log, "recovered", "enter"), 0);
    run(&mut app).unwrap();
    assert_eq!(count(&log, "recovered", "enter"), 1);
    assert_eq!(polls.get(), 3);
}

fn signal_node() -> (Node, Signal<u32>) {
    let local = Rc::new(Cell::new(None));
    let node = Node::leaf(Style::new().width(80.).height(80.), {
        let local = local.clone();
        move |style| {
            use_layout(style);
            local.set(Some(signal(17_u32)));
        }
    });
    (node, local.get().unwrap())
}

#[test]
fn application_owned_global_survives_scene_pop_replace_and_exit_cleanup() {
    for removal in ["pop", "replace", "exit"] {
        // The application-level owner is outside every Scene and outlives App.
        let global_owner = OwnedScope::new();
        let global = global_owner.run(|| signal(17_u32));
        let log = Log::default();
        let local_handle = Rc::new(Cell::new(None::<Signal<u32>>));
        let cleaned = Rc::new(RefCell::new(Vec::new()));
        let node = Node::leaf(Style::new().width(80.).height(80.), {
            let local_handle = local_handle.clone();
            let cleaned = cleaned.clone();
            move |style| {
                use_layout(style);
                let local = signal(7_u32);
                local_handle.set(Some(local));
                // Register teardown only: a global change must not rerun cleanup.
                watch_effect(move || {
                    let cleaned = cleaned.clone();
                    on_cleanup(move || {
                        assert!(!global.is_disposed());
                        assert!(!local.is_disposed());
                        cleaned
                            .borrow_mut()
                            .push((global.get_untracked(), local.get_untracked()));
                    });
                });
            }
        });
        let node_ref = *node;
        let local = local_handle.get().unwrap();
        let mut app = app(Probe::new("root", log.clone()));
        run(&mut app).unwrap();
        app.push_scene(Probe::new("settings", log.clone()).with_node(node));
        run(&mut app).unwrap();
        global.set(23);
        assert!(
            cleaned.borrow().is_empty(),
            "cleanup belongs to Scene teardown"
        );

        match removal {
            "pop" => {
                app.pop_scene();
            }
            "replace" => {
                app.replace_scene(Probe::new("replacement", log.clone()));
            }
            "exit" => request_exit(&mut app),
            _ => unreachable!(),
        }
        run(&mut app).unwrap();
        assert_eq!(*cleaned.borrow(), vec![(23, 7)], "{removal}");
        assert_eq!(count(&log, "settings", "drop"), 1);
        assert!(!alive(node_ref));
        assert!(local.is_disposed());
        assert_eq!(local.try_get(), None);
        assert_eq!(global.try_get(), Some(23), "{removal} cannot own global");

        drop(app);
        assert_eq!(global.try_get(), Some(23));
        drop(global_owner);
        assert!(global.is_disposed());
        assert_eq!(global.try_get(), None);
        assert_eq!(*cleaned.borrow(), vec![(23, 7)]);
    }
}

#[test]
fn covered_scene_resumes_same_instance_with_updated_application_owned_global() {
    let global_owner = OwnedScope::new();
    let global = global_owner.run(|| signal(17_u32));
    let log = Log::default();
    let (node, local) = signal_node();
    let node_ref = *node;
    local.set(9);
    let reads = Rc::new(RefCell::new(Vec::new()));
    let resumed = Rc::new(RefCell::new(Vec::new()));
    let mut lower = Probe::new("retained", log.clone());
    let stats = lower.stats.clone();
    lower.update = Some(Box::new({
        let reads = reads.clone();
        move |_| {
            reads.borrow_mut().push((global.get(), local.get()));
            Ok(())
        }
    }));
    lower.resume = Some(Box::new({
        let resumed = resumed.clone();
        move |_| {
            resumed.borrow_mut().push((global.get(), local.get()));
            Ok(())
        }
    }));
    let mut app = app(lower.with_node(node));
    run(&mut app).unwrap();
    app.push_scene(Probe::new("settings", log.clone()));
    run(&mut app).unwrap();
    global.set(29);
    run(&mut app).unwrap();
    assert_eq!(*stats.instance_updates.borrow(), vec![1]);
    assert_eq!(*reads.borrow(), vec![(17, 9)]);
    assert!(resumed.borrow().is_empty());
    assert_eq!(local.try_get(), Some(9));

    app.pop_scene();
    run(&mut app).unwrap();
    assert_eq!(*resumed.borrow(), vec![(29, 9)]);
    assert!(app.displayed[0].node == node_ref);
    assert_eq!(count(&log, "retained", "enter"), 1);
    assert_eq!(count(&log, "retained", "resume"), 1);
    run(&mut app).unwrap();
    assert_eq!(*stats.instance_updates.borrow(), vec![1, 2]);
    assert_eq!(*reads.borrow(), vec![(17, 9), (29, 9)]);

    drop(app);
    assert!(local.is_disposed());
    assert_eq!(global.try_get(), Some(29));
    drop(global_owner);
    assert!(global.is_disposed());
}

#[test]
fn app_drop_releases_global_readers_before_external_owner_after_task_cleanup() {
    struct ReadOnDrop {
        name: &'static str,
        global: Signal<u32>,
        reads: Rc<RefCell<Vec<(&'static str, u32)>>>,
    }
    impl Drop for ReadOnDrop {
        fn drop(&mut self) {
            assert!(
                !self.global.is_disposed(),
                "{} still needs global",
                self.name
            );
            self.reads
                .borrow_mut()
                .push((self.name, self.global.get_untracked()));
        }
    }

    // Signal handles are non-owning; this external guard must outlive all readers.
    let global_owner = OwnedScope::new();
    let global = global_owner.run(|| signal(17_u32));
    let log = Log::default();
    let drops = Rc::new(RefCell::new(Vec::new()));
    let task_reads = Rc::new(RefCell::new(Vec::new()));
    let task_local_handle = Rc::new(Cell::new(None::<Signal<u32>>));
    let (node, scene_local) = signal_node();
    let mut root = Probe::new("root", log.clone()).with_node(node);
    root.task = Some(task::spawn(scope_future({
        let drops = drops.clone();
        let task_reads = task_reads.clone();
        let task_local_handle = task_local_handle.clone();
        async move {
            let local = signal(41_u32);
            task_local_handle.set(Some(local));
            watch_effect(move || {
                let drops = drops.clone();
                on_cleanup(move || {
                    assert_eq!(local.try_get(), Some(41));
                    drops.borrow_mut().push(("task", global.get_untracked()));
                });
            });
            std::future::poll_fn(move |cx| {
                task_reads.borrow_mut().push(global.get_untracked());
                cx.waker().wake_by_ref();
                Poll::<()>::Pending
            })
            .await;
        }
    })));
    let mut app = app(root);
    run(&mut app).unwrap();
    task::tick(32);
    let task_local = task_local_handle.get().unwrap();
    assert_eq!(*task_reads.borrow(), vec![17]);
    app.set_default_scene_transition({
        let factory = ReadOnDrop {
            name: "factory",
            global,
            reads: drops.clone(),
        };
        let drops = drops.clone();
        move || {
            let _owned = &factory;
            assert_eq!(global.get_untracked(), 17);
            let visual = ReadOnDrop {
                name: "visual",
                global,
                reads: drops.clone(),
            };
            SceneTransition::overlay(
                TransitionTiming::AroundSwitch {
                    exit: transition::Transition::linear(1.),
                    enter: transition::Transition::linear(1.),
                },
                move |_| {
                    let _owned = &visual;
                    assert_eq!(global.get_untracked(), 17);
                    Ok(())
                },
            )
        }
    });
    let (pending_node, pending_local) = signal_node();
    let mut destination = Probe::new("pending", log.clone()).with_node(pending_node);
    destination.drop_hook = Some(Box::new({
        let drops = drops.clone();
        move || {
            drops
                .borrow_mut()
                .push(("destination", global.get_untracked()))
        }
    }));
    app.push_scene(destination);
    run(&mut app).unwrap();
    assert!(app.is_transitioning());
    assert!(app.active_transition.as_ref().unwrap().pending.is_some());
    assert_eq!(count(&log, "pending", "enter"), 0);
    assert!(drops.borrow().is_empty());

    drop(app);
    // Task Drop requests cancellation. Drain on the foreground before disposing
    // global; do not rely on every cancellation path running cleanup immediately.
    task::tick(32);
    assert_eq!(drops.borrow().len(), 4);
    for name in ["factory", "visual", "destination", "task"] {
        assert_eq!(
            drops
                .borrow()
                .iter()
                .filter(|entry| **entry == (name, 17))
                .count(),
            1,
            "{name} releases while the external global owner is alive"
        );
    }
    assert_eq!(count(&log, "root", "drop"), 1);
    assert_eq!(count(&log, "pending", "drop"), 1);
    assert!(scene_local.is_disposed());
    assert!(pending_local.is_disposed());
    assert!(task_local.is_disposed());
    task::tick(32);
    assert_eq!(
        *task_reads.borrow(),
        vec![17],
        "cancelled task never polls again"
    );
    assert_eq!(global.try_get(), Some(17));
    drop(global_owner);
    assert!(global.is_disposed());
}

fn dependent_scene(
    name: &'static str,
    log: Log,
    parent: Signal<u32>,
    observed: Rc<RefCell<Vec<u32>>>,
) -> Probe {
    let mut child = Probe::new(name, log);
    child.drop_hook = Some(Box::new(move || {
        assert!(
            !parent.is_disposed(),
            "queued child drops before the parent signal owner"
        );
        observed.borrow_mut().push(parent.get());
    }));
    child
}

struct DependentResult {
    receiver: Signal<u32>,
    observed: Rc<RefCell<Vec<u32>>>,
    log: Log,
}

impl Drop for DependentResult {
    fn drop(&mut self) {
        assert!(
            !self.receiver.is_disposed(),
            "pending result drops before the receiver signal owner"
        );
        self.observed.borrow_mut().push(self.receiver.get());
        note(&self.log, "answer", "result-drop");
    }
}

#[test]
fn dropping_app_drops_uncommitted_child_before_parent_node_and_signal() {
    let log = Log::default();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let (node, parent) = signal_node();
    let node_ref = *node;
    let mut app = app(Probe::new("parent", log.clone()).with_node(node));
    run(&mut app).unwrap();
    app.push_scene(dependent_scene(
        "queued-child",
        log.clone(),
        parent,
        observed.clone(),
    ));
    log.borrow_mut().clear();
    drop(app);
    assert_eq!(*observed.borrow(), vec![17]);
    assert_eq!(
        *log.borrow(),
        vec![("queued-child", "drop"), ("parent", "drop")]
    );
    assert!(parent.is_disposed());
    assert!(!alive(node_ref));
}

#[test]
fn pending_result_drops_before_receiver_scene_owner_on_replace_reset_and_app_drop() {
    for operation in ["replace", "reset", "drop"] {
        let log = Log::default();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let (node, receiver) = signal_node();
        let node_ref = *node;
        let mut app = app(Probe::new("receiver", log.clone()).with_node(node));
        run(&mut app).unwrap();
        paused(&mut app, 100.).unwrap();
        app.push_scene(Probe::new("child", log.clone()));
        paused(&mut app, 200.).unwrap();
        app.pop_scene_with_result(DependentResult {
            receiver,
            observed: observed.clone(),
            log: log.clone(),
        });
        paused(&mut app, 300.).unwrap();
        assert_eq!(app.scenes[0].pending_results.len(), 1);
        assert!(observed.borrow().is_empty());
        log.borrow_mut().clear();
        match operation {
            "replace" => {
                app.replace_scene(Probe::new("replacement", log.clone()));
                paused(&mut app, 400.).unwrap();
            }
            "reset" => {
                app.reset_scene(Probe::new("replacement", log.clone()));
                paused(&mut app, 400.).unwrap();
            }
            "drop" => drop(app),
            _ => unreachable!(),
        }
        assert_eq!(*observed.borrow(), vec![17], "{operation}");
        assert_eq!(
            *log.borrow(),
            vec![("answer", "result-drop"), ("receiver", "drop")],
            "{operation}"
        );
        assert!(receiver.is_disposed());
        assert!(!alive(node_ref));
    }
}

#[test]
fn exit_and_root_pop_drop_pause_requested_children_before_the_signal_owner_even_when_paused() {
    for terminal in ["exit", "root-pop"] {
        for suspended in [false, true] {
            let log = Log::default();
            let observed = Rc::new(RefCell::new(Vec::new()));
            let (node, parent) = signal_node();
            let node_ref = *node;
            let mut root = Probe::new("parent", log.clone());
            root.pause = Some(Box::new({
                let log = log.clone();
                let observed = observed.clone();
                move |ctx| {
                    ctx.push(dependent_scene(
                        "deferred-child",
                        log.clone(),
                        parent,
                        observed.clone(),
                    ));
                    Ok(())
                }
            }));
            let mut app = app(root.with_node(node));
            run(&mut app).unwrap();
            log.borrow_mut().clear();
            if terminal == "exit" {
                request_exit(&mut app);
            } else {
                app.pop_scene();
            }
            if suspended {
                paused(&mut app, 100.).unwrap();
            } else {
                run(&mut app).unwrap();
            }
            assert!(app.is_exited());
            assert_eq!(app.scene_count(), 0);
            assert_eq!(
                *observed.borrow(),
                vec![17],
                "{terminal}, paused={suspended}"
            );
            assert_eq!(
                lifecycle(&log),
                vec![
                    ("parent", "pause"),
                    ("deferred-child", "drop"),
                    ("parent", "drop")
                ]
            );
            assert!(parent.is_disposed());
            assert!(!alive(node_ref));
            assert!(app.command.is_none());
            assert_eq!(count(&log, "deferred-child", "enter"), 0);
        }
    }
}

#[test]
fn fifo_stops_at_middle_result_error_consumes_it_once_and_drops_only_the_remainder_on_reset() {
    let log = Log::default();
    let received = Rc::new(RefCell::new(Vec::new()));
    let dropped = Rc::new(Cell::new(0));
    let mut root = Probe::new("root", log.clone());
    root.result = Some(Box::new({
        let received = received.clone();
        move |result, _| {
            let result = result.downcast::<Answer>().ok().unwrap();
            received.borrow_mut().push(result.value.clone());
            ensure!(result.value != "second", "deliberate middle result failure");
            Ok(())
        }
    }));
    let mut app = app(root);
    run(&mut app).unwrap();
    paused(&mut app, 100.).unwrap();
    for value in ["first", "second", "third"] {
        app.push_scene(Probe::new("unentered", log.clone()));
        paused(&mut app, 200.).unwrap();
        app.pop_scene_with_result(Answer {
            value: value.into(),
            dropped: dropped.clone(),
        });
        paused(&mut app, 300.).unwrap();
    }
    assert_eq!(app.scenes[0].pending_results.len(), 3);
    assert!(run(&mut app).is_err());
    assert!(app.is_faulted());
    assert_eq!(
        *received.borrow(),
        vec!["first".to_owned(), "second".to_owned()]
    );
    assert_eq!(dropped.get(), 2);
    assert_eq!(app.scenes[0].pending_results.len(), 1);
    assert_eq!(count(&log, "root", "resume"), 0);
    assert!(run(&mut app).is_err());
    assert_eq!(dropped.get(), 2);
    assert_eq!(
        *received.borrow(),
        vec!["first".to_owned(), "second".to_owned()]
    );
    app.reset_scene(Probe::new("replacement", log.clone()));
    run(&mut app).unwrap();
    assert_eq!(dropped.get(), 3);
    assert_eq!(
        *received.borrow(),
        vec!["first".to_owned(), "second".to_owned()]
    );
    assert_eq!(count(&log, "root", "result"), 2);
    assert_eq!(count(&log, "root", "resume"), 0);
}

#[test]
fn paused_pending_result_stays_with_receiver_when_covered_again_and_new_receiver_handles_before_enter()
 {
    for receiver_was_entered in [false, true] {
        let log = Log::default();
        let receiver = Probe::new("receiver", log.clone());
        let stats = receiver.stats.clone();
        let mut app = app(receiver);
        if receiver_was_entered {
            run(&mut app).unwrap();
        }
        paused(&mut app, 100.).unwrap();
        app.push_scene(Probe::new("first-child", log.clone()));
        paused(&mut app, 200.).unwrap();
        app.pop_scene_with_result(17_u32);
        paused(&mut app, 300.).unwrap();
        assert_eq!(app.scenes[0].pending_results.len(), 1);
        app.push_scene(Probe::new("second-child", log.clone()));
        paused(&mut app, 400.).unwrap();
        log.borrow_mut().clear();
        run(&mut app).unwrap();
        assert_eq!(count(&log, "second-child", "enter"), 1);
        assert_eq!(count(&log, "receiver", "result"), 0);
        assert!(stats.results.borrow().is_empty());
        assert_eq!(app.scenes[0].pending_results.len(), 1);
        app.pop_scene_with_result(18_u32);
        log.borrow_mut().clear();
        run(&mut app).unwrap();
        assert_eq!(*stats.results.borrow(), vec![17, 18]);
        assert_eq!(app.scenes[0].pending_results.len(), 0);
        assert_eq!(*stats.result_dt.borrow(), vec![0., 0.]);
        let phase = if receiver_was_entered {
            "resume"
        } else {
            "enter"
        };
        let own: Vec<_> = lifecycle(&log)
            .into_iter()
            .filter(|(name, _)| *name == "receiver")
            .collect();
        assert_eq!(
            own,
            vec![
                ("receiver", "result"),
                ("receiver", "result"),
                ("receiver", phase)
            ]
        );
        run(&mut app).unwrap();
        assert_eq!(*stats.results.borrow(), vec![17, 18]);
        assert_eq!(count(&log, "receiver", phase), 1);
    }
}

#[test]
fn permanent_top_removal_discards_its_pause_child_before_owner_but_keeps_global_requests() {
    for operation in ["pop", "replace", "reset"] {
        // Host/task requests are made after pause in the paused globals callback;
        // only requests created by the permanently departing pause are discarded.
        for (suspended, global_request) in [
            (false, "none"),
            (true, "none"),
            (true, "host"),
            (true, "task"),
        ] {
            let log = Log::default();
            let observed = Rc::new(RefCell::new(Vec::new()));
            let (lower_node, lower_signal) = signal_node();
            let lower_ref = *lower_node;
            let (removed_node, removed_signal) = signal_node();
            let removed_ref = *removed_node;
            let mut removed = Probe::new("removed", log.clone());
            removed.pause = Some(Box::new({
                let log = log.clone();
                let observed = observed.clone();
                move |ctx| {
                    ctx.push(dependent_scene(
                        "pause-child",
                        log.clone(),
                        removed_signal,
                        observed.clone(),
                    ));
                    Ok(())
                }
            }));
            let mut app = app(Probe::new("lower", log.clone()).with_node(lower_node));
            run(&mut app).unwrap();
            app.push_scene(removed.with_node(removed_node));
            run(&mut app).unwrap();
            log.borrow_mut().clear();
            match operation {
                "pop" => {
                    app.pop_scene();
                }
                "replace" => {
                    app.replace_scene(Probe::new("replacement", log.clone()));
                }
                "reset" => {
                    app.reset_scene(Probe::new("replacement", log.clone()));
                }
                _ => unreachable!(),
            }
            if suspended {
                app.run_checked_paused_frame(frame(0., 100.), |app| match global_request {
                    "none" => {}
                    "host" => {
                        app.push_scene(Probe::new("global-child", log.clone()));
                    }
                    "task" => {
                        let mut request = task::spawn({
                            let log = log.clone();
                            async move { Probe::new("global-child", log) }
                        });
                        task::tick(32);
                        app.push_scene(request.try_take().unwrap());
                    }
                    _ => unreachable!(),
                })
                .unwrap();
            } else {
                run(&mut app).unwrap();
            }
            let expected_count = if operation == "replace" { 2 } else { 1 };
            assert_eq!(
                app.scene_count(),
                expected_count,
                "{operation}/{global_request}: exactly the selected commit"
            );
            assert_eq!(*observed.borrow(), vec![17]);
            assert_eq!(count(&log, "removed", "pause"), 1);
            assert_eq!(count(&log, "removed", "drop"), 1);
            assert_eq!(count(&log, "pause-child", "drop"), 1);
            assert_eq!(count(&log, "pause-child", "enter"), 0);
            assert_eq!(
                count(&log, "lower", "drop"),
                usize::from(operation == "reset")
            );
            assert_eq!(alive(lower_ref), operation != "reset");
            assert_eq!(lower_signal.is_disposed(), operation == "reset");
            assert!(!alive(removed_ref));
            assert!(removed_signal.is_disposed());
            let order = log.borrow();
            let child_drop = order
                .iter()
                .position(|entry| *entry == ("pause-child", "drop"))
                .unwrap();
            let owner_drop = order
                .iter()
                .position(|entry| *entry == ("removed", "drop"))
                .unwrap();
            assert!(
                child_drop < owner_drop,
                "{operation}, paused={suspended}: borrowed pause request drops first"
            );
            drop(order);
            assert_eq!(app.command.is_some(), global_request != "none");
            assert_eq!(count(&log, "global-child", "drop"), 0);
            assert_eq!(count(&log, "global-child", "enter"), 0);

            if global_request != "none" {
                paused(&mut app, 200.).unwrap();
                assert_eq!(
                    app.scene_count(),
                    expected_count + 1,
                    "globals request is not mistaken for the old pause request"
                );
                assert_eq!(count(&log, "global-child", "enter"), 0);
                run(&mut app).unwrap();
                assert_eq!(count(&log, "global-child", "enter"), 1);
                if operation == "pop" {
                    assert_eq!(
                        count(&log, "lower", "resume"),
                        0,
                        "paused receiver was covered before activation"
                    );
                } else {
                    assert_eq!(
                        count(&log, "replacement", "enter"),
                        0,
                        "new selected scene was covered before activation"
                    );
                }
            } else {
                run(&mut app).unwrap();
                assert_eq!(
                    app.scene_count(),
                    expected_count,
                    "discarded pause child cannot commit on the next frame"
                );
                if operation == "pop" {
                    assert_eq!(count(&log, "lower", "resume"), 1);
                } else {
                    assert_eq!(count(&log, "replacement", "enter"), 1);
                }
            }
            assert_eq!(count(&log, "pause-child", "enter"), 0);
            assert_eq!(count(&log, "pause-child", "drop"), 1);
            assert_eq!(count(&log, "removed", "drop"), 1);
            assert!(app.command.is_none());
        }
    }
}
