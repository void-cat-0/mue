//! Real Window/App regression: synthetic raw events, real layout and pixels.
use super::*;
use crate::{
    SceneEvent,
    color::Color,
    event::pointer::PointerAction,
    input,
    node::{Node, flexbox},
    style::{Styleable, StyleableExt},
};
use mue_core::prelude::*;
use std::cell::RefCell;

fn touch(x: f32, y: f32) {
    input::inject(|events| {
        events.touch_event(miniquad::TouchPhase::Started, 101, x, y, 12.);
        events.touch_event(miniquad::TouchPhase::Ended, 101, x, y, 12.01);
    });
}

async fn continuing_touch_across_scene_replacement() {
    use crate::{
        input::InputEvent,
        scene::{RootInput, Scene, SceneContext, SceneFrame, SceneRender, SceneRoot},
    };
    use std::cell::Cell;

    struct View {
        root: Node,
        raw: Rc<RefCell<Vec<InputEvent>>>,
        next: Option<Box<View>>,
        replace: bool,
    }
    impl Scene for View {
        fn event(
            &mut self,
            event: SceneEvent<'_>,
            _ctx: &mut SceneContext<'_>,
        ) -> anyhow::Result<()> {
            let SceneEvent::Input(input) = event else {
                return Ok(());
            };
            self.raw.borrow_mut().extend_from_slice(input.events());
            if self.next.is_some()
                && input.events().iter().any(|event| {
                    matches!(
                        event,
                        InputEvent::Touch {
                            phase: miniquad::TouchPhase::Started,
                            id: 900,
                            ..
                        }
                    )
                })
            {
                self.replace = true;
            }
            Ok(())
        }
        fn update(&mut self, ctx: &mut SceneContext<'_>) -> anyhow::Result<()> {
            if self.replace {
                self.replace = false;
                ctx.replace(*self.next.take().unwrap());
            }
            Ok(())
        }
        fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> anyhow::Result<()> {
            roots.push(SceneRoot::new(*self.root, frame.viewport).input(RootInput::Modal));
            Ok(())
        }
        fn render(&mut self, render: &mut SceneRender<'_>) -> anyhow::Result<()> {
            render.draw(*self.root)
        }
    }

    let old_raw = Rc::new(RefCell::new(Vec::new()));
    let new_raw = Rc::new(RefCell::new(Vec::new()));
    let old_canceled = Rc::new(Cell::new(0));
    let new_taps = Rc::new(Cell::new(0));
    let new_root = flexbox()
        .size_full()
        .on_tap({
            let new_taps = new_taps.clone();
            move |_| new_taps.set(new_taps.get() + 1)
        })
        .into_node();
    let old_root = flexbox()
        .size_full()
        .on_pointer_event({
            let old_canceled = old_canceled.clone();
            move |event| {
                if event.action() == PointerAction::Cancel {
                    old_canceled.set(old_canceled.get() + 1);
                }
            }
        })
        .into_node();
    let game = View {
        root: new_root,
        raw: new_raw.clone(),
        next: None,
        replace: false,
    };
    let loading = View {
        root: old_root,
        raw: old_raw.clone(),
        next: Some(Box::new(game)),
        replace: false,
    };
    let mut app = App::with_scene(loading);
    app.try_frame().unwrap();
    macroquad::window::next_frame().await;
    input::inject(|events| {
        events.touch_event(
            miniquad::TouchPhase::Started,
            900,
            30.125,
            40.25,
            12345.678901234,
        )
    });
    app.try_frame().unwrap();
    assert!(
        matches!(old_raw.borrow().as_slice(), [InputEvent::Touch { phase: miniquad::TouchPhase::Started, id: 900, x, y, time }]
        if *x == 30.125 && *y == 40.25 && *time == 12345.678901234)
    );
    // Loading requested replacement during update after this input; no replay.
    assert_eq!(old_canceled.get(), 1);
    assert!(new_raw.borrow().is_empty());
    macroquad::window::next_frame().await;
    input::inject(|events| {
        events.touch_event(
            miniquad::TouchPhase::Moved,
            900,
            31.5,
            42.75,
            12346.987654321,
        );
        events.touch_event(
            miniquad::TouchPhase::Ended,
            900,
            32.25,
            43.5,
            12347.111111111,
        );
    });
    app.try_frame().unwrap();
    assert!(
        matches!(new_raw.borrow().as_slice(), [InputEvent::Touch { phase: miniquad::TouchPhase::Moved, id: 900, x, y, time },
        InputEvent::Touch { phase: miniquad::TouchPhase::Ended, id: 900, x: end_x, y: end_y, time: end_time }]
        if *x == 31.5 && *y == 42.75 && *time == 12346.987654321
            && *end_x == 32.25 && *end_y == 43.5 && *end_time == 12347.111111111)
    );
    assert_eq!(
        new_taps.get(),
        0,
        "continuing input must not synthesize a new UI press"
    );
    macroquad::window::next_frame().await;
    input::inject(|events| {
        events.touch_event(miniquad::TouchPhase::Started, 901, 30., 40., 12348.);
        events.touch_event(miniquad::TouchPhase::Ended, 901, 30., 40., 12348.1);
    });
    app.try_frame().unwrap();
    assert_eq!(new_taps.get(), 1, "fresh Node Down/Up still taps normally");
    assert_eq!(
        new_raw.borrow().len(),
        4,
        "modal UI does not filter Scene raw events"
    );
    macroquad::window::next_frame().await;
}

#[test]
#[ignore = "requires a graphics display; verifies real App event/layout/render ordering"]
fn input_precedes_updates_and_feedback_renders_in_the_same_frame() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue input phase regression".into(),
            window_width: 320,
            window_height: 240,
            high_dpi: false,
            window_resizable: true,
            ..Default::default()
        },
        async {
            // Keep the pixel regression independent of installed system fonts.
            let mut fonts = crate::font::fontdb::Database::new();
            fonts.load_font_data(
                include_bytes!("../../../examples/playground/assets/fonts/NotoSans-Regular.ttf")
                    .to_vec(),
            );
            fonts.set_sans_serif_family("Noto Sans");
            crate::font::initialize("en-US", fonts).unwrap();
            let owner = OwnedScope::new();
            let (swapped, clicked) = owner.run(|| (signal(false), signal(false)));
            let log = Rc::new(RefCell::new(Vec::new()));
            let a = flexbox()
                .absolute()
                .width(40.)
                .height(40.)
                .inset_top(30.)
                .inset_left(swapped.map(|v| if v { 200. } else { 20. }))
                .background(Color::from_rgba(30, 90, 200, 255))
                .on_pointer_event({
                    let log = log.clone();
                    move |event| {
                        if event.action() == PointerAction::Down {
                            log.borrow_mut().push("input-A");
                            clicked.set(true);
                        }
                    }
                });
            let b = flexbox()
                .absolute()
                .width(40.)
                .height(40.)
                .inset_top(30.)
                .inset_left(swapped.map(|v| if v { 20. } else { 200. }))
                .background(Color::from_rgba(220, 150, 30, 255))
                .on_pointer_event({
                    let log = log.clone();
                    move |event| {
                        log.borrow_mut().push(match event.action() {
                            PointerAction::Down => "input-B",
                            PointerAction::Move => "move-B",
                            PointerAction::Up => "up-B",
                            PointerAction::Cancel => "cancel-B",
                        });
                    }
                });
            let feedback = flexbox()
                .absolute()
                .inset_top(110.)
                .inset_left(20.)
                .height(30.)
                .width(clicked.map(|v| if v { 100. } else { 20. }))
                .background(clicked.map(|v| {
                    crate::shader::Brush::Solid(if v {
                        Color::from_rgba(0, 255, 0, 255)
                    } else {
                        Color::from_rgba(255, 0, 0, 255)
                    })
                }))
                .on_layout({
                    let log = log.clone();
                    move |_| log.borrow_mut().push("layout-feedback")
                })
                .on_update({
                    let log = log.clone();
                    move |_| log.borrow_mut().push("node-update")
                })
                .on_render({
                    let log = log.clone();
                    move |_| {
                        log.borrow_mut().push(if clicked.get() {
                            "render-green"
                        } else {
                            "render-red"
                        })
                    }
                });
            // Explicit hit bounds are already usable on the initial tree.
            // First-frame input is dispatched normally, even before layout.
            let first_frame = Rc::new(std::cell::Cell::new(false));
            let probe = Node::leaf(
                crate::style::Style::new().on_pointer_event({
                    let first_frame = first_frame.clone();
                    move |event| {
                        if event.action() == PointerAction::Down {
                            first_frame.set(true);
                        }
                    }
                }),
                |_| {
                    crate::event::hit_test::set_hit_test_fn(Rc::new(|node, pos, result| {
                        if crate::math::Rect::new(270., 180., 30., 30.).contains(&pos) {
                            result.hit_self(node);
                        }
                    }))
                },
            );
            let fixed_text = crate::node::text("alpha beta gamma delta epsilon zeta")
                .absolute()
                .inset_left(20.)
                .inset_top(160.)
                .width(120.)
                .height(60.)
                .font_size(16.)
                .line_height(20.)
                .color(Color::from_rgba(255, 255, 255, 255));
            let mut app = App::new(
                flexbox()
                    .size_full()
                    .children((a, b, feedback, probe, fixed_text)),
            );
            touch(280., 190.);
            app.frame();
            assert!(first_frame.get());
            assert!(!clicked.get());
            macroquad::window::next_frame().await;
            log.borrow_mut().clear();

            struct Advance(Rc<RefCell<Vec<&'static str>>>);
            impl crate::transition::Driver for Advance {
                fn advance(&self, _dt: f32) {
                    self.0.borrow_mut().push("transition");
                }
            }
            let driver: Rc<dyn crate::transition::Driver> = Rc::new(Advance(log.clone()));
            crate::transition::register_driver(&driver);
            crate::runtime::set_timeout(0., {
                let log = log.clone();
                move || {
                    assert!(clicked.get());
                    log.borrow_mut().push("timeout");
                    swapped.set(true);
                }
            });
            let task = mue_core::task::spawn({
                let log = log.clone();
                async move {
                    assert!(clicked.get());
                    log.borrow_mut().push("task");
                }
            });
            touch(40., 50.);
            macroquad::window::clear_background(macroquad::color::BLACK);
            app.frame_with_update(|| {
                assert!(clicked.get());
                log.borrow_mut().push("host-update");
            });
            let order = log.borrow().clone();
            let index = |value| order.iter().position(|v| *v == value).unwrap();
            assert!(!order.contains(&"input-B"));
            assert!(index("input-A") < index("transition"));
            assert!(index("input-A") < index("timeout"));
            assert!(index("timeout") < index("task"));
            assert!(index("task") < index("host-update"));
            assert!(index("host-update") < index("layout-feedback"));
            assert!(index("layout-feedback") < index("node-update"));
            assert!(index("node-update") < index("render-green"));
            assert_eq!(order.iter().filter(|v| **v == "node-update").count(), 1);
            assert!(task.is_finished());
            let image = macroquad::texture::get_screen_data();
            let pixel = image.get_pixel(90, 120);
            assert!(
                pixel.g > 0.9 && pixel.r < 0.1,
                "feedback must be green at its expanded width: {pixel:?}"
            );
            assert!(
                (181..215).any(|y| (20..140).any(|x| image.get_pixel(x, y).r > 0.5)),
                "fixed-size text must wrap and draw beyond its first line",
            );
            // Re-preparing and redrawing a prepared tree must not advance its
            // frame state or run layout callbacks during the actual draw.
            let size = Vector::new(320., 240.);
            let root = app.displayed[0].node;
            root.prepare_layout(size);
            root.prepare_layout(size);
            let prepared_log = log.borrow().clone();
            for _ in 0..2 {
                macroquad::window::clear_background(macroquad::color::BLACK);
                root.render();
                let redrawn = macroquad::texture::get_screen_data();
                assert_eq!(redrawn.bytes, image.bytes);
            }
            let log_after_draw = log.borrow();
            assert_eq!(
                log_after_draw
                    .iter()
                    .filter(|v| **v == "node-update")
                    .count(),
                1,
            );
            assert_eq!(
                log_after_draw
                    .iter()
                    .filter(|v| **v == "layout-feedback")
                    .count(),
                prepared_log
                    .iter()
                    .filter(|v| **v == "layout-feedback")
                    .count(),
            );
            drop(log_after_draw);
            macroquad::window::next_frame().await;

            // Resizing neither cancels capture nor drops newly received events.
            log.borrow_mut().clear();
            input::inject(|events| {
                events.touch_event(miniquad::TouchPhase::Started, 202, 40., 50., 13.)
            });
            app.frame();
            macroquad::window::next_frame().await;
            let previous_width = macroquad::window::screen_width();
            macroquad::window::request_new_screen_size(previous_width + 160., 240.);
            let mut resized = false;
            for _ in 0..60 {
                let changed = macroquad::window::screen_width() != previous_width;
                if changed {
                    touch(40., 50.);
                    input::inject(|events| {
                        events.touch_event(miniquad::TouchPhase::Moved, 202, 300., 50., 13.1);
                        events.touch_event(miniquad::TouchPhase::Ended, 202, 300., 50., 13.2);
                    });
                }
                // Keep consuming each host frame; skipped-frame cancellation
                // is a separate input interruption policy, not resize handling.
                app.frame();
                macroquad::window::next_frame().await;
                if changed {
                    resized = true;
                    break;
                }
            }
            assert!(resized, "window resize must reach the host");
            assert_eq!(log.borrow().iter().filter(|v| **v == "input-B").count(), 2);
            assert!(log.borrow().contains(&"move-B"));
            assert!(log.borrow().contains(&"up-B"));
            assert!(!log.borrow().contains(&"cancel-B"));

            // Suspend drains old input and cancels an active capture. The
            // first normal frame accepts fresh input; it is not a dead frame.
            log.borrow_mut().clear();
            input::inject(|events| {
                events.touch_event(miniquad::TouchPhase::Started, 203, 40., 50., 14.)
            });
            app.frame();
            macroquad::window::next_frame().await;
            log.borrow_mut().clear();
            touch(40., 50.);
            app.frame_paused();
            assert_eq!(*log.borrow(), vec!["cancel-B"]);
            macroquad::window::next_frame().await;
            log.borrow_mut().clear();
            touch(40., 50.);
            app.frame();
            assert_eq!(log.borrow().iter().filter(|v| **v == "input-B").count(), 1);
            assert!(!log.borrow().contains(&"cancel-B"));
            macroquad::window::next_frame().await;
            log.borrow_mut().clear();
            app.frame();
            assert!(!log.borrow().iter().any(|v| v.starts_with("input-")));
            macroquad::window::next_frame().await;

            // Same real Window/input source, a Scene pair, and actual Node taps.
            // Synthetic native events validate plumbing, not hardware touch timing.
            continuing_touch_across_scene_replacement().await;
            macroquad::window::next_frame().await;
        },
    );
}
