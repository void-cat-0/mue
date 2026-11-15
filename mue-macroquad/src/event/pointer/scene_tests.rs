use super::*;
use crate::{event::hit_test::tests::TestNode, math::Rect};
use miniquad::TouchPhase;
use mue_core::{Disposable, prop::Prop, signal::signal};

fn root(node: NodeRef, bounds: Rect, input: RootInput) -> SceneRoot {
    SceneRoot {
        node,
        bounds,
        input,
    }
}

fn on_pointer(node: NodeRef, callback: impl FnMut(&PointerEvent) + 'static) {
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[node.id]
            .hooks
            .pointer_event
            .append(callback)
    });
}

// Return the original batch for assertions; dispatch only reads it. App, rather
// than this helper, passes that same unfiltered batch as SceneEvent::Input.
fn dispatch(
    manager: &mut PointerManager,
    roots: &[SceneRoot],
    record: impl FnOnce(&mut dyn EventHandler),
) -> InputBatch {
    let input = InputBatch::recorded(record);
    let plan = manager.plan_roots(roots, &input);
    manager.dispatch_roots(plan, roots, &input);
    input
}

#[test]
fn multiple_roots_preplan_all_hits_before_overlay_callbacks() {
    let bounds = Rect::new(0., 0., 80., 20.);
    let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let target = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let back = TestNode::new(None, &[left.0, target.0]);
    let cover = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let overlay = TestNode::new(None, &[]);
    let events = Rc::new(RefCell::new(Vec::new()));
    for (node, label) in [(target.0, "target"), (cover.0, "overlay")] {
        let out = events.clone();
        on_pointer(node, move |event| {
            if event.action() == PointerAction::Down {
                out.borrow_mut().push(label);
            }
        });
    }
    let (target_ref, overlay_ref, cover_ref) = (target.0, overlay.0, cover.0);
    let out = events.clone();
    on_pointer(left.0, move |event| {
        if event.action() == PointerAction::Down {
            out.borrow_mut().push("left");
            let children = (*signal(Rc::<[NodeRef]>::from([cover_ref]))).owned();
            let old = Runtime::with(|rt| {
                let mut nodes = rt.nodes.borrow_mut();
                nodes[target_ref.id].matrix_transform =
                    Prop::Static(crate::math::Matrix::from_translation(vec2(100., 0.)));
                std::mem::replace(&mut nodes[overlay_ref.id].children, children)
            });
            old.dispose();
        }
    });
    let roots = [
        root(back.0, bounds, RootInput::Interactive),
        root(overlay.0, bounds, RootInput::Interactive),
    ];
    let mut manager = PointerManager::headless();
    let input = dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 1.);
        events.touch_event(TouchPhase::Started, 2, 35., 5., 2.);
    });
    assert_eq!(input.events().len(), 2);
    assert_eq!(*events.borrow(), vec!["left", "target"]);
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 3, 35., 5., 3.);
    });
    assert_eq!(*events.borrow(), vec!["left", "target", "overlay"]);
}

#[test]
fn root_order_none_and_modal_only_control_node_paths() {
    let bounds = Rect::new(0., 0., 60., 20.);
    let back = TestNode::new(Some(bounds), &[]);
    let front = TestNode::new(Some(bounds), &[]);
    let empty = TestNode::new(None, &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    for (node, label) in [(back.0, "back"), (front.0, "front")] {
        let out = actions.clone();
        on_pointer(node, move |event| {
            if event.action() == PointerAction::Down {
                out.borrow_mut().push(label);
            }
        });
    }
    let mut manager = PointerManager::headless();
    let mut roots = vec![
        root(back.0, bounds, RootInput::Interactive),
        root(front.0, bounds, RootInput::Interactive),
        root(empty.0, bounds, RootInput::Interactive),
    ];
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.)
    });
    assert_eq!(*actions.borrow(), vec!["front"]);
    roots[1].input = RootInput::None;
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 0.)
    });
    assert_eq!(*actions.borrow(), vec!["front", "back"]);
    roots[2].input = RootInput::Modal;
    roots[2].bounds = Rect::new(20., 0., 20., 20.);
    assert!(hit_scene_roots(&roots, vec2(25., 5.)).contains(&empty.0));
    assert_eq!(hit_scene_roots(&roots, vec2(25., 5.)).len(), 1);
    assert!(hit_scene_roots(&roots, vec2(5., 5.)).contains(&back.0));
    let input = dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 3, 25., 5., 0.);
        events.touch_event(TouchPhase::Started, 4, 5., 5., 0.);
        events.touch_event(TouchPhase::Started, 5, 100., 5., 0.);
    });
    assert_eq!(input.events().len(), 3);
    assert_eq!(*actions.borrow(), vec!["front", "back", "back"]);
    assert!(
        manager.tracks[&PointerId::from_touch_id(3)]
            .nodes
            .contains(&empty.0)
    );
}

#[test]
fn captured_touch_crosses_bounds_and_removed_root_cancels_only_its_node_operation() {
    let bounds = Rect::new(0., 0., 20., 20.);
    let target = TestNode::new(Some(bounds), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let roots = [root(target.0, bounds, RootInput::Interactive)];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.)
    });
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Moved, 1, 100., 5., 1.)
    });
    let input = dispatch(&mut manager, &[], |events| {
        events.touch_event(TouchPhase::Moved, 1, 110., 5., 2.);
        events.touch_event(TouchPhase::Ended, 1, 110., 5., 3.);
    });
    assert_eq!(input.events().len(), 2);
    assert!(path_is_live(&IndexSet::from([target.0])));
    assert_eq!(
        *actions.borrow(),
        vec![
            PointerAction::Down,
            PointerAction::Move,
            PointerAction::Cancel
        ]
    );
    assert!(manager.tracks.is_empty());

    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 4.)
    });
    let closed = [root(target.0, bounds, RootInput::None)];
    dispatch(&mut manager, &closed, |_| {});
    dispatch(&mut manager, &closed, |_| {});
    let input = dispatch(&mut manager, &closed, |events| {
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 5.)
    });
    assert_eq!(input.events().len(), 1);
    assert_eq!(
        actions
            .borrow()
            .iter()
            .filter(|action| **action == PointerAction::Cancel)
            .count(),
        2
    );
    assert!(manager.tracks.is_empty());
}

#[test]
fn blank_touch_down_does_not_retarget_into_ui_and_raw_values_are_unchanged() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| {
        out.borrow_mut().push((event.pointer_id(), event.action()))
    });
    let roots = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    manager.dpi_scale = 2.;
    let input = dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 90.125, 20.75, 12345.678901234);
        events.touch_event(TouchPhase::Started, 2, 10., 10., 98765.4321);
    });
    assert!(
        matches!(input.events(), [InputEvent::Touch { id: 1, x, y, time, .. }, InputEvent::Touch { id: 2, .. }]
        if *x == 90.125 && *y == 20.75 && *time == 12345.678901234)
    );
    let input = dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Moved, 1, 10.5, 12.25, 12346.987654321);
        events.touch_event(TouchPhase::Ended, 1, 11.75, 13.5, 12347.111111111);
        events.touch_event(TouchPhase::Moved, 2, 100., 100., 98766.4321);
        events.touch_event(TouchPhase::Ended, 2, 100., 100., 98767.4321);
    });
    assert_eq!(input.events().len(), 4);
    assert!(
        matches!(input.events()[0], InputEvent::Touch { id: 1, x, y, time, .. }
        if x == 10.5 && y == 12.25 && time == 12346.987654321)
    );
    assert_eq!(
        *actions.borrow(),
        vec![
            (PointerId::from_touch_id(2), PointerAction::Down),
            (PointerId::from_touch_id(2), PointerAction::Move),
            (PointerId::from_touch_id(2), PointerAction::Up)
        ]
    );
}

#[test]
fn mouse_empty_capture_and_multi_buttons_do_not_retarget_but_wheel_uses_cursor() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let wheels = Rc::new(RefCell::new(Vec::new()));
    let out = wheels.clone();
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[target.0.id]
            .hooks
            .wheel_event
            .append(move |_| out.borrow_mut().push(()))
    });
    let roots = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &roots, |events| {
        events.mouse_button_down_event(MouseButton::Left, 50., 5.)
    });
    // This is a different batch, so an empty capture must not be canceled by
    // root validation and then become a new press on the target.
    let input = dispatch(&mut manager, &roots, |events| {
        events.mouse_motion_event(5., 5.);
        events.mouse_button_down_event(MouseButton::Right, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
        events.mouse_wheel_event(0., 1.);
        events.mouse_button_up_event(MouseButton::Right, 5., 6.);
    });
    assert_eq!(input.events().len(), 5);
    assert!(actions.borrow().is_empty());
    assert_eq!(wheels.borrow().len(), 1);
    dispatch(&mut manager, &roots, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_down_event(MouseButton::Right, 50., 5.);
        events.mouse_button_up_event(MouseButton::Left, 50., 5.);
        events.mouse_motion_event(50., 6.);
        events.mouse_button_up_event(MouseButton::Right, 50., 6.);
    });
    assert_eq!(actions.borrow().first(), Some(&PointerAction::Down));
    assert_eq!(actions.borrow().last(), Some(&PointerAction::Up));
    assert_eq!(
        actions
            .borrow()
            .iter()
            .filter(|action| **action == PointerAction::Down)
            .count(),
        1
    );
}

#[test]
fn removed_mouse_root_cannot_reestablish_capture_with_another_button() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let other = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let other_actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let out = other_actions.clone();
    on_pointer(other.0, move |event| out.borrow_mut().push(event.action()));
    let first = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let next = [root(
        other.0,
        Rect::new(30., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &first, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.)
    });
    dispatch(&mut manager, &next, |events| {
        events.mouse_motion_event(35., 5.);
        events.mouse_button_down_event(MouseButton::Right, 35., 5.);
        events.mouse_button_up_event(MouseButton::Left, 35., 5.);
        events.mouse_button_up_event(MouseButton::Right, 35., 5.);
    });
    assert_eq!(
        *actions.borrow(),
        vec![PointerAction::Down, PointerAction::Cancel]
    );
    assert!(other_actions.borrow().is_empty());
    dispatch(&mut manager, &next, |events| {
        events.mouse_button_down_event(MouseButton::Left, 35., 5.);
        events.mouse_button_up_event(MouseButton::Left, 35., 5.);
    });
    assert_eq!(other_actions.borrow().first(), Some(&PointerAction::Down));
    assert_eq!(other_actions.borrow().last(), Some(&PointerAction::Up));
}

#[test]
fn removed_root_revokes_old_gesture_tokens_once() {
    struct Recorder(
        Rc<RefCell<Vec<ClaimToken>>>,
        Rc<RefCell<Vec<PointerAction>>>,
        Rc<std::cell::Cell<usize>>,
    );
    impl Gesture for Recorder {
        fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
            self.1.borrow_mut().push(event.action());
            if event.action() == PointerAction::Down {
                self.0.borrow_mut().push(token.clone());
            }
        }
        fn on_rejected(&mut self, _pointer: PointerId) {
            self.2.set(self.2.get() + 1);
        }
    }
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let tokens = Rc::new(RefCell::new(Vec::new()));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let rejected = Rc::new(std::cell::Cell::new(0));
    Runtime::with(|rt| {
        let id = rt.gestures.borrow_mut().insert(Some(Box::new(Recorder(
            tokens.clone(),
            actions.clone(),
            rejected.clone(),
        ))));
        rt.nodes.borrow_mut()[target.0.id].gestures.push(id);
    });
    let roots = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.)
    });
    assert!(tokens.borrow()[0].can_claim());
    dispatch(&mut manager, &[], |_| {});
    dispatch(&mut manager, &[], |_| {});
    assert!(!tokens.borrow()[0].can_claim());
    assert_eq!(
        rejected.get(),
        0,
        "live capture cancellation is not a second rejection"
    );
    assert_eq!(
        *actions.borrow(),
        vec![PointerAction::Down, PointerAction::Cancel]
    );
}

#[test]
fn hover_and_wheel_only_roots_keep_pointer_hit_query_compatible_and_batch_intact() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let hovers = Rc::new(RefCell::new(Vec::new()));
    let out = hovers.clone();
    let wheels = Rc::new(RefCell::new(Vec::new()));
    let out_wheel = wheels.clone();
    Runtime::with(|rt| {
        let mut nodes = rt.nodes.borrow_mut();
        nodes[target.0.id]
            .hooks
            .hover_event
            .append(move |event| out.borrow_mut().push(event.action()));
        nodes[target.0.id]
            .hooks
            .wheel_event
            .append(move |_| out_wheel.borrow_mut().push(()));
    });
    let roots = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    assert!(hit_scene_roots(&roots, vec2(5., 5.)).is_empty());
    let mut manager = PointerManager::headless();
    let input = dispatch(&mut manager, &roots, |events| {
        events.mouse_motion_event(5., 5.);
        events.mouse_wheel_event(0., -1.);
    });
    assert_eq!(input.events().len(), 2);
    assert_eq!(wheels.borrow().len(), 1);
    assert_eq!(*hovers.borrow(), vec![PointerAction::Down]);
}

#[test]
fn interruption_and_focus_cancel_node_state_without_rewriting_raw_events() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let roots = [root(
        target.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 1.)
    });
    let interrupted = InputBatch::from_events(
        vec![InputEvent::Touch {
            phase: TouchPhase::Moved,
            id: 1,
            x: 6.,
            y: 5.,
            time: 2.,
        }],
        true,
    );
    let plan = manager.plan_roots(&roots, &interrupted);
    manager.dispatch_roots(plan, &roots, &interrupted);
    assert!(interrupted.is_interrupted());
    assert!(
        matches!(interrupted.events(), [InputEvent::Touch { phase: TouchPhase::Moved, id: 1, time, .. }] if *time == 2.)
    );
    assert_eq!(
        *actions.borrow(),
        vec![PointerAction::Down, PointerAction::Cancel]
    );
    assert!(manager.tracks.is_empty());
    let input = dispatch(&mut manager, &roots, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 3.);
        events.window_minimized_event();
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 4.);
        events.window_restored_event();
        events.touch_event(TouchPhase::Started, 3, 5., 5., 5.);
        events.touch_event(TouchPhase::Ended, 3, 5., 5., 6.);
    });
    assert_eq!(input.events().len(), 6);
    assert_eq!(
        *actions.borrow(),
        vec![
            PointerAction::Down,
            PointerAction::Cancel,
            PointerAction::Down,
            PointerAction::Cancel,
            PointerAction::Down,
            PointerAction::Up
        ]
    );
}

#[test]
fn custom_first_frame_hit_and_plan_before_enter_style_mutation() {
    let target = TestNode::new(None, &[]);
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[target.0.id].hit_test = Some(Rc::new(|node, _, result| {
            result.hit_self(node);
        }));
    });
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let roots = [root(
        target.0,
        Rect::new(0., 0., 100., 100.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    let input =
        InputBatch::recorded(|events| events.touch_event(TouchPhase::Started, 1, 50., 50., 1.));
    let plan = manager.plan_roots(&roots, &input);
    Runtime::with(|rt| rt.nodes.borrow_mut()[target.0.id].hit_test = None);
    manager.dispatch_roots(plan, &roots, &input);
    assert_eq!(*actions.borrow(), vec![PointerAction::Down]);
    assert_eq!(input.events().len(), 1);
}

#[test]
fn canceled_ui_capture_does_not_turn_unmatched_up_into_a_new_node_tap() {
    use crate::gesture::TapGesture;
    let first = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let second = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let taps = Rc::new(RefCell::new(Vec::new()));
    for (node, label) in [(first.0, "old"), (second.0, "new")] {
        let out = taps.clone();
        let mut tap = TapGesture::default();
        tap.on_tap.append(move |_| out.borrow_mut().push(label));
        Runtime::with(|rt| {
            let id = rt.gestures.borrow_mut().insert(Some(Box::new(tap)));
            rt.nodes.borrow_mut()[node.id].gestures.push(id);
        });
    }
    let old = [root(
        first.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let new = [root(
        second.0,
        Rect::new(0., 0., 20., 20.),
        RootInput::Interactive,
    )];
    let mut manager = PointerManager::headless();
    dispatch(&mut manager, &old, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 1.);
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
    });
    manager.cancel_input();
    dispatch(&mut manager, &new, |events| {
        events.touch_event(TouchPhase::Moved, 1, 6., 5., 2.);
        events.touch_event(TouchPhase::Ended, 1, 6., 5., 3.);
        events.mouse_motion_event(6., 5.);
        events.mouse_button_up_event(MouseButton::Left, 6., 5.);
    });
    assert!(taps.borrow().is_empty());
    dispatch(&mut manager, &new, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 4.);
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 5.);
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    assert_eq!(*taps.borrow(), vec!["new", "new"]);
}
