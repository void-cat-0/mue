use crate::{
    event::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId},
    gesture::Gesture,
    hook::HookFn,
};

pub struct TapGesture {
    threshold: f32,

    active: Option<PointerId>,

    pub(crate) on_tap: HookFn<()>,
}

impl Default for TapGesture {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl TapGesture {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,

            active: None,

            on_tap: HookFn::default(),
        }
    }
}

impl Gesture for TapGesture {
    fn on_event(&mut self, event: &PointerEvent, claim_token: &ClaimToken) {
        if let Some(active) = &self.active {
            if *active != event.pointer_id() {
                return;
            }

            return match event.action() {
                PointerAction::Down => {}
                PointerAction::Move => {
                    if (event.start_position() - event.position()).length() > self.threshold {
                        self.active = None;
                        claim_token.dismiss();
                    }
                }
                PointerAction::Cancel => {
                    self.active = None;
                    claim_token.dismiss();
                }
                PointerAction::Up => {
                    if (event.start_position() - event.position()).length() <= self.threshold
                        && claim_token.claim()
                    {
                        self.on_tap.invoke(&());
                    }
                    self.active = None;
                }
            };
        }

        // No active pointer, try to claim this one
        if event.action() == PointerAction::Down {
            self.active = Some(event.pointer_id());
        } else {
            claim_token.dismiss();
        }
    }

    fn on_rejected(&mut self, pointer_id: PointerId) {
        if self.active == Some(pointer_id) {
            self.active = None;
        }
    }
}
