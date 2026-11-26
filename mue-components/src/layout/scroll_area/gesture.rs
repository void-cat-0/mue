//! Dragging content and scrollbars.

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::gesture::Gesture,
    input::pointer::{
        ButtonState, ClaimToken, PointerAction, PointerEvent, PointerId, PointerType,
    },
    math::Vector,
};

use super::{ScrollAxis, state::ScrollState};

pub(super) struct Drag {
    id: PointerId,
    origin: Vector,
    previous: Vector,
    time: f32,
    claimed: bool,
    axis: ScrollAxis,
    bar: Option<usize>,
}

pub(super) struct ScrollGesture {
    pub(super) state: Rc<RefCell<ScrollState>>,
}

impl Gesture for ScrollGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        let mut state = self.state.borrow_mut();
        let options = state.options;
        if let Some(drag) = &state.drag {
            if drag.id != event.pointer_id() {
                // A second finger stops the fling/drag; it is not treated as another pan.
                state.drag = None;
                state.velocity = Vector::ZERO;
                token.dismiss();
                return;
            }
        } else if event.action() == PointerAction::Down {
            let layout = state.bar_layout();
            let touch = event.pointer_type() == PointerType::Touch;
            let bar = if options.scrollbars {
                state.bars.band_at(&layout, event.position(), touch)
            } else {
                None
            };
            let allowed = match event.pointer_type() {
                PointerType::Touch => options.touch,
                PointerType::Mouse => {
                    event.button_state().contains(ButtonState::PRIMARY)
                        && (options.mouse_drag || bar.is_some())
                }
            };
            if !allowed {
                token.dismiss();
                return;
            }
            state.velocity = Vector::ZERO;
            let claimed = bar.is_some() && token.claim();
            if bar.is_some() && !claimed {
                token.dismiss();
                return;
            }
            if let Some(axis) = bar
                && let Some((track, thumb)) = layout.grab_geometry(axis)
            {
                state.bars.dragging = Some(axis);
                // Clicking the track outside the thumb jumps there.
                let along = event.position()[axis];
                if along < thumb.min()[axis] || along > thumb.max()[axis] {
                    let mut offset = state.offset.get();
                    let fraction =
                        (event.position()[axis] - track.min()[axis] - thumb.size()[axis] / 2.)
                            / (track.size()[axis] - thumb.size()[axis]).max(1.);
                    offset[axis] = fraction * state.max[axis];
                    state.set_offset(offset);
                }
            }
            state.drag = Some(Drag {
                id: event.pointer_id(),
                origin: event.position(),
                previous: event.position(),
                time: event.time(),
                claimed,
                axis: options.axis,
                bar,
            });
            return;
        } else {
            token.dismiss();
            return;
        }

        let mut drag = state.drag.take().unwrap();
        match event.action() {
            PointerAction::Move => {
                let total = event.position() - drag.origin;
                if !drag.claimed {
                    if total.length() <= options.drag_threshold {
                        state.drag = Some(drag);
                        return;
                    }
                    let dominant = if total.x.abs() > total.y.abs() {
                        ScrollAxis::Horizontal
                    } else {
                        ScrollAxis::Vertical
                    };
                    if options.axis != ScrollAxis::Both && options.axis != dominant {
                        token.dismiss();
                        return;
                    }
                    if options.axis_lock {
                        drag.axis = dominant;
                    }
                    let delta = -drag.axis.mask(total);
                    let current = state.offset.get();
                    let can_move = (current + delta).clamp(Vector::ZERO, state.max) != current;
                    let can_pull = state.refresh.is_some()
                        && drag.axis != ScrollAxis::Horizontal
                        && current.y <= 0.
                        && total.y > 0.;
                    if (!can_move && !can_pull && options.chain) || !token.claim() {
                        token.dismiss();
                        return;
                    }
                    drag.claimed = true;
                }
                let delta = event.position() - drag.previous;
                let dt = (event.time() - drag.time).clamp(0.001, 0.1);
                if let Some(axis) = drag.bar {
                    if let Some((track, thumb)) = state.bar_layout().grab_geometry(axis) {
                        let mut movement = Vector::ZERO;
                        movement[axis] = delta[axis] * state.max[axis]
                            / (track.size()[axis] - thumb.size()[axis]).max(1.);
                        state.scroll_by(movement);
                    }
                } else if state.refresh.is_some()
                    && drag.axis != ScrollAxis::Horizontal
                    && (state.pull.get_untracked() > 0.
                        || (state.offset.get().y <= 0. && delta.y > 0.))
                {
                    // Past the top: pull with resistance instead of scrolling.
                    let pull = (state.pull.get_untracked() + delta.y * 0.5).max(0.);
                    state.pull.set(pull);
                    state.velocity = Vector::ZERO;
                } else {
                    let moved = state.scroll_by(-drag.axis.mask(delta));
                    state.velocity = (state.velocity * 0.25 + moved / dt * 0.75)
                        .clamp_length_max(options.max_velocity);
                }
                drag.previous = event.position();
                drag.time = event.time();
                state.drag = Some(drag);
            }
            PointerAction::Up | PointerAction::Cancel => {
                state.bars.dragging = None;
                let trigger = event.action() == PointerAction::Up
                    && state.refresh.as_ref().is_some_and(|refresh| {
                        !refresh.refreshing.get_untracked()
                            && state.pull.get_untracked() >= refresh.threshold
                    });
                if trigger && let Some(refresh) = state.refresh.clone() {
                    refresh.refreshing.set(true);
                    drop(state);
                    (refresh.action.borrow_mut())();
                    token.dismiss();
                    return;
                }
                if event.action() == PointerAction::Cancel
                    || !options.momentum
                    || drag.bar.is_some()
                    || event.time() - drag.time > 0.1
                {
                    state.velocity = Vector::ZERO;
                }
                token.dismiss();
            }
            PointerAction::Down => state.drag = Some(drag),
        }
    }

    fn on_rejected(&mut self, pointer: PointerId) {
        let mut state = self.state.borrow_mut();
        if state.drag.as_ref().is_some_and(|drag| drag.id == pointer) {
            state.drag = None;
            state.bars.dragging = None;
            state.velocity = Vector::ZERO;
        }
    }
}
