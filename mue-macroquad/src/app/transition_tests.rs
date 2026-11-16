//! Headless public-request and draw-only transition scheduling regressions.

use super::*;
use crate::{
    layout::use_layout,
    node::Node,
    style::{Style, Styleable},
    transition::Transition,
};
use miniquad::TouchPhase;
use mue_core::signal::{Access, Signal, signal};
use std::cell::{Cell, RefCell};

type Log = Rc<RefCell<Vec<(&'static str, &'static str)>>>;
type Samples = Rc<RefCell<Vec<TransitionFrame>>>;

#[test]
fn visual_drops_before_queued_destination_owner_when_commit_pause_fails() {
    for timing in [
        TransitionTiming::AfterSwitch(Transition::linear(0.1)),
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.1),
            enter: Transition::linear(0.1),
        },
    ] {
        let log = Log::default();
        let mut root = Probe::new("root", &log);
        root.fail = Some("pause");
        let mut app = app(root);
        run(&mut app, 0.1).unwrap();
        let (node, destination_signal) = owned_signal_node();
        let mut child = Probe::new("destination", &log);
        child.node = Some(node);
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe {
            signal: destination_signal,
            drops: drops.clone(),
        };
        app.push_scene(child)
            .transition(SceneTransition::overlay(timing, move |_| {
                let _owned = &probe;
                Ok(())
            }));
        match timing {
            TransitionTiming::AfterSwitch(_) => {
                assert!(run(&mut app, 1.).is_err());
            }
            TransitionTiming::AroundSwitch { .. } => {
                run(&mut app, 1.).unwrap();
                run(&mut app, 1.).unwrap();
                assert!(run(&mut app, 1.).is_err());
            }
        }
        assert!(app.is_faulted());
        assert_eq!(drops.get(), 1);
        assert!(destination_signal.is_disposed());
        assert_eq!(count(&log, "destination", "enter"), 0);
    }
}

#[test]
fn requests_over_never_entered_suspended_source_do_not_manufacture_activation() {
    for timing in [
        TransitionTiming::AfterSwitch(Transition::linear(0.1)),
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.1),
            enter: Transition::linear(0.1),
        },
    ] {
        let log = Log::default();
        let mut app = app(Probe::new("old", &log));
        app.run_checked_paused_frame(frame(0.), |_| {}).unwrap();
        app.replace_scene(Probe::new("new", &log))
            .transition(SceneTransition::overlay(timing, |_| {
                panic!("unentered source must not draw")
            }));
        app.run_checked_paused_frame(frame(0.), |_| {}).unwrap();
        assert_eq!(count(&log, "old", "enter"), 0);
        assert_eq!(count(&log, "old", "pause"), 0);
        assert_eq!(count(&log, "old", "render"), 0);
        assert_eq!(count(&log, "new", "enter"), 0);
        assert!(!app.is_transitioning());
        run(&mut app, 0.1).unwrap();
        assert_eq!(count(&log, "new", "enter"), 1);
    }
}

#[test]
fn pause_requests_stay_separate_from_selected_around_destination_and_new_global_requests() {
    let log = Log::default();
    let mut root = Probe::new("root", &log);
    let copies = log.clone();
    root.events = Some(Box::new(move |event, ctx| {
        if let SceneEvent::Pause = event {
            ctx.push(Probe::new("pause-child", &copies));
        }
        Ok(())
    }));
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("selected", &log))
        .transition(around(Rc::default()));
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(count(&log, "selected", "enter"), 1);
    assert_eq!(count(&log, "pause-child", "enter"), 0);
    run(&mut app, 0.1).unwrap();
    assert_eq!(app.scene_count(), 3);
    assert_eq!(count(&log, "pause-child", "enter"), 1);
}

fn owned_signal_node() -> (Node, Signal<u32>) {
    let handle = Rc::new(Cell::new(None));
    let node = Node::leaf(Style::new().width(80.).height(80.), {
        let handle = handle.clone();
        move |style| {
            use_layout(style);
            handle.set(Some(signal(17_u32)));
        }
    });
    (node, handle.get().unwrap())
}
fn dependent_effect(owner: Signal<u32>, drops: Rc<Cell<usize>>) -> SceneTransition {
    let probe = DropProbe {
        signal: owner,
        drops,
    };
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.25),
            enter: Transition::linear(0.25),
        },
        move |_| {
            let _keep_alive = &probe;
            Ok(())
        },
    )
}

#[test]
fn pending_effect_and_destination_drop_before_source_on_cancel_fault_reset_exit_and_app_drop() {
    for operation in ["cancel", "fault", "reset", "exit", "drop"] {
        let log = Log::default();
        let (node, owner) = owned_signal_node();
        let mut root = Probe::new("root", &log);
        root.node = Some(node);
        let mut app = app(root);
        run(&mut app, 0.1).unwrap();
        let effect_drops = Rc::new(Cell::new(0));
        let child_drops = Rc::new(Cell::new(0));
        let mut child = Probe::new("pending", &log);
        child.drop_probe = Some(DropProbe {
            signal: owner,
            drops: child_drops.clone(),
        });
        app.push_scene(child)
            .transition(dependent_effect(owner, effect_drops.clone()));
        run(&mut app, 0.1).unwrap();
        assert_eq!(effect_drops.get(), 0);
        assert_eq!(child_drops.get(), 0);
        match operation {
            "cancel" => {
                app.push_scene(Probe::new("selected", &log))
                    .without_transition();
                run(&mut app, 0.1).unwrap();
            }
            "fault" => {
                app.fault();
            }
            "reset" => {
                app.reset_scene(Probe::new("selected", &log))
                    .without_transition();
                run(&mut app, 0.1).unwrap();
            }
            "exit" => {
                SceneRequest::register(&mut app.command, SceneCommand::Exit).without_transition();
                run(&mut app, 0.1).unwrap();
            }
            "drop" => {
                drop(app);
                assert!(owner.is_disposed());
                assert_eq!(effect_drops.get(), 1);
                assert_eq!(child_drops.get(), 1);
                continue;
            }
            _ => unreachable!(),
        }
        assert_eq!(effect_drops.get(), 1, "{operation}");
        assert_eq!(child_drops.get(), 1, "{operation}");
        assert_eq!(count(&log, "pending", "enter"), 0);
    }
}

#[test]
fn terminal_result_and_visual_drop_before_root_owner() {
    let log = Log::default();
    let (node, owner) = owned_signal_node();
    let mut root = Probe::new("root", &log);
    root.node = Some(node);
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    let effect_drops = Rc::new(Cell::new(0));
    let result_drops = Rc::new(Cell::new(0));
    app.pop_scene_with_result(DropProbe {
        signal: owner,
        drops: result_drops.clone(),
    })
    .transition(dependent_effect(owner, effect_drops.clone()));
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    assert!(app.is_exited());
    assert!(owner.is_disposed());
    assert_eq!(effect_drops.get(), 1);
    assert_eq!(result_drops.get(), 1);
}

#[test]
fn pause_failure_drops_selected_visual_and_child_before_global_recovery() {
    let log = Log::default();
    let (node, owner) = owned_signal_node();
    let mut root = Probe::new("root", &log);
    root.node = Some(node);
    root.fail = Some("pause");
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    let effect_drops = Rc::new(Cell::new(0));
    let child_drops = Rc::new(Cell::new(0));
    let mut child = Probe::new("pending", &log);
    child.drop_probe = Some(DropProbe {
        signal: owner,
        drops: child_drops.clone(),
    });
    app.push_scene(child)
        .transition(dependent_effect(owner, effect_drops.clone()));
    let recovered_log = log.clone();
    assert!(
        app.run_checked_paused_frame(frame(0.), |app| {
            assert_eq!(effect_drops.get(), 1);
            assert_eq!(child_drops.get(), 1);
            app.reset_scene(Probe::new("recovered", &recovered_log))
                .without_transition();
            app.install_command(frame(0.)).unwrap();
        })
        .is_err()
    );
    assert!(owner.is_disposed());
}

#[test]
fn fault_recovery_never_draws_failed_source_or_creates_default_effect() {
    let log = Log::default();
    let mut root = Probe::new("root", &log);
    root.fail = Some("update");
    let mut app = app(root);
    assert!(run(&mut app, 0.1).is_err());
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    app.set_default_scene_transition(move || {
        captured.set(captured.get() + 1);
        after(Rc::default())
    });
    app.replace_scene(Probe::new("recovered", &log));
    run(&mut app, 0.1).unwrap();
    assert_eq!(calls.get(), 0);
    assert_eq!(count(&log, "root", "source"), 0);
    assert_eq!(count(&log, "recovered", "enter"), 1);
    assert!(!app.is_transitioning());
}
type Hook = Box<dyn FnMut(SceneEvent<'_>, &mut SceneContext<'_>) -> Result<()>>;

struct Probe {
    name: &'static str,
    log: Log,
    events: Option<Hook>,
    node: Option<Node>,
    node_ticks: Rc<RefCell<Vec<f32>>>,
    scene_ticks: Rc<RefCell<Vec<f32>>>,
    fail: Option<&'static str>,
    drop_probe: Option<DropProbe>,
}
impl Probe {
    fn new(name: &'static str, log: &Log) -> Self {
        Self {
            name,
            log: log.clone(),
            events: None,
            node: None,
            node_ticks: Rc::default(),
            scene_ticks: Rc::default(),
            fail: None,
            drop_probe: None,
        }
    }
    fn with_node(mut self) -> Self {
        let ticks = self.node_ticks.clone();
        self.node = Some(Node::leaf(
            Style::new()
                .width(50.)
                .height(50.)
                .on_update(move |dt| ticks.borrow_mut().push(*dt)),
            |style| {
                use_layout(style);
            },
        ));
        self
    }
}
impl Scene for Probe {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        let phase = match &event {
            SceneEvent::Enter => "enter",
            SceneEvent::Pause => "pause",
            SceneEvent::Resume => "resume",
            SceneEvent::Returned(_) => "returned",
            SceneEvent::Input(_) => "input",
        };
        self.log.borrow_mut().push((self.name, phase));
        if let Some(hook) = &mut self.events {
            hook(event, ctx)?;
        }
        ensure!(self.fail != Some(phase), "deliberate {phase} error");
        Ok(())
    }
    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.log.borrow_mut().push((self.name, "update"));
        self.scene_ticks.borrow_mut().push(ctx.frame.dt);
        ensure!(self.fail != Some("update"), "deliberate update error");
        Ok(())
    }
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        self.log.borrow_mut().push((self.name, "layout"));
        if let Some(node) = &self.node {
            roots.push(SceneRoot::new(**node, frame.viewport));
        }
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        self.log.borrow_mut().push((
            self.name,
            if render.is_transition_source() {
                "source"
            } else {
                "render"
            },
        ));
        if let Some(node) = &self.node {
            render.draw(**node)?;
        }
        ensure!(self.fail != Some("render"), "deliberate render error");
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        drop(self.drop_probe.take());
        self.log.borrow_mut().push((self.name, "drop"));
    }
}

struct DropProbe {
    signal: Signal<u32>,
    drops: Rc<Cell<usize>>,
}
impl Drop for DropProbe {
    fn drop(&mut self) {
        assert_eq!(self.signal.get(), 17);
        self.drops.set(self.drops.get() + 1);
    }
}

fn frame(dt: f32) -> SceneFrame {
    SceneFrame {
        dt,
        time: 42.,
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
fn run(app: &mut App, dt: f32) -> Result<()> {
    app.run_checked_frame(InputBatch::recorded(|_| {}), frame(dt), || {}, |_, _| {})
}
fn count(log: &Log, name: &'static str, phase: &'static str) -> usize {
    log.borrow()
        .iter()
        .filter(|entry| **entry == (name, phase))
        .count()
}
fn around(samples: Rc<RefCell<Vec<TransitionFrame>>>) -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.25),
            enter: Transition::ease_out(0.25),
        },
        move |frame| {
            samples.borrow_mut().push(frame);
            Ok(())
        },
    )
}
fn after(samples: Rc<RefCell<Vec<TransitionFrame>>>) -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AfterSwitch(Transition::linear(0.25)),
        move |frame| {
            samples.borrow_mut().push(frame);
            Ok(())
        },
    )
}

#[test]
#[allow(
    clippy::forget_non_drop,
    reason = "prove that forgetting the configuration handle never discards the registered request"
)]
fn chained_requests_register_without_drop_and_latest_override_wins() {
    let log = Log::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    let discarded_samples = Rc::default();
    app.push_scene(Probe::new("discarded", &log))
        .transition(after(discarded_samples));
    assert_eq!(count(&log, "discarded", "drop"), 0);
    let request = app.push_scene(Probe::new("selected", &log));
    std::mem::forget(request);
    assert_eq!(count(&log, "discarded", "drop"), 1);
    run(&mut app, 0.1).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(count(&log, "selected", "enter"), 1);
    assert!(!app.is_transitioning());
}

#[test]
fn after_switch_commits_first_only_top_updates_and_endpoint_retires_next_frame() {
    let log = Log::default();
    let root = Probe::new("root", &log).with_node();
    let root_ticks = root.node_ticks.clone();
    let child = Probe::new("child", &log).with_node();
    let child_ticks = child.node_ticks.clone();
    let samples = Samples::default();
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    app.push_scene(child).transition(after(samples.clone()));
    run(&mut app, 3.).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(count(&log, "root", "pause"), 1);
    assert_eq!(count(&log, "root", "update"), 1);
    assert_eq!(&*root_ticks.borrow(), &[0.1]);
    assert_eq!(&*child_ticks.borrow(), &[0.]);
    assert_eq!(samples.borrow()[0].linear_progress, 0.);
    run(&mut app, 0.125).unwrap();
    assert_eq!(app.transition_state().unwrap().linear_progress, 0.5);
    run(&mut app, 10.).unwrap();
    assert_eq!(app.transition_state().unwrap().linear_progress, 1.);
    assert!(app.is_transitioning());
    run(&mut app, 0.1).unwrap();
    assert!(!app.is_transitioning());
    assert_eq!(count(&log, "root", "update"), 1);
    assert_eq!(count(&log, "child", "enter"), 1);
}

#[test]
fn around_switch_requires_presented_exit_and_returns_before_resume() {
    let log = Log::default();
    let samples = Samples::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("child", &log));
    run(&mut app, 0.1).unwrap();
    app.pop_scene_with_result(9_u32)
        .transition(around(samples.clone()));
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(samples.borrow()[0].phase, TransitionPhase::BeforeSwitch);
    assert_eq!(samples.borrow()[0].linear_progress, 0.);
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 2); // endpoint has actually drawn, not yet committed
    assert_eq!(samples.borrow()[1].linear_progress, 1.);
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 1);
    let events = log.borrow();
    let returned = events
        .iter()
        .position(|event| *event == ("root", "returned"))
        .unwrap();
    let resume = events
        .iter()
        .position(|event| *event == ("root", "resume"))
        .unwrap();
    assert!(returned < resume);
    drop(events);
    assert_eq!(
        app.transition_state().unwrap().phase,
        TransitionPhase::AfterSwitch
    );
    assert_eq!(app.transition_state().unwrap().linear_progress, 0.);
}

#[test]
fn new_request_cancels_pending_before_switch_even_after_exit_endpoint() {
    let log = Log::default();
    let samples = Samples::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("never-entered", &log))
        .transition(around(samples));
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    app.push_scene(Probe::new("replacement", &log))
        .without_transition();
    run(&mut app, 1.).unwrap();
    assert_eq!(count(&log, "never-entered", "enter"), 0);
    assert_eq!(count(&log, "never-entered", "drop"), 1);
    assert_eq!(count(&log, "replacement", "enter"), 1);
    assert_eq!(app.scene_count(), 2);
    assert!(!app.is_transitioning());
}

#[test]
fn new_request_after_commit_does_not_roll_back_the_scene_stack() {
    let log = Log::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("middle", &log))
        .transition(after(Rc::default()));
    run(&mut app, 1.).unwrap();
    app.push_scene(Probe::new("top", &log)).without_transition();
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 3);
    assert_eq!(count(&log, "middle", "drop"), 0);
    assert_eq!(count(&log, "middle", "pause"), 1);
}

#[test]
fn defaults_create_only_selected_fresh_effects_and_can_be_bypassed() {
    let log = Log::default();
    let created = Rc::new(Cell::new(0));
    let counts = created.clone();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.set_default_scene_transition(move || {
        counts.set(counts.get() + 1);
        after(Rc::default())
    });
    app.push_scene(Probe::new("unused", &log));
    app.push_scene(Probe::new("one", &log));
    run(&mut app, 0.1).unwrap();
    assert_eq!(created.get(), 1);
    app.pop_scene().without_transition();
    run(&mut app, 0.1).unwrap();
    assert_eq!(created.get(), 1);
    app.push_scene(Probe::new("two", &log))
        .transition(after(Rc::default()));
    run(&mut app, 0.1).unwrap();
    assert_eq!(created.get(), 1);
    app.clear_default_scene_transition();
    app.pop_scene();
    run(&mut app, 0.1).unwrap();
    assert!(!app.is_transitioning());
}

#[test]
fn host_pause_freezes_exit_and_resume_first_delta_is_zero() {
    let log = Log::default();
    let samples = Samples::default();
    let mut app = app(Probe::new("root", &log).with_node());
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("child", &log))
        .transition(around(samples.clone()));
    run(&mut app, 0.1).unwrap();
    run(&mut app, 0.1).unwrap();
    let frozen = app.transition_state();
    let draw_count = samples.borrow().len();
    for _ in 0..3 {
        app.run_checked_paused_frame(frame(99.), |_| {}).unwrap();
    }
    assert_eq!(app.transition_state(), frozen);
    assert_eq!(samples.borrow().len(), draw_count);
    run(&mut app, 99.).unwrap();
    assert_eq!(
        app.transition_state().unwrap().linear_progress,
        frozen.unwrap().linear_progress
    );
    assert_eq!(app.transition_state().unwrap().dt, 0.);
    assert_eq!(count(&log, "root", "resume"), 1);
    run(&mut app, 99.).unwrap();
    assert_eq!(app.scene_count(), 1);
    run(&mut app, 99.).unwrap();
    assert_eq!(app.scene_count(), 2);
}

#[test]
fn animated_request_selected_while_paused_waits_without_rendering() {
    let log = Log::default();
    let samples = Samples::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.run_checked_paused_frame(frame(0.), |_| {}).unwrap();
    app.push_scene(Probe::new("child", &log))
        .transition(after(samples.clone()));
    app.run_checked_paused_frame(frame(0.), |_| {}).unwrap();
    assert_eq!(app.scene_count(), 1);
    assert!(samples.borrow().is_empty());
    assert_eq!(count(&log, "child", "enter"), 0);
    run(&mut app, 99.).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert_eq!(app.transition_state().unwrap().linear_progress, 0.);
    assert_eq!(count(&log, "root", "resume"), 1);
}

#[test]
fn endpoint_draw_error_faults_without_committing_destination() {
    let log = Log::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("child", &log))
        .transition(SceneTransition::overlay(
            TransitionTiming::AroundSwitch {
                exit: Transition::linear(0.1),
                enter: Transition::Instant,
            },
            |frame| {
                ensure!(frame.linear_progress < 1., "endpoint error");
                Ok(())
            },
        ));
    run(&mut app, 0.1).unwrap();
    assert!(run(&mut app, 1.).is_err());
    assert!(app.is_faulted());
    assert_eq!(app.scene_count(), 1);
    assert_eq!(count(&log, "child", "drop"), 1);
    assert_eq!(count(&log, "child", "enter"), 0);
    assert!(!app.is_transitioning());
}

#[test]
fn terminal_pop_presents_exit_but_never_draws_an_entering_scene() {
    let log = Log::default();
    let samples = Samples::default();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.pop_scene_with_result(7_u32)
        .transition(around(samples.clone()));
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    assert!(!app.is_exited());
    run(&mut app, 1.).unwrap();
    assert!(app.is_exited());
    assert_eq!(app.scene_count(), 0);
    assert_eq!(samples.borrow().len(), 2);
    assert!(!app.is_transitioning());
}

#[test]
fn instant_segments_still_present_one_endpoint_each_before_switch_and_retire() {
    let log = Log::default();
    let samples = Rc::new(RefCell::new(Vec::new()));
    let copies = samples.clone();
    let mut app = app(Probe::new("root", &log));
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("child", &log))
        .transition(SceneTransition::overlay(
            TransitionTiming::AroundSwitch {
                exit: Transition::Instant,
                enter: Transition::Instant,
            },
            move |frame| {
                copies.borrow_mut().push(frame);
                Ok(())
            },
        ));
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 1);
    run(&mut app, 1.).unwrap();
    assert_eq!(app.scene_count(), 2);
    assert!(app.is_transitioning());
    run(&mut app, 1.).unwrap();
    assert!(!app.is_transitioning());
    assert_eq!(samples.borrow().len(), 2);
    assert!(
        samples
            .borrow()
            .iter()
            .all(|frame| frame.linear_progress == 1.)
    );
}

#[test]
fn headless_composite_error_drops_destination_without_double_updating_source() {
    let log = Log::default();
    let root = Probe::new("root", &log).with_node();
    let ticks = root.node_ticks.clone();
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    app.push_scene(Probe::new("child", &log))
        .transition(SceneTransition::crossfade(Transition::linear(0.1)));
    assert!(run(&mut app, 0.1).is_err());
    assert_eq!(count(&log, "root", "source"), 1);
    assert_eq!(&*ticks.borrow(), &[0.1]);
    assert_eq!(count(&log, "root", "update"), 1);
    assert!(app.is_faulted());
}

#[test]
fn raw_batches_are_complete_during_both_phases_and_across_switch() {
    let log = Log::default();
    let observed = Rc::new(RefCell::new(Vec::<String>::new()));
    let recorder = |observed: Rc<RefCell<Vec<String>>>| -> Hook {
        Box::new(move |event, _| {
            if let SceneEvent::Input(input) = event {
                observed.borrow_mut().push(format!("{:?}", input.events()));
            }
            Ok(())
        })
    };
    let mut root = Probe::new("root", &log);
    root.events = Some(recorder(observed.clone()));
    let mut child = Probe::new("child", &log);
    child.events = Some(recorder(observed.clone()));
    let mut app = app(root);
    run(&mut app, 0.1).unwrap();
    let down =
        InputBatch::recorded(|events| events.touch_event(TouchPhase::Started, 7, 2., 3., 12.));
    let expected_down = format!("{:?}", down.events());
    app.run_checked_frame(down, frame(0.1), || {}, |_, _| {})
        .unwrap();
    app.push_scene(child).transition(around(Rc::default()));
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    run(&mut app, 1.).unwrap();
    let continuing = InputBatch::recorded(|events| {
        events.touch_event(TouchPhase::Moved, 7, 4., 5., 12.5);
        events.touch_event(TouchPhase::Ended, 7, 4., 5., 12.75);
    });
    let expected_continuing = format!("{:?}", continuing.events());
    app.run_checked_frame(continuing, frame(0.1), || {}, |_, _| {})
        .unwrap();
    assert!(observed.borrow().contains(&expected_down));
    assert_eq!(observed.borrow().last().unwrap(), &expected_continuing);
}
