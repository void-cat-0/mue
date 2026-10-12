use super::{Gesture, GesturePhase};
use crate::{
    event::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId},
    hook::HookFn,
    math::Vector,
};

/// Positions and distances are in screen coordinates (pixels).
#[derive(Debug, Clone, Copy)]
pub struct PanEvent {
    pub phase: GesturePhase,
    pub pointer_id: PointerId,
    pub position: Vector,
    /// Displacement from the initial press.
    pub translation: Vector,
    /// Displacement since the previous callback; Start includes the threshold distance.
    pub delta: Vector,
}

pub struct PanGesture {
    threshold: f32,
    active: Option<(PointerId, Vector, Vector, bool)>,
    pub(crate) on_pan: HookFn<PanEvent>,
}

impl Default for PanGesture {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl PanGesture {
    pub fn new(threshold: f32) -> Self {
        assert!(threshold.is_finite() && threshold >= 0.0);
        Self {
            threshold,
            active: None,
            on_pan: HookFn::default(),
        }
    }

    fn emit(&mut self, phase: GesturePhase, position: Vector) {
        if let Some((pointer_id, origin, previous, _)) = self.active.as_mut() {
            self.on_pan.invoke(&PanEvent {
                phase,
                pointer_id: *pointer_id,
                position,
                translation: position - *origin,
                delta: position - *previous,
            });
            *previous = position;
        }
    }
}

impl Gesture for PanGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        let Some((id, origin, _, started)) = self.active else {
            if event.action() == PointerAction::Down {
                self.active = Some((
                    event.pointer_id(),
                    event.position(),
                    event.position(),
                    false,
                ));
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
            PointerAction::Move => {
                if started {
                    self.emit(GesturePhase::Update, event.position());
                } else if (event.position() - origin).length() > self.threshold {
                    if token.claim() {
                        self.active.as_mut().unwrap().3 = true;
                        self.emit(GesturePhase::Start, event.position());
                    } else {
                        self.active = None;
                        token.dismiss();
                    }
                }
            }
            PointerAction::Up | PointerAction::Cancel => {
                if started {
                    self.emit(
                        if event.action() == PointerAction::Up {
                            GesturePhase::End
                        } else {
                            GesturePhase::Cancel
                        },
                        event.position(),
                    );
                }
                self.active = None;
                token.dismiss();
            }
            PointerAction::Down => {}
        }
    }

    fn on_rejected(&mut self, pointer_id: PointerId) {
        if let Some((id, _, position, started)) = self.active
            && id == pointer_id
        {
            if started {
                self.emit(GesturePhase::Cancel, position);
            }
            self.active = None;
        }
    }
}
