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

pub(crate) trait Driver {
    fn advance(&self, dt: f32);
}

pub(crate) fn register_driver(driver: &Rc<dyn Driver>) {
    DRIVERS.with_borrow_mut(|drivers| drivers.push(Rc::downgrade(driver)));
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
    register_driver(&driver);
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
