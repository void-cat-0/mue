use super::*;
use crate::{
    event::{hit_test::tests::TestNode, wheel::WheelEvent},
    gesture::{GesturePhase, TapGesture},
    math::{Matrix, Rect},
    node::Node,
    style::{Style, Styleable},
};
use miniquad::TouchPhase;
use mue_core::{Disposable, prop::Prop, signal::signal};
use std::cell::Cell;

fn manager(root: NodeRef) -> PointerManager {
    PointerManager {
        root_node: root,
        subscriber: None,
        tracks: HashMap::new(),
        hover: HoverState::default(),
        dpi_scale: 1.,
        planned_path: None,
    }
}

fn dispatch(input: &mut PointerManager, record: impl FnOnce(&mut dyn EventHandler)) {
    let batch = InputBatch::recorded(record);
    let plan = InputPlan::new(input.root_node, &batch, &input.hover, 1.);
    input.dispatch_plan(plan);
}

fn on_pointer(node: NodeRef, callback: impl FnMut(&PointerEvent) + 'static) {
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[node.id]
            .hooks
            .pointer_event
            .append(callback)
    });
}

fn on_wheel(node: NodeRef, callback: impl FnMut(&WheelEvent) + 'static) {
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[node.id]
            .hooks
            .wheel_event
            .append(callback)
    });
}

fn attach_gesture(node: NodeRef, gesture: impl Gesture + 'static) -> GestureId {
    Runtime::with(|rt| {
        let id = rt.gestures.borrow_mut().insert(Some(Box::new(gesture)));
        rt.nodes.borrow_mut()[node.id].gestures.push(id);
        id
    })
}

fn record_down(node: NodeRef, label: &'static str, events: Rc<RefCell<Vec<&'static str>>>) {
    on_pointer(node, move |event| {
        if event.action() == PointerAction::Down {
            events.borrow_mut().push(label);
        }
    });
}

fn styled_node(style: Style) -> Node {
    let node = Node::leaf(style, |style| {
        crate::layout::use_layout(style);
    });
    let rect = Runtime::with(|rt| rt.nodes.borrow()[node.id].layout.rect);
    rect.set(Rect::new(0., 0., 30., 30.));
    node
}

#[test]
fn focus_cancel_revokes_capture_before_later_events_without_retargeting() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(target.0, move |event| out.borrow_mut().push(event.action()));
    let mut input = manager(target.0);
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.window_minimized_event();
        events.touch_event(TouchPhase::Moved, 1, 6., 5., 0.);
        events.touch_event(TouchPhase::Ended, 1, 6., 5., 0.);
        events.window_restored_event();
        events.touch_event(TouchPhase::Started, 2, 5., 5., 0.);
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 0.);
    });
    assert_eq!(
        *actions.borrow(),
        vec![
            PointerAction::Down,
            PointerAction::Cancel,
            PointerAction::Down,
            PointerAction::Up
        ]
    );
    assert!(input.tracks.is_empty());
}

#[test]
fn two_touch_downs_keep_the_displayed_target_when_the_first_opens_an_overlay() {
    let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let background = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let target = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let cover = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let overlay = TestNode::new(None, &[]);
    let root = TestNode::new(None, &[background.0, left.0, target.0, overlay.0]);
    let events = Rc::new(RefCell::new(Vec::new()));
    record_down(background.0, "background", events.clone());
    record_down(target.0, "target", events.clone());
    record_down(cover.0, "overlay", events.clone());
    let (target_ref, overlay_ref, cover_ref) = (target.0, overlay.0, cover.0);
    let out = events.clone();
    on_pointer(left.0, move |event| {
        if event.action() != PointerAction::Down {
            return;
        }
        out.borrow_mut().push("left");
        // Both geometry and the visible child list change during the first Down.
        let children = (*signal(Rc::<[NodeRef]>::from([cover_ref]))).owned();
        let old = Runtime::with(|rt| {
            let mut nodes = rt.nodes.borrow_mut();
            nodes[target_ref.id].matrix_transform =
                Prop::Static(Matrix::from_translation(vec2(100., 0.)));
            std::mem::replace(&mut nodes[overlay_ref.id].children, children)
        });
        old.dispose();
    });
    let mut input = manager(root.0);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.touch_event(TouchPhase::Started, 2, 35., 5., 0.);
    });
    assert_eq!(*events.borrow(), vec!["left", "target"]);

    // A fresh batch sees the overlay that the earlier callback actually opened.
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 3, 35., 5., 0.);
    });
    assert_eq!(*events.borrow(), vec!["left", "target", "overlay"]);
}

#[test]
fn down_and_up_in_one_batch_tap_once_for_touch_and_mouse() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let taps = Rc::new(Cell::new(0));
    let out = taps.clone();
    let mut tap = TapGesture::default();
    tap.on_tap.append(move |_| out.set(out.get() + 1));
    attach_gesture(target.0, tap);
    let mut input = manager(target.0);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.touch_event(TouchPhase::Ended, 1, 5., 5., 0.);
    });
    assert_eq!(taps.get(), 1);
    assert!(input.tracks.is_empty());

    dispatch(&mut input, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    assert_eq!(taps.get(), 2);
    dispatch(&mut input, |_| {});
    assert_eq!(taps.get(), 2);
}

#[test]
fn captured_move_and_up_stay_on_the_down_target_outside_its_bounds() {
    let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let right = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let root = TestNode::new(None, &[left.0, right.0]);
    let events = Rc::new(RefCell::new(Vec::new()));
    for (node, label) in [(left.0, "left"), (right.0, "right")] {
        let out = events.clone();
        on_pointer(node, move |event| {
            out.borrow_mut()
                .push((label, event.action(), event.position()));
        });
    }
    let mut input = manager(root.0);
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
    });
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Moved, 1, 35., 5., 0.);
        events.touch_event(TouchPhase::Ended, 1, 35., 5., 0.);
    });

    assert_eq!(
        *events.borrow(),
        vec![
            ("left", PointerAction::Down, vec2(5., 5.)),
            ("left", PointerAction::Move, vec2(35., 5.)),
            ("left", PointerAction::Up, vec2(35., 5.)),
        ]
    );
    assert!(input.tracks.is_empty());
}

#[test]
fn a_disposed_planned_target_drops_the_route_without_falling_back() {
    let first = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let background = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let target = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let root = TestNode::new(None, &[background.0, first.0, target.0]);
    let events = Rc::new(RefCell::new(Vec::new()));
    record_down(background.0, "background", events.clone());
    record_down(target.0, "target", events.clone());
    record_down(root.0, "root", events.clone());
    let disposed = target.0;
    let out = events.clone();
    on_pointer(first.0, move |event| {
        if event.action() == PointerAction::Down {
            out.borrow_mut().push("first");
            disposed.dispose();
        }
    });
    let mut input = manager(root.0);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.touch_event(TouchPhase::Started, 2, 35., 5., 0.);
        events.touch_event(TouchPhase::Moved, 2, 36., 5., 0.);
        events.touch_event(TouchPhase::Ended, 2, 36., 5., 0.);
    });
    // Even the planned target's live ancestor must not receive that stale Down.
    assert_eq!(*events.borrow(), vec!["first", "root"]);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 3, 35., 5., 0.);
    });
    assert_eq!(
        *events.borrow(),
        vec!["first", "root", "background", "root"]
    );
}

#[test]
fn tap_callback_can_dispose_its_node_and_create_a_working_replacement() {
    let current = Rc::new(Cell::new(NodeRef::null()));
    let replacement = Rc::new(RefCell::new(None));
    let taps = Rc::new(Cell::new(0));
    let replacement_taps = Rc::new(Cell::new(0));
    let (self_ref, created, out, next_out) = (
        current.clone(),
        replacement.clone(),
        taps.clone(),
        replacement_taps.clone(),
    );
    let target = styled_node(Style::new().on_tap(move |_| {
        out.set(out.get() + 1);
        self_ref.get().dispose();
        let next_out = next_out.clone();
        *created.borrow_mut() = Some(styled_node(Style::new().on_tap(move |_| {
            next_out.set(next_out.get() + 1);
        })));
    }));
    current.set(*target);
    let old_gesture = Runtime::with(|rt| rt.nodes.borrow()[target.id].gestures[0]);
    let root = TestNode::new(None, &[*target]);
    let mut input = manager(root.0);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.touch_event(TouchPhase::Ended, 1, 5., 5., 0.);
    });
    assert_eq!(taps.get(), 1);
    Runtime::with(|rt| {
        assert!(!rt.nodes.borrow().contains_key(target.id));
        assert!(!rt.gestures.borrow().contains_key(old_gesture));
    });
    let next = **replacement
        .borrow()
        .as_ref()
        .expect("tap created a replacement");
    let next_root = TestNode::new(None, &[next]);
    input.root_node = next_root.0;
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 0.);
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 0.);
    });
    assert_eq!(replacement_taps.get(), 1);
    assert_eq!(taps.get(), 1);
}

#[test]
fn pan_callback_can_dispose_its_node_and_create_a_working_replacement() {
    let current = Rc::new(Cell::new(NodeRef::null()));
    let replacement = Rc::new(RefCell::new(None));
    let phases = Rc::new(RefCell::new(Vec::new()));
    let replacement_taps = Rc::new(Cell::new(0));
    let (self_ref, created, out, next_out) = (
        current.clone(),
        replacement.clone(),
        phases.clone(),
        replacement_taps.clone(),
    );
    let target = styled_node(Style::new().on_pan(move |event| {
        out.borrow_mut().push(event.phase);
        if event.phase == GesturePhase::Start {
            self_ref.get().dispose();
            let next_out = next_out.clone();
            *created.borrow_mut() = Some(styled_node(Style::new().on_tap(move |_| {
                next_out.set(next_out.get() + 1);
            })));
        }
    }));
    current.set(*target);
    let old_gesture = Runtime::with(|rt| rt.nodes.borrow()[target.id].gestures[0]);
    let root = TestNode::new(None, &[*target]);
    let mut input = manager(root.0);

    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.touch_event(TouchPhase::Moved, 1, 20., 5., 0.);
        events.touch_event(TouchPhase::Ended, 1, 25., 5., 0.);
    });
    assert_eq!(*phases.borrow(), vec![GesturePhase::Start]);
    Runtime::with(|rt| {
        assert!(!rt.nodes.borrow().contains_key(target.id));
        assert!(!rt.gestures.borrow().contains_key(old_gesture));
    });
    let next = **replacement
        .borrow()
        .as_ref()
        .expect("pan created a replacement");
    let next_root = TestNode::new(None, &[next]);
    input.root_node = next_root.0;
    dispatch(&mut input, |events| {
        events.touch_event(TouchPhase::Started, 2, 5., 5., 0.);
        events.touch_event(TouchPhase::Ended, 2, 5., 5., 0.);
    });
    assert_eq!(replacement_taps.get(), 1);
}

#[test]
fn mouse_buttons_share_capture_until_the_last_button_is_released() {
    let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let right = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let root = TestNode::new(None, &[left.0, right.0]);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let out = actions.clone();
    on_pointer(left.0, move |event| {
        out.borrow_mut()
            .push((event.action(), event.button_state()))
    });
    let right_downs = Rc::new(Cell::new(0));
    let out = right_downs.clone();
    on_pointer(right.0, move |event| {
        if event.action() == PointerAction::Down {
            out.set(out.get() + 1);
        }
    });
    let mut input = manager(root.0);
    dispatch(&mut input, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_motion_event(35., 5.);
        events.mouse_button_down_event(MouseButton::Right, 35., 5.);
        events.mouse_button_up_event(MouseButton::Left, 35., 5.);
        events.mouse_button_up_event(MouseButton::Right, 35., 5.);
    });
    assert_eq!(right_downs.get(), 0);
    assert_eq!(
        actions
            .borrow()
            .iter()
            .filter(|(a, _)| *a == PointerAction::Down)
            .count(),
        1
    );
    assert_eq!(
        actions
            .borrow()
            .iter()
            .filter(|(a, _)| *a == PointerAction::Up)
            .count(),
        1
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&(PointerAction::Up, ButtonState::empty()))
    );
    assert!(
        actions
            .borrow()
            .iter()
            .any(|(_, b)| *b == (ButtonState::PRIMARY | ButtonState::SECONDARY))
    );
}

#[test]
fn a_claim_token_from_the_previous_mouse_press_never_revives() {
    struct SaveTokens(Rc<RefCell<Vec<ClaimToken>>>);
    impl Gesture for SaveTokens {
        fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
            if event.action() == PointerAction::Down {
                self.0.borrow_mut().push(token.clone());
            }
        }
    }
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let tokens = Rc::new(RefCell::new(Vec::new()));
    attach_gesture(target.0, SaveTokens(tokens.clone()));
    let mut input = manager(target.0);
    dispatch(&mut input, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    let old = tokens.borrow()[0].clone();
    assert!(!old.can_claim());

    dispatch(&mut input, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
    });
    assert_eq!(tokens.borrow().len(), 2);
    let fresh = tokens.borrow()[1].clone();
    assert_eq!(old.pointer_id(), fresh.pointer_id());
    assert!(!old.can_claim());
    assert!(!old.claim());
    assert!(fresh.can_claim());
    assert!(fresh.claim());
    dispatch(&mut input, |events| {
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    assert!(!old.can_claim());
    assert!(!fresh.can_claim());
}

#[test]
fn wheel_only_batches_keep_mouse_position_until_suspension() {
    let target = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let wheels = Rc::new(Cell::new(0));
    let out = wheels.clone();
    on_wheel(target.0, move |_| out.set(out.get() + 1));
    let mut input = manager(target.0);
    input.hover.observe_mouse(vec2(35., 5.));
    input.hover.rehit(target.0);
    dispatch(&mut input, |events| events.mouse_wheel_event(0., 1.));
    assert_eq!(wheels.get(), 1);

    input.pause_input();
    dispatch(&mut input, |events| events.mouse_wheel_event(0., 1.));
    assert_eq!(wheels.get(), 1, "suspend still requires a fresh position");
}

#[test]
fn wheel_keeps_its_planned_path_and_bubbles_remainder_after_an_earlier_callback_moves_it() {
    let first = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let background = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let target = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
    let root = TestNode::new(None, &[background.0, first.0, target.0]);
    let target_ref = target.0;
    on_pointer(first.0, move |event| {
        if event.action() == PointerAction::Down {
            Runtime::with(|rt| {
                rt.nodes.borrow_mut()[target_ref.id].matrix_transform =
                    Prop::Static(Matrix::from_translation(vec2(100., 0.)));
            });
        }
    });
    let wheels = Rc::new(RefCell::new(Vec::new()));
    for (node, label) in [
        (target.0, "target"),
        (background.0, "background"),
        (root.0, "root"),
    ] {
        let out = wheels.clone();
        on_wheel(node, move |event| {
            out.borrow_mut()
                .push((label, event.position, event.delta, event.remaining_delta()));
            if label == "target" {
                event.consume_delta(vec2(0., 1.));
            }
        });
    }
    let mut input = manager(root.0);
    dispatch(&mut input, |events| {
        events.mouse_motion_event(35., 5.);
        events.touch_event(TouchPhase::Started, 1, 5., 5., 0.);
        events.mouse_wheel_event(0., 3.);
    });
    assert_eq!(
        *wheels.borrow(),
        vec![
            ("target", vec2(35., 5.), vec2(0., 3.), vec2(0., 3.)),
            ("root", vec2(35., 5.), vec2(0., 3.), vec2(0., 2.)),
        ]
    );

    dispatch(&mut input, |events| events.mouse_wheel_event(0., 3.));
    assert_eq!(
        &wheels.borrow()[2..],
        &[
            ("background", vec2(35., 5.), vec2(0., 3.), vec2(0., 3.)),
            ("root", vec2(35., 5.), vec2(0., 3.), vec2(0., 3.)),
        ]
    );
}

#[test]
fn hover_self_disposal_does_not_bubble_a_stale_enter_to_ancestors() {
    let target = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
    let root = TestNode::new(None, &[target.0]);
    let enters = Rc::new(Cell::new(0));
    let target_ref = target.0;
    let out = enters.clone();
    Runtime::with(|rt| {
        let mut nodes = rt.nodes.borrow_mut();
        nodes[target_ref.id].hooks.hover_event.append(move |event| {
            if event.action() == PointerAction::Down {
                target_ref.dispose();
            }
        });
        nodes[root.0.id].hooks.hover_event.append(move |event| {
            if event.action() == PointerAction::Down {
                out.set(out.get() + 1);
            }
        });
    });
    let mut input = manager(root.0);
    dispatch(&mut input, |events| events.mouse_motion_event(5., 5.));
    assert_eq!(enters.get(), 0);
    assert!(input.tracks[&PointerId::MOUSE].hover_nodes.is_empty());
}

#[test]
fn hover_callback_layout_changes_do_not_retarget_a_down_in_the_same_batch() {
    let rect = Some(Rect::new(0., 0., 20., 20.));
    let background = TestNode::new(rect, &[]);
    let target = TestNode::new(rect, &[]);
    let root = TestNode::new(None, &[background.0, target.0]);
    let events = Rc::new(RefCell::new(Vec::new()));
    record_down(background.0, "background", events.clone());
    record_down(target.0, "target", events.clone());
    let target_ref = target.0;
    let out = events.clone();
    Runtime::with(|rt| {
        rt.nodes.borrow_mut()[target_ref.id]
            .hooks
            .hover_event
            .append(move |event| {
                if event.action() == PointerAction::Down {
                    out.borrow_mut().push("hover");
                    Runtime::with(|rt| {
                        rt.nodes.borrow_mut()[target_ref.id].matrix_transform =
                            Prop::Static(Matrix::from_translation(vec2(100., 0.)));
                    });
                }
            });
    });
    let mut input = manager(root.0);
    dispatch(&mut input, |events| {
        events.mouse_motion_event(5., 5.);
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    assert_eq!(*events.borrow(), vec!["hover", "target"]);

    dispatch(&mut input, |events| {
        events.mouse_button_down_event(MouseButton::Left, 5., 5.);
        events.mouse_button_up_event(MouseButton::Left, 5., 5.);
    });
    assert_eq!(*events.borrow(), vec!["hover", "target", "background"]);
}
