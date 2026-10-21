use super::{Gesture, GesturePhase};
use crate::{
    event::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId, PointerType},
    hook::HookFn,
    math::Vector,
};
use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, Copy)]
pub struct PinchEvent {
    pub phase: GesturePhase,
    /// Midpoint of the two pointers in screen pixels.
    pub center: Vector,
    /// Distance relative to the initial separation (1.0 means unchanged).
    pub scale: f32,
    /// Additive change in scale since the previous callback.
    pub delta: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct RotateEvent {
    pub phase: GesturePhase,
    pub center: Vector,
    /// Accumulated radians; positive is clockwise in screen coordinates.
    pub rotation: f32,
    /// Radians since the previous callback.
    pub delta: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TransformEvent {
    pub phase: GesturePhase,
    pub center: Vector,
    /// Midpoint displacement from the initial two-pointer center.
    pub translation: Vector,
    /// Midpoint displacement since the previous callback.
    pub translation_delta: Vector,
    pub scale: f32,
    pub scale_delta: f32,
    pub rotation: f32,
    pub rotation_delta: f32,
}

struct Contact {
    token: ClaimToken,
    position: Vector,
}

#[derive(Clone, Copy)]
struct Sample {
    phase: GesturePhase,
    center: Vector,
    translation: Vector,
    scale: f32,
    rotation: f32,
}

#[derive(Clone, Copy)]
enum Recognition {
    Pinch(f32),
    Rotate(f32),
    Transform { distance: f32, rotation: f32 },
}

#[derive(Default)]
struct TwoPointers {
    contacts: Vec<Contact>,
    distance: f32,
    angle: f32,
    center: Vector,
    rotation: f32,
    started: bool,
    last: Option<Sample>,
}

impl TwoPointers {
    fn clear(&mut self) {
        for contact in self.contacts.drain(..) {
            contact.token.dismiss();
        }
        self.distance = 0.0;
        self.center = Vector::ZERO;
        self.rotation = 0.0;
        self.started = false;
        self.last = None;
    }

    fn rejected(&mut self, id: PointerId) -> Option<Sample> {
        if !self.contacts.iter().any(|c| c.token.pointer_id() == id) {
            return None;
        }
        let sample = self.last.map(|s| Sample {
            phase: GesturePhase::Cancel,
            ..s
        });
        self.clear();
        sample
    }

    fn event(
        &mut self,
        event: &PointerEvent,
        token: &ClaimToken,
        recognition: Recognition,
    ) -> Option<Sample> {
        // A mouse (including an emulated mouse) is never a second finger.
        if event.pointer_type() != PointerType::Touch {
            token.dismiss();
            return None;
        }
        let index = self
            .contacts
            .iter()
            .position(|c| c.token.pointer_id() == event.pointer_id());
        if event.action() == PointerAction::Down {
            if index.is_some() {
                return None;
            }
            if self.contacts.len() == 2 {
                token.dismiss();
                let cancelled = self.last.map(|sample| Sample {
                    phase: GesturePhase::Cancel,
                    ..sample
                });
                self.clear();
                return cancelled;
            }
            self.contacts.push(Contact {
                token: token.clone(),
                position: event.position(),
            });
            if self.contacts.len() == 2 {
                let span = self.contacts[1].position - self.contacts[0].position;
                self.distance = span.length();
                self.angle = span.y.atan2(span.x);
                self.center = (self.contacts[0].position + self.contacts[1].position) * 0.5;
                self.rotation = 0.0;
            }
            return None;
        }
        let Some(index) = index else {
            token.dismiss();
            return None;
        };
        self.contacts[index].position = event.position();
        let ending = matches!(event.action(), PointerAction::Up | PointerAction::Cancel);
        let mut result = None;
        if self.contacts.len() == 2 {
            let span = self.contacts[1].position - self.contacts[0].position;
            let distance = span.length();
            let angle = span.y.atan2(span.x);
            // Coincident contacts have no usable scale or angle baseline.
            if self.distance <= f32::EPSILON {
                self.distance = distance;
                self.angle = angle;
            } else {
                if distance > f32::EPSILON {
                    self.rotation += (angle - self.angle + PI).rem_euclid(TAU) - PI;
                    self.angle = angle;
                }
                let current_center = (self.contacts[0].position + self.contacts[1].position) * 0.5;
                let moved = (current_center - self.center).length();
                let changed_distance = (distance - self.distance).abs();
                let changed_rotation = self.rotation.abs();
                let crossed = match recognition {
                    Recognition::Pinch(threshold) => changed_distance > threshold,
                    Recognition::Rotate(threshold) => changed_rotation > threshold,
                    Recognition::Transform { distance, rotation } => {
                        changed_distance > distance
                            || changed_rotation > rotation
                            || moved > distance
                    }
                };
                let phase = if self.started {
                    Some(if ending {
                        if event.action() == PointerAction::Up {
                            GesturePhase::End
                        } else {
                            GesturePhase::Cancel
                        }
                    } else {
                        GesturePhase::Update
                    })
                } else if !ending && crossed {
                    if ClaimToken::claim_all(&[&self.contacts[0].token, &self.contacts[1].token]) {
                        self.started = true;
                        Some(GesturePhase::Start)
                    } else {
                        self.clear();
                        return None;
                    }
                } else {
                    None
                };
                if let Some(phase) = phase {
                    let sample = Sample {
                        phase,
                        center: current_center,
                        translation: current_center - self.center,
                        scale: distance / self.distance,
                        rotation: self.rotation,
                    };
                    self.last = Some(sample);
                    result = Some(sample);
                }
            }
        }
        if ending {
            self.clear();
        }
        result
    }
}

pub struct PinchGesture {
    threshold: f32,
    pointers: TwoPointers,
    previous: f32,
    pub(crate) on_pinch: HookFn<PinchEvent>,
}

impl Default for PinchGesture {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl PinchGesture {
    /// Minimum change in separation, in pixels, before claiming both pointers.
    pub fn new(threshold: f32) -> Self {
        assert!(threshold.is_finite() && threshold >= 0.0);
        Self {
            threshold,
            pointers: TwoPointers::default(),
            previous: 1.0,
            on_pinch: HookFn::default(),
        }
    }

    fn emit(&mut self, sample: Sample) {
        if sample.phase == GesturePhase::Start {
            self.previous = 1.0;
        }
        self.on_pinch.invoke(&PinchEvent {
            phase: sample.phase,
            center: sample.center,
            scale: sample.scale,
            delta: sample.scale - self.previous,
        });
        self.previous = sample.scale;
    }
}

impl Gesture for PinchGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        if let Some(sample) = self
            .pointers
            .event(event, token, Recognition::Pinch(self.threshold))
        {
            self.emit(sample);
        }
    }
    fn on_rejected(&mut self, id: PointerId) {
        if let Some(sample) = self.pointers.rejected(id) {
            self.emit(sample);
        }
    }
}

pub struct RotateGesture {
    threshold: f32,
    pointers: TwoPointers,
    previous: f32,
    pub(crate) on_rotate: HookFn<RotateEvent>,
}

pub struct TransformGesture {
    distance_threshold: f32,
    rotation_threshold: f32,
    pointers: TwoPointers,
    previous_scale: f32,
    previous_rotation: f32,
    previous_translation: Vector,
    pub(crate) on_transform: HookFn<TransformEvent>,
}

impl Default for TransformGesture {
    fn default() -> Self {
        Self {
            distance_threshold: 10.,
            rotation_threshold: 0.1,
            pointers: TwoPointers::default(),
            previous_scale: 1.,
            previous_rotation: 0.,
            previous_translation: Vector::ZERO,
            on_transform: HookFn::default(),
        }
    }
}

impl TransformGesture {
    fn emit(&mut self, sample: Sample) {
        if sample.phase == GesturePhase::Start {
            self.previous_scale = 1.;
            self.previous_rotation = 0.;
            self.previous_translation = Vector::ZERO;
        }
        self.on_transform.invoke(&TransformEvent {
            phase: sample.phase,
            center: sample.center,
            translation: sample.translation,
            translation_delta: sample.translation - self.previous_translation,
            scale: sample.scale,
            scale_delta: sample.scale - self.previous_scale,
            rotation: sample.rotation,
            rotation_delta: sample.rotation - self.previous_rotation,
        });
        self.previous_scale = sample.scale;
        self.previous_rotation = sample.rotation;
        self.previous_translation = sample.translation;
    }
}

impl Gesture for TransformGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        if let Some(sample) = self.pointers.event(
            event,
            token,
            Recognition::Transform {
                distance: self.distance_threshold,
                rotation: self.rotation_threshold,
            },
        ) {
            self.emit(sample);
        }
    }

    fn on_rejected(&mut self, id: PointerId) {
        if let Some(sample) = self.pointers.rejected(id) {
            self.emit(sample);
        }
    }
}

impl Default for RotateGesture {
    fn default() -> Self {
        Self::new(0.1)
    }
}

impl RotateGesture {
    /// Minimum rotation in radians before claiming both pointers.
    pub fn new(threshold: f32) -> Self {
        assert!(threshold.is_finite() && threshold >= 0.0);
        Self {
            threshold,
            pointers: TwoPointers::default(),
            previous: 0.0,
            on_rotate: HookFn::default(),
        }
    }

    fn emit(&mut self, sample: Sample) {
        if sample.phase == GesturePhase::Start {
            self.previous = 0.0;
        }
        self.on_rotate.invoke(&RotateEvent {
            phase: sample.phase,
            center: sample.center,
            rotation: sample.rotation,
            delta: sample.rotation - self.previous,
        });
        self.previous = sample.rotation;
    }
}

impl Gesture for RotateGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        if let Some(sample) = self
            .pointers
            .event(event, token, Recognition::Rotate(self.threshold))
        {
            self.emit(sample);
        }
    }
    fn on_rejected(&mut self, id: PointerId) {
        if let Some(sample) = self.pointers.rejected(id) {
            self.emit(sample);
        }
    }
}
