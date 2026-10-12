mod long_press;
mod pan;
mod swipe;
mod tap;
mod transform;

pub use long_press::LongPressGesture;
pub use pan::{PanEvent, PanGesture};
pub use swipe::{SwipeDirection, SwipeEvent, SwipeGesture};
pub use tap::TapGesture;
pub use transform::{PinchEvent, PinchGesture, RotateEvent, RotateGesture};

/// Lifecycle of a continuous gesture. End and Cancel are emitted only after Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GesturePhase {
    Start,
    Update,
    End,
    Cancel,
}

use slotmap::new_key_type;

use crate::event::pointer::{ClaimToken, PointerEvent, PointerId};

new_key_type! {
    pub(crate) struct GestureId;
}

pub trait Gesture {
    fn on_event(&mut self, event: &PointerEvent, claim_token: &ClaimToken);

    fn on_rejected(&mut self, _pointer_id: PointerId) {}
}
