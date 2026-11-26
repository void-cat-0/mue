//! Style transitions for reactive values.
//!
//! Reactive property transitions and their time curves.
//!
//! A style transition interpolates compatible property values when their
//! reactive source changes. The configured curve is [`Transition`], exposed as
//! the transition style property through [`crate::style::Style`]. [`advance`]
//! moves active property transitions forward; [`crate::App`] calls it once during
//! each normal frame before layout. It does not advance in
//! [`crate::App::try_frame_paused`].
//!
//! Only values with compatible representations interpolate. For example, a size
//! change from pixels to percent is discrete because resolution requires layout
//! context. Nonpositive or non-finite durations also resolve immediately. Scene
//! effects use the same [`Transition`] curve type but are scheduled and presented
//! independently by [`crate::SceneTransition`].

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use mue_core::{
    effect::computed,
    runtime::batch,
    signal::{Access, ReadSignal, Signal, signal},
};

/// A time-based curve. Durations are in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transition {
    Instant,
    Linear {
        duration: f32,
    },
    Ease {
        duration: f32,
        strength: f32,
    },
    EaseIn {
        duration: f32,
        strength: f32,
    },
    EaseOut {
        duration: f32,
        strength: f32,
    },
    EaseInOut {
        duration: f32,
        strength: f32,
    },
    /// A damped spring. `bounce` is clamped to 0..=1.
    Spring {
        duration: f32,
        bounce: f32,
    },
}
impl mue_core::prop::PropValue for Transition {}

impl Transition {
    /// Interpolate at a constant rate for `duration` seconds.
    pub const fn linear(duration: f32) -> Self {
        Self::Linear { duration }
    }
    /// Use the style `ease` curve for `duration` seconds.
    pub const fn ease(duration: f32) -> Self {
        Self::Ease {
            duration,
            strength: 1.,
        }
    }
    /// Use the style `ease` curve with a blend strength from 0 to 1.
    pub const fn ease_with_strength(duration: f32, strength: f32) -> Self {
        Self::Ease { duration, strength }
    }
    /// Ease in for `duration` seconds.
    pub const fn ease_in(duration: f32) -> Self {
        Self::EaseIn {
            duration,
            strength: 1.,
        }
    }
    /// Ease in with a blend strength from 0 to 1.
    pub const fn ease_in_with_strength(duration: f32, strength: f32) -> Self {
        Self::EaseIn { duration, strength }
    }
    /// Ease out for `duration` seconds.
    pub const fn ease_out(duration: f32) -> Self {
        Self::EaseOut {
            duration,
            strength: 1.,
        }
    }
    /// Ease out with a blend strength from 0 to 1.
    pub const fn ease_out_with_strength(duration: f32, strength: f32) -> Self {
        Self::EaseOut { duration, strength }
    }
    /// Ease smoothly at both ends for `duration` seconds.
    pub const fn ease_in_out(duration: f32) -> Self {
        Self::EaseInOut {
            duration,
            strength: 1.,
        }
    }
    /// Ease at both ends with a blend strength from 0 to 1.
    pub const fn ease_in_out_with_strength(duration: f32, strength: f32) -> Self {
        Self::EaseInOut { duration, strength }
    }
    /// Use a damped spring that settles after `duration` seconds.
    pub const fn spring(duration: f32, bounce: f32) -> Self {
        Self::Spring { duration, bounce }
    }

    pub fn duration(self) -> f32 {
        match self {
            Self::Instant => 0.,
            Self::Linear { duration }
            | Self::Ease { duration, .. }
            | Self::EaseIn { duration, .. }
            | Self::EaseOut { duration, .. }
            | Self::EaseInOut { duration, .. }
            | Self::Spring { duration, .. } => duration,
        }
    }

    pub fn sample(self, progress: f32) -> f32 {
        let t = progress.clamp(0., 1.);
        if t >= 1. {
            return 1.;
        }
        match self {
            Self::Instant => 1.,
            Self::Linear { .. } => t,
            Self::Ease { strength, .. } => blend_curve(t, t * t * (3. - 2. * t), strength),
            Self::EaseIn { strength, .. } => blend_curve(t, t * t, strength),
            Self::EaseOut { strength, .. } => blend_curve(t, 1. - (1. - t) * (1. - t), strength),
            Self::EaseInOut { strength, .. } => blend_curve(t, t * t * (3. - 2. * t), strength),
            Self::Spring { bounce, .. } => {
                let bounce = if bounce.is_finite() {
                    bounce.clamp(0., 1.)
                } else {
                    0.
                };
                let damping = 1. - 0.75 * bounce;
                if damping >= 1. {
                    1. - (1. + 10. * t) * (-10. * t).exp()
                } else {
                    let ratio = (1. - damping * damping).sqrt();
                    let phase = 8. / damping * ratio * t;
                    1. - (-8. * t).exp() * (phase.cos() + damping / ratio * phase.sin())
                }
            }
        }
    }
}

fn blend_curve(linear: f32, eased: f32, strength: f32) -> f32 {
    let strength = if strength.is_finite() {
        strength.clamp(0., 1.)
    } else {
        1.
    };
    linear + (eased - linear) * strength
}

/// Values that can be presented between two state values.
pub trait Interpolatable: Clone + PartialEq + 'static {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self;

    fn can_interpolate(_from: &Self, _to: &Self) -> bool {
        true
    }
}

use crate::style::{FontSize, Size};

impl Interpolatable for Size {
    fn can_interpolate(from: &Self, to: &Self) -> bool {
        std::mem::discriminant(from) == std::mem::discriminant(to) && !matches!(to, Self::Auto)
    }

    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        match (from, to) {
            (Self::Px(a), Self::Px(b)) => Self::Px(f32::interpolate(a, b, progress)),
            (Self::Percent(a), Self::Percent(b)) => Self::Percent(f32::interpolate(a, b, progress)),
            (Self::Vw(a), Self::Vw(b)) => Self::Vw(f32::interpolate(a, b, progress)),
            (Self::Vh(a), Self::Vh(b)) => Self::Vh(f32::interpolate(a, b, progress)),
            // Unit changes, including a transition to or from `Auto`, are
            // discrete. The declaration is never implicitly converted.
            _ => *to,
        }
    }
}

impl Interpolatable for FontSize {
    fn can_interpolate(from: &Self, to: &Self) -> bool {
        std::mem::discriminant(from) == std::mem::discriminant(to)
    }

    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        match (from, to) {
            (Self::Px(a), Self::Px(b)) => Self::Px(f32::interpolate(a, b, progress)),
            (Self::Percent(a), Self::Percent(b)) => Self::Percent(f32::interpolate(a, b, progress)),
            (Self::Em(a), Self::Em(b)) => Self::Em(f32::interpolate(a, b, progress)),
            (Self::Rem(a), Self::Rem(b)) => Self::Rem(f32::interpolate(a, b, progress)),
            (Self::Vw(a), Self::Vw(b)) => Self::Vw(f32::interpolate(a, b, progress)),
            (Self::Vh(a), Self::Vh(b)) => Self::Vh(f32::interpolate(a, b, progress)),
            // Different units switch immediately; no implicit conversion is
            // possible without the layout/typography resolution context.
            _ => *to,
        }
    }
}

impl Interpolatable for f32 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        from + (to - from) * progress
    }
}

impl Interpolatable for f64 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        from + (to - from) * progress as f64
    }
}

impl<T: Interpolatable, const N: usize> Interpolatable for [T; N] {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        std::array::from_fn(|i| T::interpolate(&from[i], &to[i], progress))
    }
}

impl<T: Interpolatable> Interpolatable for Option<T> {
    fn interpolate(a: &Self, b: &Self, t: f32) -> Self {
        match (a, b) {
            (Some(a), Some(b)) => Some(T::interpolate(a, b, t)),
            _ => b.clone(),
        }
    }
}

mod lengths {
    use super::Interpolatable;
    use taffy::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto};
    macro_rules! length {
        ($($ty:ty),*) => { $(impl Interpolatable for $ty {
            fn interpolate(a: &Self, b: &Self, t: f32) -> Self {
                let from = a.into_raw();
                let to = b.into_raw();
                if from.tag() == to.tag() {
                    let value = f32::interpolate(&from.value(), &to.value(), t);
                    match from.tag() {
                        CompactLength::LENGTH_TAG => return Self::length(value),
                        CompactLength::PERCENT_TAG => return Self::percent(value),
                        _ => {}
                    }
                }
                // Auto and mixed units require layout resolution; switch discretely.
                *b
            }
        })* };
    }
    length!(Dimension, LengthPercentage, LengthPercentageAuto);
}

thread_local! {
    static DRIVERS: RefCell<Vec<Weak<dyn Driver>>> = RefCell::new(Vec::new());
}

pub trait Driver {
    fn advance(&self, dt: f32);
}

pub fn register_driver(driver: &Rc<dyn Driver>) {
    DRIVERS.with_borrow_mut(|drivers| drivers.push(Rc::downgrade(driver)));
}

struct Tween<T> {
    from: T,
    to: T,
    elapsed: f32,
    transition: Option<Transition>,
    value: T,
    frame: Signal<()>,
    registration: Option<Weak<dyn Driver>>,
}

impl<T> Drop for Tween<T> {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            let _ = DRIVERS.try_with(|drivers| {
                if let Ok(mut drivers) = drivers.try_borrow_mut() {
                    drivers.retain(|driver| !Weak::ptr_eq(driver, &registration));
                }
            });
        }
    }
}

impl<T: Interpolatable> Driver for RefCell<Tween<T>> {
    fn advance(&self, dt: f32) {
        let mut tween = self.borrow_mut();
        if tween.frame.is_disposed() {
            return;
        }
        let Some(transition) = tween.transition else {
            return;
        };
        tween.elapsed = (tween.elapsed + dt.max(0.)).min(transition.duration());
        let progress = tween.elapsed / transition.duration();
        let value = if progress >= 1. {
            tween.to.clone()
        } else {
            T::interpolate(&tween.from, &tween.to, transition.sample(progress))
        };
        if progress >= 1. {
            tween.transition = None;
        }
        tween.value = value;
        let frame = tween.frame;
        drop(tween);
        frame.force_trigger();
    }
}

fn transition_value<T: Interpolatable>(
    source: ReadSignal<T>,
    curve: mue_core::prop::Prop<Transition>,
) -> ReadSignal<T> {
    let scope = mue_core::scope::current_scope();
    let mut target = source.get_clone_untracked();
    let mut revision = source.revision_untracked();
    let mut tween: Option<Rc<RefCell<Tween<T>>>> = None;
    // Keep the source -> presentation edge visible to the scheduler, but defer
    // frame state and driver registration until a value actually animates.
    computed(move |previous: Option<&T>| {
        let destination = source.get_clone();
        let source_revision = source.revision_untracked();
        if revision != source_revision {
            revision = source_revision;
            if target != destination {
                target = destination.clone();
                let from = tween
                    .as_ref()
                    .map(|tween| tween.borrow().value.clone())
                    .or_else(|| previous.cloned())
                    .unwrap_or_else(|| destination.clone());
                let curve = curve.get_clone_untracked();
                let transition = (curve.duration().is_finite()
                    && curve.duration() > 0.
                    && from != destination
                    && T::can_interpolate(&from, &destination))
                .then_some(curve);
                if transition.is_some() && tween.is_none() {
                    // Updates may originate in a temporary caller scope. The
                    // animation belongs to the original style binding's scope.
                    let frame = scope.run(|| signal(()));
                    let state = Rc::new(RefCell::new(Tween {
                        from: from.clone(),
                        to: destination.clone(),
                        elapsed: 0.,
                        transition,
                        value: from.clone(),
                        frame,
                        registration: None,
                    }));
                    let driver: Rc<dyn Driver> = state.clone();
                    state.borrow_mut().registration = Some(Rc::downgrade(&driver));
                    register_driver(&driver);
                    tween = Some(state);
                }
                if let Some(state) = &tween {
                    let mut state = state.borrow_mut();
                    state.from = from;
                    state.to = destination.clone();
                    state.elapsed = 0.;
                    state.transition = transition;
                    if transition.is_none() {
                        state.value = destination.clone();
                    }
                }
            }
        }
        if let Some(state) = &tween {
            let state = state.borrow();
            state.frame.track();
            state.value.clone()
        } else {
            target.clone()
        }
    })
}

/// Advance active transitions by elapsed seconds. The app calls this before layout.
pub fn advance(dt: f32) {
    if !dt.is_finite() || dt <= 0. {
        return;
    }
    let drivers = DRIVERS.with_borrow_mut(std::mem::take);
    batch(|| {
        for driver in &drivers {
            if let Some(driver) = driver.upgrade() {
                driver.advance(dt);
            }
        }
    });
    DRIVERS.with_borrow_mut(|registered| {
        registered.extend(
            drivers
                .into_iter()
                .filter(|driver| driver.strong_count() > 0),
        );
    });
}

use crate::{math::Matrix, style::Style};
use mue_core::prop::Prop;

impl Interpolatable for crate::math::Rect {
    fn interpolate(a: &Self, b: &Self, t: f32) -> Self {
        Self::new(
            f32::interpolate(&a.x, &b.x, t),
            f32::interpolate(&a.y, &b.y, t),
            f32::interpolate(&a.w, &b.w, t),
            f32::interpolate(&a.h, &b.h, t),
        )
    }
}

// Bind animatable style fields to their configured transition.
macro_rules! bind_property {
    (width, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (height, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (min_width, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (min_height, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (max_width, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (max_height, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (flex_basis, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (flex_grow, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (flex_shrink, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (opacity, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (radius_top_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (radius_top_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (radius_bottom_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (radius_bottom_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (border_color, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (outline_width, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (outline_color, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (backdrop_blur, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (outline_offset, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (font_size, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (line_height, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (line_height_multiplier, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (transform_list, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (transform_origin, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (letter_spacing, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (paragraph_spacing, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (column_gap, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (row_gap, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (aspect_ratio, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (background, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (color, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (margin_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (margin_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (margin_top, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (margin_bottom, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (padding_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (padding_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (padding_top, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (padding_bottom, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (border_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (border_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (border_top, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (border_bottom, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (inset_left, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (inset_right, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (inset_top, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (inset_bottom, $value:expr, $transition:expr) => {
        crate::transition::bind_projected($value, $transition)
    };
    (matrix_transform, $value:expr, $transition:expr) => {
        crate::transition::bind_projected_matrix($value, $transition)
    };
    ($field:ident, $value:expr, $transition:expr) => {
        $value
    };
}
pub(crate) use bind_property;

/// Follow `target` with an animation: when it changes, the result moves from
/// its current value to the new one along `curve`. Animations advance once per
/// App frame. A static target or `Transition::Instant` curve returns `target`.
///
/// ```
/// use mue_core::prelude::*;
/// use mue_macroquad::transition::{Transition, animated};
///
/// # let owner = mue_core::scope::OwnedScope::new();
/// # owner.run(|| {
/// let open = signal(false);
/// let target: Prop<f32> = open.map(|open| if open { 1. } else { 0. }).into();
/// let progress = animated::<f32>(target, Transition::ease_out(0.2));
/// assert_eq!(progress.get(), 0.);
/// # });
/// ```
pub fn animated<T: Interpolatable>(
    target: impl Into<Prop<T>>,
    curve: impl Into<Prop<Transition>>,
) -> Prop<T> {
    bind_projected(target.into(), Some(curve.into()))
}

pub(crate) fn bind_projected<T: Interpolatable>(
    value: Prop<T>,
    transition: Option<Prop<Transition>>,
) -> Prop<T> {
    // Only the static variant proves that this binding can never animate.
    // A dynamic curve currently equal to Instant can change on a later update.
    if matches!(transition, Some(Prop::Static(Transition::Instant))) {
        return value;
    }
    match (value, transition) {
        (Prop::Dynamic(source), Some(transition)) => transition_value(source, transition).into(),
        (value, _) => value,
    }
}
pub(crate) fn bind_projected_matrix(
    value: Prop<Matrix>,
    transition: Option<Prop<Transition>>,
) -> Prop<Matrix> {
    // Bypass before either matrix conversion: the original signal is sufficient.
    if matches!(transition, Some(Prop::Static(Transition::Instant))) {
        return value;
    }
    match (value, transition) {
        (Prop::Dynamic(source), Some(transition)) => {
            let columns = source.map(|m| m.to_cols_array());
            transition_value(columns, transition)
                .map(|m| Matrix::from_cols_array(&m))
                .into()
        }
        (value, _) => value,
    }
}

/// Bind final style inputs once, inside the node's lifetime scope.
pub(crate) fn bind_style(style: &mut Style) {
    let transition = style
        .declared_transition()
        .unwrap_or(crate::transition::Transition::Instant.into());
    style.bind_explicit_transitions(transition);
}
