//! Length inputs are distinct from unitless scalar properties.
//!
//! ```
//! use mue_macroquad::style::{Style, Styleable, px, percent, vw};
//! let _ = Style::new().width(percent(50.0)).font_size(px(18.0)).radius(vw(1.0));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{Style, Styleable, px};
//! let _ = Style::new().opacity(px(0.5));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{Style, Styleable, vw};
//! let _ = Style::new().flex_grow(vw(1.0));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{Style, Styleable, vh};
//! let _ = Style::new().flex_shrink(vh(1.0));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::{node::aspect_ratio, style::px};
//! let _ = aspect_ratio().ratio(px(1.5));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{percent, px};
//! let _ = percent(px(50.0));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{vw, vh};
//! let _ = vw(vh(10.0));
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::{Style, Styleable, percent};
//! let _ = Style::new().font_size(percent(120.0));
//! ```

use mue_core::{effect::computed, prop::Prop, signal::Access};
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

use crate::viewport;

/// A logical pixel length.
///
/// Existing bare `f32` style values are already logical pixels; this wrapper is
/// provided when an explicit unit is useful at the call site.
#[derive(Clone, Copy)]
pub struct Px(Prop<f32>);

/// A percentage of the containing block, expressed as `0..=100`.
#[derive(Clone, Copy)]
pub struct Percent(Prop<f32>);

/// A percentage of the logical viewport width, expressed as `0..=100`.
#[derive(Clone, Copy)]
pub struct Vw(Prop<f32>);

/// A percentage of the logical viewport height, expressed as `0..=100`.
#[derive(Clone, Copy)]
pub struct Vh(Prop<f32>);

/// Construct an explicit logical pixel length.
pub fn px(value: impl Into<Prop<f32>>) -> Px {
    Px(value.into())
}

/// Construct a containing-block percentage.
///
/// `percent(50.0)` represents 50%, while Taffy's internal representation is
/// the normalized value `0.5`.
pub fn percent(value: impl Into<Prop<f32>>) -> Percent {
    Percent(value.into())
}

/// Construct a width-relative viewport length.
///
/// `vw(50.0)` represents half of the current logical viewport width.
pub fn vw(value: impl Into<Prop<f32>>) -> Vw {
    let value = value.into();
    let viewport = viewport::read();
    Vw(computed(move |_| value.get() * viewport.get().width / 100.0).into())
}

/// Construct a height-relative viewport length.
///
/// `vh(50.0)` represents half of the current logical viewport height.
pub fn vh(value: impl Into<Prop<f32>>) -> Vh {
    let value = value.into();
    let viewport = viewport::read();
    Vh(computed(move |_| value.get() * viewport.get().height / 100.0).into())
}

/// An input for a Taffy length property, preserving percentages and `auto`
/// where the target type supports them.
pub trait IntoLength<T> {
    fn into_length(self) -> Prop<T>;
}

macro_rules! impl_layout_length {
    ($($ty:ty),* $(,)?) => { $(
        impl<T: Into<Prop<$ty>>> IntoLength<$ty> for T {
            fn into_length(self) -> Prop<$ty> {
                self.into()
            }
        }
    )* };
}

impl_layout_length!(Dimension, LengthPercentage, LengthPercentageAuto);

/// An input for a length resolved to logical pixels, such as a font size or
/// corner radius. Containing-block percentages are not supported.
pub trait IntoLengthValue {
    fn into_length_value(self) -> Prop<f32>;
}

impl<T: Into<Prop<f32>>> IntoLengthValue for T {
    fn into_length_value(self) -> Prop<f32> {
        self.into()
    }
}

macro_rules! impl_unit_length_value {
    ($($ty:ty),* $(,)?) => { $(
        impl IntoLengthValue for $ty {
            fn into_length_value(self) -> Prop<f32> {
                self.0
            }
        }
    )* };
}

impl_unit_length_value!(Px, Vw, Vh);

macro_rules! impl_pixel_unit {
    ($unit:ty) => {
        impl From<$unit> for Prop<Dimension> {
            fn from(value: $unit) -> Self {
                value.0.into()
            }
        }

        impl From<$unit> for Prop<LengthPercentage> {
            fn from(value: $unit) -> Self {
                value.0.into()
            }
        }

        impl From<$unit> for Prop<LengthPercentageAuto> {
            fn from(value: $unit) -> Self {
                value.0.into()
            }
        }
    };
}

impl_pixel_unit!(Px);
impl_pixel_unit!(Vw);
impl_pixel_unit!(Vh);

macro_rules! impl_percent_unit {
    ($ty:ty) => {
        impl From<Percent> for Prop<$ty> {
            fn from(value: Percent) -> Self {
                value.0.map(|value| <$ty>::percent(value / 100.0))
            }
        }
    };
}

impl_percent_unit!(Dimension);
impl_percent_unit!(LengthPercentage);
impl_percent_unit!(LengthPercentageAuto);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::Viewport;
    use mue_core::{prop::Prop, signal::signal};

    fn raw<T>(value: Prop<T>) -> T
    where
        T: Clone + 'static,
    {
        value.get_clone_untracked()
    }

    #[test]
    fn explicit_pixels_keep_logical_pixel_semantics() {
        let dimension: Prop<Dimension> = px(12.0).into();
        assert_eq!(raw(dimension).into_raw().value(), 12.0);

        let scalar = px(12.0).into_length_value();
        assert_eq!(raw(scalar), 12.0);
    }

    #[test]
    fn percentages_use_css_style_input_and_taffy_normalization() {
        let dimension: Prop<Dimension> = percent(50.0).into();
        let raw = raw(dimension).into_raw();
        assert_eq!(raw.tag(), taffy::CompactLength::PERCENT_TAG);
        assert_eq!(raw.value(), 0.5);
    }

    #[test]
    fn reactive_values_are_preserved() {
        let value = signal(25.0);
        let percent_value: Prop<LengthPercentageAuto> = percent(*value).into();
        assert_eq!(raw(percent_value).into_raw().value(), 0.25);

        value.set(75.0);
        assert_eq!(raw(percent_value).into_raw().value(), 0.75);
    }

    #[test]
    fn viewport_units_follow_the_logical_viewport() {
        viewport::set(Viewport {
            width: 800.0,
            height: 600.0,
        });

        let percentage = signal(50.0);
        let width = vw(percentage).into_length_value();
        let height = vh(25.0).into_length_value();
        let layout_width: Prop<Dimension> = vw(percentage).into_length();
        assert_eq!(raw(width), 400.0);
        assert_eq!(raw(height), 150.0);
        assert_eq!(raw(layout_width).into_raw().value(), 400.0);

        viewport::set(Viewport {
            width: 1000.0,
            height: 800.0,
        });
        assert_eq!(raw(width), 500.0);
        assert_eq!(raw(height), 200.0);
        assert_eq!(raw(layout_width).into_raw().value(), 500.0);

        percentage.set(25.0);
        assert_eq!(raw(width), 250.0);
        assert_eq!(raw(layout_width).into_raw().value(), 250.0);
    }
}
