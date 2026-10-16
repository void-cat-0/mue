//! State-driven animation for reactive values.
//!
//! Wrap a state change in [`with_animation`] and read the presentation value
//! from [`animated`]. Call [`advance`] once per frame before layout/rendering.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use mue_core::{
    effect::watch,
    runtime::batch,
    signal::{Access, ReadSignal, Signal, signal},
};

/// A time-based curve. Durations are in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Animation {
    Linear {
        duration: f32,
    },
    EaseInOut {
        duration: f32,
    },
    /// A damped spring. `bounce` is clamped to 0..=1.
    Spring {
        duration: f32,
        bounce: f32,
    },
}

impl Animation {
    /// Interpolate at a constant rate for `duration` seconds.
    pub const fn linear(duration: f32) -> Self {
        Self::Linear { duration }
    }
    /// Ease smoothly at both ends for `duration` seconds.
    pub const fn ease_in_out(duration: f32) -> Self {
        Self::EaseInOut { duration }
    }
    /// Use a damped spring that settles after `duration` seconds.
    pub const fn spring(duration: f32, bounce: f32) -> Self {
        Self::Spring { duration, bounce }
    }

    pub(crate) fn duration(self) -> f32 {
        match self {
            Self::Linear { duration }
            | Self::EaseInOut { duration }
            | Self::Spring { duration, .. } => duration,
        }
    }

    pub(crate) fn sample(self, progress: f32) -> f32 {
        let t = progress.clamp(0., 1.);
        if t >= 1. {
            return 1.;
        }
        match self {
            Self::Linear { .. } => t,
            Self::EaseInOut { .. } => t * t * (3. - 2. * t),
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

/// Values that can be presented between two state values.
pub trait Animatable: Clone + PartialEq + 'static {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self;

    fn can_interpolate(_from: &Self, _to: &Self) -> bool {
        true
    }
}

use crate::style::{FontSize, Size};

impl Animatable for Size {
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

impl Animatable for FontSize {
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

impl Animatable for f32 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        from + (to - from) * progress
    }
}

impl Animatable for f64 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        from + (to - from) * progress as f64
    }
}

impl<T: Animatable, const N: usize> Animatable for [T; N] {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        std::array::from_fn(|i| T::interpolate(&from[i], &to[i], progress))
    }
}

impl<T: Animatable> Animatable for Option<T> {
    fn interpolate(a: &Self, b: &Self, t: f32) -> Self {
        match (a, b) {
            (Some(a), Some(b)) => Some(T::interpolate(a, b, t)),
            _ => b.clone(),
        }
    }
}

mod lengths {
    use super::Animatable;
    use taffy::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto};
    macro_rules! length {
        ($($ty:ty),*) => { $(impl Animatable for $ty {
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

/// Animate state changes made by `f`; `None` makes them immediate.
/// The curve follows updates through computed signals and batches.
pub fn with_animation<R>(animation: impl Into<Option<Animation>>, f: impl FnOnce() -> R) -> R {
    mue_core::update_context::with_update_context(animation.into(), f)
}

fn transaction() -> Option<Option<Animation>> {
    mue_core::update_context::UpdateContext::current()
        .get::<Option<Animation>>()
        .copied()
}

trait Driver {
    fn advance(&self, dt: f32);
}

struct Tween<T> {
    from: T,
    to: T,
    elapsed: f32,
    animation: Option<Animation>,
    value: Signal<T>,
    running: Signal<bool>,
}

impl<T: Animatable> Driver for RefCell<Tween<T>> {
    fn advance(&self, dt: f32) {
        let mut tween = self.borrow_mut();
        if tween.value.is_disposed() {
            return;
        }
        let Some(animation) = tween.animation else {
            return;
        };
        tween.elapsed = (tween.elapsed + dt.max(0.)).min(animation.duration());
        let progress = tween.elapsed / animation.duration();
        let value = if progress >= 1. {
            tween.to.clone()
        } else {
            T::interpolate(&tween.from, &tween.to, animation.sample(progress))
        };
        if progress >= 1. {
            tween.animation = None;
        }
        let presentation = tween.value;
        let running = tween.running;
        drop(tween);
        presentation.set(value);
        if progress >= 1. {
            running.set(false);
        }
    }
}

/// A live presentation value for a source signal. Changing the source inside
/// `with_animation` animates this value; changing it normally updates immediately.
#[derive(Clone, Copy)]
pub struct Animated<T> {
    /// The current interpolated presentation value.
    pub value: ReadSignal<T>,
    /// True while this value is moving toward its target.
    pub running: ReadSignal<bool>,
}

/// Follow source changes immediately, except inside an animation transaction.
pub fn animated<T: Animatable>(source: ReadSignal<T>) -> Animated<T> {
    animated_with(source, None)
}

/// Animate every change to a value with a default curve. An explicit transaction
/// overrides this curve, including `with_animation(None, ...)`.
pub fn animated_with<T: Animatable>(
    source: ReadSignal<T>,
    animation: impl Into<Option<Animation>>,
) -> Animated<T> {
    let fallback = animation.into();
    let initial = source.get_clone_untracked();
    let value = signal(initial.clone());
    let running = signal(false);
    let tween = Rc::new(RefCell::new(Tween {
        from: initial.clone(),
        to: initial,
        elapsed: 0.,
        animation: None,
        value,
        running,
    }));
    let driver: Rc<dyn Driver> = tween.clone();
    DRIVERS.with_borrow_mut(|drivers| drivers.push(Rc::downgrade(&driver)));
    watch(source, move |source| {
        let destination = source.get_clone_untracked();
        let animation = transaction()
            .unwrap_or(fallback)
            .filter(|a| a.duration().is_finite() && a.duration() > 0.);
        let mut tween = tween.borrow_mut();
        if tween.to == destination {
            return;
        }
        tween.from = value.get_clone_untracked();
        tween.to = destination.clone();
        tween.elapsed = 0.;
        tween.animation = animation
            .filter(|_| tween.from != destination && T::can_interpolate(&tween.from, &destination));
        let is_running = tween.animation.is_some();
        drop(tween);
        if !is_running {
            value.set(destination);
        }
        running.set(is_running);
    });
    Animated {
        value: *value,
        running: *running,
    }
}

/// Advance all live animations by elapsed seconds. The application runtime calls
/// this once per frame; non-graphical users may call it directly.
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

impl Animatable for crate::math::Rect {
    fn interpolate(a: &Self, b: &Self, t: f32) -> Self {
        Self::new(
            f32::interpolate(&a.x, &b.x, t),
            f32::interpolate(&a.y, &b.y, t),
            f32::interpolate(&a.w, &b.w, t),
            f32::interpolate(&a.h, &b.h, t),
        )
    }
}

fn bind<T: Animatable>(value: &mut Option<Prop<T>>, animation: Option<Animation>) {
    if let Some(Prop::Dynamic(source)) = value {
        *value = Some(animated_with(*source, animation).value.into());
    }
}

/// Called after factory/theme resolution, inside the node's lifetime scope.
pub(crate) fn bind_style(style: &mut Style) {
    let animation = style.animation;
    macro_rules! bind_fields { ($($field:ident),*) => { $(bind(&mut style.$field, animation);)* }; }
    bind_fields!(
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        flex_basis,
        flex_grow,
        flex_shrink,
        opacity,
        radius,
        font_size,
        line_height,
        letter_spacing,
        paragraph_spacing,
        column_gap,
        row_gap,
        aspect_ratio,
        background,
        color,
        margin_left,
        margin_right,
        margin_top,
        margin_bottom,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        border_left,
        border_right,
        border_top,
        border_bottom,
        inset_left,
        inset_right,
        inset_top,
        inset_bottom
    );
    if let Some(Prop::Dynamic(source)) = style.transform {
        let columns = source.map(|m| m.to_cols_array());
        style.transform = Some(
            animated_with(columns, animation)
                .value
                .map(|m| Matrix::from_cols_array(&m))
                .into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mue_core::{Disposable, effect::computed, scope::Scope};

    #[test]
    fn transaction_survives_batch_and_computed_chain() {
        let state = signal(false);
        let target = computed(move |_| if state.get() { 100.0_f32 } else { 0. });
        let intermediate = computed(move |_| target.get() * 2.);
        let displayed = animated(intermediate);
        batch(|| with_animation(Animation::linear(1.), || state.set(true)));
        assert_eq!(displayed.value.get(), 0.);
        advance(0.25);
        assert_eq!(displayed.value.get(), 50.);
    }

    #[test]
    fn implicit_curve_can_be_disabled_and_zero_duration_snaps() {
        let state = signal(0.0_f32);
        let displayed = animated_with(*state, Animation::linear(1.));
        state.set(8.);
        advance(0.5);
        assert_eq!(displayed.value.get(), 4.);
        with_animation(None, || state.set(2.));
        assert_eq!(displayed.value.get(), 2.);
        with_animation(Animation::linear(0.), || state.set(12.));
        assert_eq!(displayed.value.get(), 12.);
        assert!(!displayed.running.get());
    }

    #[test]
    fn scoped_animation_releases_driver() {
        let scope = Scope::new();
        scope.run(|| {
            let state = signal(0.0_f32);
            let _displayed = animated(*state);
            with_animation(Animation::linear(1.), || state.set(1.));
        });
        scope.dispose();
        advance(0.5);
        assert!(DRIVERS.with_borrow(|drivers| drivers.is_empty()));
    }

    #[test]
    fn invalid_time_is_ignored_and_spring_finishes_exactly() {
        let state = signal([0.0_f32, 2.]);
        let displayed = animated(*state);
        with_animation(Animation::spring(0.8, 0.4), || state.set([10., 8.]));
        advance(f32::NAN);
        advance(-1.);
        assert_eq!(displayed.value.get(), [0., 2.]);
        advance(0.8);
        assert_eq!(displayed.value.get(), [10., 8.]);
        assert!(!displayed.running.get());
    }

    #[test]
    fn completion_uses_exact_target_even_with_large_values() {
        let state = signal(1.0e20_f32);
        let displayed = animated(*state);
        with_animation(Animation::linear(1.), || state.set(1.));
        advance(1.);
        assert_eq!(displayed.value.get(), 1.);
    }

    #[test]
    fn repeated_equal_targets_do_not_restart_a_tween() {
        let state = Signal::shallow(0.0_f32);
        let displayed = animated_with(*state, Animation::linear(1.));
        state.set(10.);
        advance(0.5);
        state.set(10.);
        advance(0.5);
        assert_eq!(displayed.value.get(), 10.);
        assert!(!displayed.running.get());
    }

    #[test]
    fn transaction_animates_and_retargets_from_presentation() {
        let source = signal(0.0_f32);
        let displayed = animated(*source);
        with_animation(Animation::linear(1.), || source.set(10.));
        advance(0.5);
        assert_eq!(displayed.value.get(), 5.);
        with_animation(Animation::linear(1.), || source.set(20.));
        advance(0.5);
        assert_eq!(displayed.value.get(), 12.5);
        advance(0.5);
        assert_eq!(displayed.value.get(), 20.);
        assert!(!displayed.running.get());
    }

    #[test]
    fn ordinary_change_snaps_and_nested_transaction_restores() {
        let source = signal(0.0_f32);
        let displayed = animated(*source);
        with_animation(Animation::linear(1.), || {
            with_animation(None, || source.set(4.));
            assert_eq!(displayed.value.get(), 4.);
            source.set(8.);
        });
        advance(0.5);
        assert_eq!(displayed.value.get(), 6.);
        source.set(3.);
        assert_eq!(displayed.value.get(), 3.);
        assert!(!displayed.running.get());
    }

    #[test]
    fn size_interpolates_only_within_the_same_unit() {
        assert_eq!(
            Size::interpolate(&Size::Px(10.), &Size::Px(30.), 0.5),
            Size::Px(20.)
        );
        assert_eq!(
            Size::interpolate(&Size::Percent(10.), &Size::Vw(30.), 0.5),
            Size::Vw(30.)
        );
        assert_eq!(
            Size::interpolate(&Size::Px(10.), &Size::Auto, 0.5),
            Size::Auto
        );
    }

    #[test]
    fn font_size_interpolates_only_within_the_same_unit() {
        assert_eq!(
            FontSize::interpolate(&FontSize::Em(1.), &FontSize::Em(3.), 0.5),
            FontSize::Em(2.)
        );
        assert_eq!(
            FontSize::interpolate(&FontSize::Px(10.), &FontSize::Rem(30.), 0.5),
            FontSize::Rem(30.)
        );
    }

    #[test]
    fn unit_changes_are_discrete_through_a_live_animation() {
        let source = signal(Size::Px(10.));
        let displayed = animated(*source);
        with_animation(Animation::linear(1.), || source.set(Size::Percent(50.)));
        assert_eq!(displayed.value.get(), Size::Percent(50.));
        assert!(!displayed.running.get());
        advance(0.5);
        assert_eq!(displayed.value.get(), Size::Percent(50.));
        advance(0.5);
        assert_eq!(displayed.value.get(), Size::Percent(50.));
        assert!(!displayed.running.get());
    }
}
