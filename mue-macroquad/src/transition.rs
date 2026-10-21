//! Style transitions for reactive values.
//!
//! A style transition interpolates matching property values; call [`advance`] once per frame.

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
    value: Signal<T>,
    _running: Signal<bool>,
}

impl<T: Interpolatable> Driver for RefCell<Tween<T>> {
    fn advance(&self, dt: f32) {
        let mut tween = self.borrow_mut();
        if tween.value.is_disposed() {
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
        let presentation = tween.value;
        drop(tween);
        presentation.set(value);
    }
}

fn transition_value<T: Interpolatable>(
    source: ReadSignal<T>,
    curve: mue_core::prop::Prop<Transition>,
) -> ReadSignal<T> {
    let initial = source.get_clone_untracked();
    let value = signal(initial.clone());
    let running = signal(false);
    let tween = Rc::new(RefCell::new(Tween {
        from: initial.clone(),
        to: initial,
        elapsed: 0.,
        transition: None,
        value,
        _running: running,
    }));
    let driver: Rc<dyn Driver> = tween.clone();
    register_driver(&driver);
    watch(source, move |source| {
        let destination = source.get_clone_untracked();
        let curve = curve.get_clone_untracked();
        let curve = (curve.duration().is_finite() && curve.duration() > 0.).then_some(curve);
        let mut tween = tween.borrow_mut();
        if tween.to == destination {
            return;
        }
        tween.from = value.get_clone_untracked();
        tween.to = destination.clone();
        tween.elapsed = 0.;
        tween.transition = curve
            .filter(|_| tween.from != destination && T::can_interpolate(&tween.from, &destination));
        let is_running = tween.transition.is_some();
        drop(tween);
        if !is_running {
            value.set(destination);
        }
    });
    *value
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

fn bind<T: Interpolatable>(value: &mut Option<Prop<T>>, transition: Prop<Transition>) {
    if let Some(Prop::Dynamic(source)) = value {
        *value = Some(transition_value(*source, transition).into());
    }
}

/// Called after factory/theme resolution, inside the node's lifetime scope.
pub(crate) fn bind_style(style: &mut Style) {
    let transition = style.take_transition();
    macro_rules! bind_fields { ($($field:ident),*) => { $(bind(&mut style.$field, transition.clone());)* }; }
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
        line_height_multiplier,
        transform_list,
        transform_origin,
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
    if let Some(Prop::Dynamic(source)) = &style.matrix_transform {
        let columns = source.map(|m| m.to_cols_array());
        style.matrix_transform = Some(
            transition_value(columns, transition)
                .map(|m| Matrix::from_cols_array(&m))
                .into(),
        );
    }
}
