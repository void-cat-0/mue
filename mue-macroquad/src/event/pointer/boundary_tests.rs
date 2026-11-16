use super::*;
use crate::event::hit_test::tests::TestNode;
use miniquad::TouchPhase;
use mue_core::{
    Disposable,
    signal::{Access, signal},
};

fn manager(root: NodeRef, dpi: f32) -> PointerManager {
    let mut manager = PointerManager::headless();
    manager.root_node = root;
    manager.dpi_scale = dpi;
    manager
        .hover
        .set_window_bounds(Some(Rect::new(0., 0., 100., 60.)));
    manager
}

fn dispatch(manager: &mut PointerManager, input: &InputBatch, planned: bool) {
    if planned {
        let plan = InputPlan::new(manager.root_node, input, &manager.hover, manager.dpi_scale);
        manager.dispatch_plan(plan);
    } else {
        input.repeat(manager);
        manager.refresh_hover(false);
    }
}

fn on_pointer(node: NodeRef, callback: impl FnMut(&PointerEvent) + 'static) {
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[node.id]
            .hooks
            .pointer_event
            .append(callback);
    });
}

#[test]
fn geometry_cancels_outside_capture_without_native_leave_and_recovers_without_enter() {
    for planned in [false, true] {
        // A Node may draw beyond the window; Node bounds are not mouse bounds.
        let target = TestNode::new(Some(Rect::new(-50., -50., 200., 160.)), &[]);
        let hovered = signal(false);
        Runtime::with(|rt| rt.nodes.borrow_mut()[target.0.id].hovered = Some(hovered));
        let actions = Rc::new(RefCell::new(Vec::new()));
        let out = actions.clone();
        on_pointer(target.0, move |event| {
            out.borrow_mut().push(event.action());
        });
        let mut manager = manager(target.0, 1.);
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                // A first motion/down does not require a preceding native enter.
                events.mouse_motion_event(5., 5.);
                events.mouse_button_down_event(MouseButton::Left, 5., 5.);
            }),
            planned,
        );
        assert!(hovered.get());
        let outside = InputBatch::recorded(|events| {
            events.mouse_motion_event(105., 5.);
            events.mouse_button_up_event(MouseButton::Left, 110., 5.);
            events.mouse_button_down_event(MouseButton::Right, -1., 5.);
        });
        dispatch(&mut manager, &outside, planned);
        assert!(!hovered.get());
        assert!(manager.hover.position().is_none());
        assert!(!manager.tracks.contains_key(&PointerId::MOUSE));
        assert_eq!(
            *actions.borrow(),
            [PointerAction::Down, PointerAction::Cancel]
        );
        assert_eq!(outside.events().len(), 3);

        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| events.mouse_motion_event(35., 5.)),
            planned,
        );
        assert!(hovered.get());
        let track = &manager.tracks[&PointerId::MOUSE];
        assert!(track.event.is_hover);
        assert!(track.event.button_state.is_empty());
        assert_eq!(manager.hover.position(), Some(vec2(35., 5.)));
        assert_eq!(
            *actions.borrow(),
            [PointerAction::Down, PointerAction::Cancel]
        );
        hovered.dispose();
    }
}

#[test]
fn leave_then_captured_outside_events_stay_invalid_until_fresh_enter() {
    for planned in [false, true] {
        let target = TestNode::new(Some(Rect::new(-50., -50., 200., 160.)), &[]);
        let mut manager = manager(target.0, 2.);
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                events.mouse_button_down_event(MouseButton::Left, 10., 10.);
                events.mouse_leave_event();
                events.mouse_motion_event(210., 10.);
                events.mouse_button_up_event(MouseButton::Left, -10., 10.);
            }),
            planned,
        );
        assert!(manager.hover.position().is_none());
        assert!(manager.tracks.is_empty());
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                events.mouse_enter_event(MouseButton::Unknown, 70., 10.);
            }),
            planned,
        );
        assert_eq!(manager.hover.position(), Some(vec2(35., 5.)));
        let track = &manager.tracks[&PointerId::MOUSE];
        assert!(track.event.is_hover);
        assert!(track.event.button_state.is_empty());
    }
}

#[test]
fn focus_restore_does_not_reuse_cached_or_unfocused_mouse_coordinates() {
    for planned in [false, true] {
        let target = TestNode::new(Some(Rect::new(-50., -50., 200., 160.)), &[]);
        let mut manager = manager(target.0, 1.);
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                events.mouse_motion_event(5., 5.);
                events.mouse_button_down_event(MouseButton::Left, 5., 5.);
                events.window_minimized_event();
                events.mouse_enter_event(MouseButton::Unknown, 10., 5.);
                events.mouse_motion_event(15., 5.);
                events.mouse_button_down_event(MouseButton::Right, 15., 5.);
                events.window_restored_event();
                events.mouse_motion_event(110., 5.);
                events.mouse_button_up_event(MouseButton::Left, 110., 5.);
            }),
            planned,
        );
        assert!(manager.hover.position().is_none());
        assert!(manager.tracks.is_empty());
        dispatch(&mut manager, &InputBatch::default(), planned);
        assert!(manager.hover.position().is_none());
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| events.mouse_motion_event(20., 5.)),
            planned,
        );
        assert_eq!(manager.hover.position(), Some(vec2(20., 5.)));
        assert!(manager.tracks[&PointerId::MOUSE].event.is_hover);
    }
}

#[test]
fn mouse_window_boundary_never_cancels_a_concurrent_touch() {
    for planned in [false, true] {
        let target = TestNode::new(Some(Rect::new(-50., -50., 200., 160.)), &[]);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let out = actions.clone();
        on_pointer(target.0, move |event| {
            out.borrow_mut()
                .push((event.pointer_type(), event.action(), event.position()));
        });
        let mut manager = manager(target.0, 1.);
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                events.mouse_button_down_event(MouseButton::Left, 5., 5.);
                events.touch_event(TouchPhase::Started, 1, 5., 5., 10.);
                events.mouse_leave_event();
                events.mouse_motion_event(105., 5.);
                events.mouse_button_up_event(MouseButton::Left, 110., 5.);
                events.touch_event(TouchPhase::Moved, 1, 110., 5., 11.);
                events.touch_event(TouchPhase::Ended, 1, 115., 5., 12.);
            }),
            planned,
        );
        assert_eq!(
            *actions.borrow(),
            [
                (PointerType::Mouse, PointerAction::Down, vec2(5., 5.)),
                (PointerType::Touch, PointerAction::Down, vec2(5., 5.)),
                (PointerType::Mouse, PointerAction::Cancel, vec2(5., 5.)),
                (PointerType::Touch, PointerAction::Move, vec2(110., 5.)),
                (PointerType::Touch, PointerAction::Up, vec2(115., 5.)),
            ]
        );
        assert!(manager.tracks.is_empty());
    }
}

#[test]
fn mouse_capture_crosses_node_bounds_without_crossing_the_window() {
    for planned in [false, true] {
        let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let right = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
        let root = TestNode::new(None, &[left.0, right.0]);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let out = actions.clone();
        on_pointer(left.0, move |event| {
            out.borrow_mut().push((event.action(), event.position()));
        });
        let right_hover = signal(false);
        Runtime::with(|rt| rt.nodes.borrow_mut()[right.0.id].hovered = Some(right_hover));
        let mut manager = manager(root.0, 1.);
        dispatch(
            &mut manager,
            &InputBatch::recorded(|events| {
                events.mouse_button_down_event(MouseButton::Left, 5., 5.);
                events.mouse_motion_event(35., 5.);
                events.mouse_button_up_event(MouseButton::Left, 35., 5.);
            }),
            planned,
        );
        assert_eq!(
            *actions.borrow(),
            [
                (PointerAction::Down, vec2(5., 5.)),
                (PointerAction::Move, vec2(35., 5.)),
                // The existing mouse-up path confirms its release position.
                (PointerAction::Move, vec2(35., 5.)),
                (PointerAction::Up, vec2(35., 5.)),
            ]
        );
        assert!(right_hover.get());
        assert!(manager.tracks[&PointerId::MOUSE].event.is_hover);
        right_hover.dispose();
    }
}

#[test]
fn resize_and_dpi_changes_alone_do_not_cancel_mouse_capture() {
    let target = TestNode::new(Some(Rect::new(0., 0., 120., 60.)), &[]);
    let hovered = signal(false);
    Runtime::with(|rt| rt.nodes.borrow_mut()[target.0.id].hovered = Some(hovered));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let mut manager = manager(target.0, 1.);
    dispatch(
        &mut manager,
        &InputBatch::recorded(|events| {
            events.mouse_motion_event(80., 5.);
            events.mouse_button_down_event(MouseButton::Left, 80., 5.);
        }),
        true,
    );
    assert!(hovered.get());
    manager
        .hover
        .set_window_bounds(Some(Rect::new(0., 0., 60., 60.)));
    manager.dpi_scale = 2.;
    dispatch(&mut manager, &InputBatch::default(), true);
    assert!(!hovered.get());
    assert!(!manager.tracks[&PointerId::MOUSE].event.is_hover);
    assert_eq!(*actions.borrow(), [PointerAction::Down]);
    dispatch(
        &mut manager,
        &InputBatch::recorded(|events| {
            events.mouse_motion_event(40., 10.);
            events.mouse_button_up_event(MouseButton::Left, 40., 10.);
        }),
        true,
    );
    assert!(hovered.get());
    assert!(manager.tracks[&PointerId::MOUSE].event.is_hover);
    assert!(!actions.borrow().contains(&PointerAction::Cancel));
    assert_eq!(actions.borrow().last(), Some(&PointerAction::Up));
    hovered.dispose();
}

#[test]
fn preplanning_and_live_dispatch_agree_on_window_boundary_positions() {
    let target = TestNode::new(Some(Rect::new(-50., -50., 200., 160.)), &[]);
    let manager = manager(target.0, 2.);
    let input = InputBatch::recorded(|events| {
        events.mouse_motion_event(10., 10.);
        events.mouse_button_down_event(MouseButton::Left, 10., 10.);
        events.mouse_motion_event(200., 10.);
        events.mouse_button_up_event(MouseButton::Left, 210., 10.);
        events.mouse_motion_event(30., 10.);
        events.mouse_leave_event();
        events.mouse_motion_event(-1., 10.);
        events.mouse_enter_event(MouseButton::Unknown, 40., 10.);
        events.window_minimized_event();
        events.mouse_motion_event(50., 10.);
        events.window_restored_event();
        events.touch_event(TouchPhase::Started, 1, 10., 10., 7.);
        events.mouse_motion_event(60., 10.);
    });
    let plan = InputPlan::new(target.0, &input, &manager.hover, 2.);
    let expected = [
        Some(vec2(5., 5.)),
        Some(vec2(5., 5.)),
        None,
        None,
        Some(vec2(15., 5.)),
        None,
        None,
        Some(vec2(20., 5.)),
        None,
        None,
        None,
        Some(vec2(5., 5.)),
        Some(vec2(30., 5.)),
    ];
    assert_eq!(plan.events.len(), expected.len());
    for (planned, expected) in plan.events.iter().zip(expected) {
        assert_eq!(planned.position, expected);
        assert_eq!(planned.path.contains(&target.0), expected.is_some());
    }
    let mut live = manager;
    input.repeat(&mut live);
    assert_eq!(live.hover.position(), Some(vec2(30., 5.)));
    assert!(plan.hover == IndexSet::from([target.0]));
    assert!(live.tracks[&PointerId::MOUSE].event.is_hover);
}
