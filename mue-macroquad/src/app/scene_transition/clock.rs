use crate::{math::Rect, motion::transition::Transition};

use super::{TransitionFrame, TransitionPhase, TransitionTiming};

/// GPU-free phase clock. `frame` proposes a draw; `did_present` commits it.
/// App must acknowledge only a successfully finished overlay/composition.
/// Not calling these methods while paused leaves all time untouched.
#[derive(Debug)]
pub(crate) struct PhaseClock {
    timing: TransitionTiming,
    phase: TransitionPhase,
    elapsed: f32,
    first_present: bool,
    endpoint_presented: bool,
}

impl PhaseClock {
    pub(crate) fn new(timing: TransitionTiming) -> Self {
        Self {
            timing,
            phase: match timing {
                TransitionTiming::AfterSwitch(_) => TransitionPhase::AfterSwitch,
                TransitionTiming::AroundSwitch { .. } => TransitionPhase::BeforeSwitch,
            },
            elapsed: 0.,
            first_present: true,
            endpoint_presented: false,
        }
    }

    pub(crate) fn phase(&self) -> TransitionPhase {
        self.phase
    }

    /// Repeated proposals before acknowledgement do not cumulatively advance.
    /// Invalid/nonpositive durations are instantaneous, just like `Instant`.
    #[must_use]
    pub(crate) fn frame(&self, dt: f32, viewport: Rect) -> TransitionFrame {
        let curve = self.curve();
        let duration = curve.duration();
        let instant = !duration.is_finite() || duration <= 0.;
        let dt = if !instant
            && !self.first_present
            && !self.endpoint_presented
            && dt.is_finite()
            && dt > 0.
        {
            dt
        } else {
            0.
        };
        let elapsed = if instant {
            0.
        } else {
            (self.elapsed + dt).min(duration)
        };
        let linear_progress = if instant {
            1.
        } else {
            (elapsed / duration).clamp(0., 1.)
        };
        TransitionFrame {
            phase: self.phase,
            linear_progress,
            progress: curve.sample(linear_progress),
            dt,
            viewport,
            elapsed,
        }
    }

    pub(crate) fn did_present(&mut self, frame: TransitionFrame) {
        debug_assert_eq!(frame.phase, self.phase);
        self.elapsed = frame.elapsed;
        self.first_present = false;
        self.endpoint_presented = frame.linear_progress >= 1.;
    }

    pub(crate) fn ready_to_switch(&self) -> bool {
        self.phase == TransitionPhase::BeforeSwitch && self.endpoint_presented
    }

    /// Call only after the exit endpoint was presented and App committed the
    /// Scene switch. A terminal switch may instead just discard this clock.
    pub(crate) fn after_switch(&mut self) {
        debug_assert!(self.ready_to_switch());
        self.phase = TransitionPhase::AfterSwitch;
        self.elapsed = 0.;
        self.first_present = true;
        self.endpoint_presented = false;
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.phase == TransitionPhase::AfterSwitch && self.endpoint_presented
    }

    fn curve(&self) -> Transition {
        match self.timing {
            TransitionTiming::AfterSwitch(curve) => curve,
            TransitionTiming::AroundSwitch { exit, enter } => match self.phase {
                TransitionPhase::BeforeSwitch => exit,
                TransitionPhase::AfterSwitch => enter,
            },
        }
    }
}
