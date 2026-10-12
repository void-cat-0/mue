use super::Gesture;
use crate::{
    event::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId},
    hook::HookFn,
    math::Vector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Emitted on release for a sufficiently long, fast movement.
#[derive(Debug, Clone, Copy)]
pub struct SwipeEvent {
    pub pointer_id: PointerId,
    pub position: Vector,
    pub translation: Vector,
    /// Average velocity in screen pixels per second.
    pub velocity: Vector,
    pub direction: SwipeDirection,
}

pub struct SwipeGesture {
    distance: f32,
    velocity: f32,
    active: Option<(PointerId, Vector, f32)>,
    pub(crate) on_swipe: HookFn<SwipeEvent>,
}

impl Default for SwipeGesture {
    fn default() -> Self {
        Self::new(50.0, 300.0)
    }
}

impl SwipeGesture {
    /// Minimum displacement in pixels and average speed in pixels per second.
    pub fn new(distance: f32, velocity: f32) -> Self {
        assert!(distance.is_finite() && distance >= 0.0);
        assert!(velocity.is_finite() && velocity >= 0.0);
        Self {
            distance,
            velocity,
            active: None,
            on_swipe: HookFn::default(),
        }
    }
}

impl Gesture for SwipeGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        let Some((id, origin, time)) = self.active else {
            if event.action() == PointerAction::Down {
                self.active = Some((event.pointer_id(), event.position(), event.time()));
            } else {
                token.dismiss();
            }
            return;
        };
        if id != event.pointer_id() {
            token.dismiss();
            return;
        }
        match event.action() {
            PointerAction::Up => {
                let translation = event.position() - origin;
                let elapsed = event.time() - time;
                if elapsed > 0.0 && translation.length() >= self.distance {
                    let velocity = translation / elapsed;
                    if velocity.length() >= self.velocity && token.claim() {
                        let direction = if translation.x.abs() >= translation.y.abs() {
                            if translation.x >= 0.0 {
                                SwipeDirection::Right
                            } else {
                                SwipeDirection::Left
                            }
                        } else if translation.y >= 0.0 {
                            SwipeDirection::Down
                        } else {
                            SwipeDirection::Up
                        };
                        self.on_swipe.invoke(&SwipeEvent {
                            pointer_id: id,
                            position: event.position(),
                            translation,
                            velocity,
                            direction,
                        });
                    }
                }
                self.active = None;
                token.dismiss();
            }
            PointerAction::Cancel => {
                self.active = None;
                token.dismiss();
            }
            _ => {}
        }
    }

    fn on_rejected(&mut self, pointer_id: PointerId) {
        if self.active.is_some_and(|(id, _, _)| id == pointer_id) {
            self.active = None;
        }
    }
}
