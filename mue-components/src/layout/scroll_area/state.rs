//! Scroll position, momentum, pull to refresh, and load more.

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::math::{Rect, Vector};

use super::{
    ScrollOptions,
    gesture::Drag,
    scrollbar::{BarLayout, Scrollbars},
};

pub(super) type Action = Rc<RefCell<dyn FnMut()>>;

/// Pull-to-refresh configuration.
#[derive(Clone)]
pub(super) struct Refresh {
    pub(super) refreshing: Signal<bool>,
    pub(super) action: Action,
    /// Pull distance that triggers a refresh on release.
    pub(super) threshold: f32,
    /// Height the content stays pulled down while refreshing.
    pub(super) hold: f32,
}

/// Load-more configuration and its edge-trigger state.
pub(super) struct ReachEnd {
    pub(super) threshold: f32,
    pub(super) action: Action,
    pub(super) armed: bool,
    pub(super) last_max: f32,
}

pub(super) struct ScrollState {
    pub(super) offset: Signal<Vector>,
    pub(super) options: ScrollOptions,
    pub(super) max: Vector,
    pub(super) viewport: Rect,
    pub(super) velocity: Vector,
    pub(super) drag: Option<Drag>,
    pub(super) last_offset: Vector,
    /// Pull-to-refresh distance past the top.
    pub(super) pull: Signal<f32>,
    pub(super) refresh: Option<Refresh>,
    pub(super) end: Option<ReachEnd>,
    /// A programmatic scroll in progress: from, to, elapsed fraction.
    pub(super) animation: Option<(Vector, Vector, f32)>,
    pub(super) bars: Scrollbars,
}

impl ScrollState {
    pub(super) fn correct_offset(&mut self) {
        // External navigation cancels an old fling. This also runs before the
        // time step so an observer of prepared geometry can stop momentum.
        if self.offset.get() != self.last_offset {
            self.velocity = Vector::ZERO;
        }
        self.set_offset(self.offset.get());
        self.last_offset = self.offset.get();
    }

    /// Advance momentum and pull springs; returns a load-more action to run.
    pub(super) fn advance(&mut self, dt: f32) -> Option<Action> {
        self.advance_pull(dt);
        self.advance_motion(dt);
        self.check_end()
    }

    fn advance_pull(&mut self, dt: f32) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        if self.drag.is_some() {
            return;
        }
        let target = if refresh.refreshing.get_untracked() {
            refresh.hold
        } else {
            0.
        };
        let pull = self.pull.get_untracked();
        if (pull - target).abs() < 0.5 {
            if pull != target {
                self.pull.set(target);
            }
            return;
        }
        let rate = 1. - (-14. * dt.clamp(0., 0.05)).exp();
        self.pull.set(pull + (target - pull) * rate);
    }

    fn check_end(&mut self) -> Option<Action> {
        let offset = self.offset.get_untracked();
        let end = self.end.as_mut()?;
        if self.max.y > end.last_max + 0.5 {
            // New content: arm again.
            end.armed = true;
        }
        end.last_max = self.max.y;
        let near = self.max.y - offset.y <= end.threshold;
        if !near {
            end.armed = true;
            None
        } else if end.armed {
            end.armed = false;
            Some(end.action.clone())
        } else {
            None
        }
    }

    pub(super) fn scroll_to(&mut self, target: Vector, animated: bool) {
        let target = self.options.axis.mask(target).clamp(Vector::ZERO, self.max);
        self.velocity = Vector::ZERO;
        if animated {
            self.animation = Some((self.offset.get_untracked(), target, 0.));
        } else {
            self.animation = None;
            self.set_offset(target);
            self.last_offset = self.offset.get_untracked();
        }
    }

    fn advance_motion(&mut self, dt: f32) {
        if let Some((from, to, t)) = self.animation {
            if self.drag.is_some() {
                // A touch takes over from the animation.
                self.animation = None;
            } else {
                let t = (t + dt.clamp(0., 0.05) / 0.3).min(1.);
                let eased = 1. - (1. - t) * (1. - t) * (1. - t);
                self.set_offset(from + (to - from) * eased);
                self.last_offset = self.offset.get_untracked();
                self.animation = (t < 1.).then_some((from, to, t));
                return;
            }
        }
        self.correct_offset();
        if self.drag.is_none() && self.options.momentum {
            let dt = dt.clamp(0., 0.05);
            let decay = (-self.options.friction * dt).exp();
            let delta = self.velocity * (1. - decay) / self.options.friction;
            let moved = self.scroll_by(delta);
            if (moved.x - delta.x).abs() > 0.01 {
                self.velocity.x = 0.;
            }
            if (moved.y - delta.y).abs() > 0.01 {
                self.velocity.y = 0.;
            }
            self.velocity *= decay;
            if self.velocity.length() < 5. {
                self.velocity = Vector::ZERO;
            }
        }
    }

    pub(super) fn set_offset(&self, value: Vector) {
        let value = Vector::new(
            if value.x.is_finite() { value.x } else { 0. },
            if value.y.is_finite() { value.y } else { 0. },
        );
        self.offset
            .set(self.options.axis.mask(value).clamp(Vector::ZERO, self.max));
    }

    pub(super) fn scroll_by(&mut self, delta: Vector) -> Vector {
        let old = self.offset.get();
        self.set_offset(old + delta);
        self.last_offset = self.offset.get();
        self.last_offset - old
    }

    /// Where the scrollbars are this frame.
    pub(super) fn bar_layout(&self) -> BarLayout {
        BarLayout {
            viewport: self.viewport,
            max: self.max,
            offset: self.offset.get_untracked(),
        }
    }
}
