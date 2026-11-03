//! Style transitions for reactive values.
//!
//! A style transition interpolates matching property values; call [`advance`] once per frame.

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

#[cfg(test)]
pub(crate) mod eager_reference;
#[cfg(test)]
pub(crate) mod reference;

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
    #[cfg(test)]
    if reference::enabled() {
        return reference::transition_value(source, curve);
    }
    #[cfg(test)]
    if eager_reference::enabled() {
        return eager_reference::bind(source, curve);
    }
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

// Theme projections use the same transition policy as explicit fields, on demand.
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
    (radius, $value:expr, $transition:expr) => {
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

/// Called once after seal, inside the node's lifetime scope.
pub(crate) fn bind_style(style: &mut Style) {
    #[cfg(test)]
    crate::runtime::record_transition_bind();
    let transition = style.take_transition();
    style.bind_theme_transition(transition);
    style.bind_explicit_transitions(transition);
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use mue_core::{runtime::resource_counts, scope::OwnedScope};

    #[test]
    fn static_instant_returns_source_without_presentation_resources() {
        let before = resource_counts();
        let drivers = DRIVERS.with_borrow(Vec::len);
        let owner = OwnedScope::new();
        let source = owner.run(|| signal(4.0f32));
        let matrix = owner.run(|| signal(Matrix::IDENTITY));
        let baseline = resource_counts();
        let (value, transform) = owner.run(|| {
            (
                bind_projected(
                    Prop::Dynamic(*source),
                    Some(Prop::Static(Transition::Instant)),
                ),
                bind_projected_matrix(
                    Prop::Dynamic(*matrix),
                    Some(Prop::Static(Transition::Instant)),
                ),
            )
        });
        assert!(matches!(value, Prop::Dynamic(_)));
        assert!(matches!(transform, Prop::Dynamic(_)));
        assert_eq!(resource_counts(), baseline);
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers);
        source.set(7.);
        let destination = Matrix::from_translation(crate::math::Vector::new(2., 3.));
        matrix.set(destination);
        assert_eq!(value.get(), 7.);
        assert_eq!(transform.get(), destination);
        drop(owner);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn dynamic_curve_currently_instant_can_switch_to_animation() {
        let before = resource_counts();
        let external = OwnedScope::new();
        let (source, curve) = external.run(|| (signal(1.0f32), signal(Transition::Instant)));
        let owner = OwnedScope::new();
        let value =
            owner.run(|| bind_projected(Prop::Dynamic(*source), Some(Prop::Dynamic(*curve))));
        let value = match value {
            Prop::Dynamic(value) => value,
            Prop::Static(_) => panic!("dynamic curve needs presentation binding"),
        };
        source.set(2.);
        assert_eq!(value.get(), 2.);
        curve.set(Transition::linear(1.));
        source.set(4.);
        assert_eq!(value.get(), 2.);
        advance(0.5);
        assert_eq!(value.get(), 3.);
        advance(0.5);
        assert_eq!(value.get(), 4.);
        drop(owner);
        assert!(value.is_disposed());
        assert!(!source.is_disposed());
        assert!(!curve.is_disposed());
        drop(external);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn frame_resources_are_lazy_reused_and_owned_by_the_binding_scope() {
        let baseline = resource_counts();
        let drivers = DRIVERS.with_borrow(Vec::len);
        let external = OwnedScope::new();
        let (source, curve) = external.run(|| (signal(1.0f32), signal(Transition::Instant)));
        let owner = OwnedScope::new();
        let before = resource_counts();
        let value = owner.run(|| transition_value(*source, curve.into()));
        assert_eq!(resource_counts().signals, before.signals + 1);
        assert_eq!(resource_counts().effects, before.effects + 1);
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers);
        source.set(2.);
        assert_eq!(value.get(), 2.);
        assert_eq!(resource_counts().signals, before.signals + 1);
        let temporary = OwnedScope::new();
        temporary.run(|| {
            curve.set(Transition::linear(1.));
            source.set(4.);
        });
        drop(temporary);
        assert_eq!(resource_counts().signals, before.signals + 2);
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers + 1);
        advance(0.5);
        assert_eq!(value.get(), 3.);
        curve.set(Transition::Instant);
        source.set(5.);
        assert_eq!(value.get(), 5.);
        curve.set(Transition::linear(1.));
        source.set(7.);
        assert_eq!(resource_counts().signals, before.signals + 2);
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers + 1);
        advance(1.);
        assert_eq!(value.get(), 7.);
        drop(owner);
        assert!(value.is_disposed());
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers);
        assert!(!source.is_disposed());
        drop(external);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn first_animation_obeys_batch_reads_before_frame_advance() {
        let baseline = resource_counts();
        for read_before_advance in [false, true] {
            let owner = OwnedScope::new();
            owner.run(|| {
                let source = signal(0.0f32);
                let value = transition_value(*source, Prop::Static(Transition::linear(1.)));
                batch(|| {
                    source.set(10.);
                    if read_before_advance {
                        assert_eq!(value.get(), 0.);
                    }
                    advance(0.5);
                });
                assert_eq!(value.get(), if read_before_advance { 5. } else { 0. });
                advance(0.5);
                assert_eq!(value.get(), if read_before_advance { 10. } else { 5. });
            });
            drop(owner);
            assert_eq!(resource_counts(), baseline);
        }
    }

    #[test]
    fn interrupted_transition_retargets_from_presented_value_and_keeps_curve_snapshot() {
        let before = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0.0f32);
            let curve = signal(Transition::linear(1.));
            let value = transition_value(*source, curve.into());
            source.set(10.);
            advance(0.5);
            assert_eq!(value.get(), 5.);
            source.set(20.);
            advance(0.5);
            assert_eq!(value.get(), 12.5);
            // A curve-only change does not alter the current animation.
            curve.set(Transition::Instant);
            advance(0.25);
            assert_eq!(value.get(), 16.25);
            source.set(30.);
            assert_eq!(value.get(), 30.);
            advance(1.);
            assert_eq!(value.get(), 30.);
        });
        drop(owner);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn synchronous_reads_inside_batch_and_update_metadata_remain_current() {
        let before = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0.0f32);
            let curve = signal(Transition::Instant);
            let value = transition_value(*source, curve.into());
            let observed = Rc::new(RefCell::new(Vec::new()));
            mue_core::effect::watch(value, {
                let observed = observed.clone();
                move |value| {
                    observed.borrow_mut().push((
                        value.get(),
                        mue_core::update_context::UpdateContext::current()
                            .get::<u32>()
                            .copied(),
                    ))
                }
            });
            mue_core::update_context::with_update_context(42u32, || {
                batch(|| {
                    source.set(2.);
                    assert_eq!(value.get(), 2.);
                    source.set(3.);
                    assert_eq!(value.get(), 3.);
                });
            });
            assert_eq!(&*observed.borrow(), &[(3., Some(42))]);
            source.set(4.);
            assert_eq!(observed.borrow().last(), Some(&(4., None)));
        });
        drop(owner);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn nan_destination_does_not_restart_on_every_animation_frame() {
        let before = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0.0f32);
            let value = transition_value(*source, Prop::Static(Transition::linear(1.)));
            source.set(f32::NAN);
            advance(0.5);
            assert!(value.get().is_nan());
            advance(0.5);
            assert!(value.get().is_nan());
            let notifications = Rc::new(std::cell::Cell::new(0));
            mue_core::effect::watch(value, {
                let notifications = notifications.clone();
                move |_| notifications.set(notifications.get() + 1)
            });
            advance(0.5);
            assert_eq!(notifications.get(), 0);
        });
        drop(owner);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn dropping_the_owner_removes_tween_registrations_without_another_frame() {
        let before = resource_counts();
        let drivers = DRIVERS.with_borrow(Vec::len);
        let owner = OwnedScope::new();
        let (source, value) = owner.run(|| {
            let source = signal(1.0f32);
            (
                source,
                transition_value(*source, Prop::Static(Transition::linear(1.))),
            )
        });
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers);
        source.set(2.);
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers + 1);
        assert!(!value.is_disposed());
        drop(owner);
        assert!(value.is_disposed());
        assert_eq!(DRIVERS.with_borrow(Vec::len), drivers);
        assert_eq!(resource_counts(), before);
    }
}
