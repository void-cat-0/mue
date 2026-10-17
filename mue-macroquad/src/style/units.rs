//! Pure length declarations and property-aware lowering.
//!
//! `Size` and `FontSize` are values; `Prop`/signals wrap them. Constructors do
//! not read the viewport or create signals. Use `signal(Size::Px(...))` when a
//! component needs to change both the value and its unit.
//!
//! ```compile_fail
//! use mue_macroquad::style::units::{FontSize, Size};
//! use mue_core::prop::Prop;
//! let _: Prop<FontSize> = Size::Px(18.0).into();
//! ```
//!
//! ```compile_fail
//! use mue_macroquad::style::units::{Size, px};
//! use mue_core::prop::Prop;
//! let _: Prop<f32> = px(0.5).into();
//! ```
//!

use mue_core::{effect::computed, prop::Prop, signal::Access};
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

use crate::viewport;

pub use mue_core::{FontSize, Size};

/// Construct a logical-pixel declaration.
pub const fn px(value: f32) -> Size {
    Size::Px(value)
}
/// Construct a containing-block percentage; `percent(50.0)` means 50%.
pub const fn percent(value: f32) -> Size {
    Size::Percent(value)
}
/// Construct a percentage of the logical viewport width.
pub const fn vw(value: f32) -> Size {
    Size::Vw(value)
}
/// Construct a percentage of the logical viewport height.
pub const fn vh(value: f32) -> Size {
    Size::Vh(value)
}

fn finite(value: f32, property: &str) -> f32 {
    assert!(
        value.is_finite(),
        "{property} length must be finite, got {value:?}"
    );
    value
}

fn non_negative(value: f32, property: &str) -> f32 {
    let value = finite(value, property);
    assert!(
        value >= 0.0,
        "{property} must be non-negative, got {value:?}"
    );
    value
}

fn relative(value: f32, base: f32, divisor: f64, property: &str) -> f32 {
    let value = finite(value, property);
    let base = non_negative(base, property);
    finite(
        (f64::from(value) * f64::from(base) / divisor) as f32,
        property,
    )
}

// should be replaced when impl_restriction is stablized
mod private {
    use mue_core::{FontSize, Size};

    pub trait Sealed {}
    impl Sealed for Size {}
    impl Sealed for FontSize {}
}

/// Lower a `Size` according to the target CSS/Taffy property.
pub trait SizeExt: private::Sealed {
    fn to_dimension(self) -> Dimension;
    fn to_length_percentage(self, property: &str) -> LengthPercentage;
    fn to_length_percentage_auto(self) -> LengthPercentageAuto;
    fn resolve(self, property: &str) -> f32;
}

impl SizeExt for Size {
    fn to_dimension(self) -> Dimension {
        match self {
            Size::Percent(value) => Dimension::percent(finite(value, "dimension percent") / 100.0),
            Size::Auto => Dimension::auto(),
            _ => Dimension::length(self.resolve("dimension")),
        }
    }

    fn to_length_percentage(self, property: &str) -> LengthPercentage {
        match self {
            Size::Auto => panic!("{property} does not support Size::Auto"),
            Size::Percent(value) => LengthPercentage::percent(finite(value, property) / 100.0),
            _ => LengthPercentage::length(self.resolve(property)),
        }
    }

    fn to_length_percentage_auto(self) -> LengthPercentageAuto {
        match self {
            Size::Auto => LengthPercentageAuto::auto(),
            Size::Percent(value) => {
                LengthPercentageAuto::percent(finite(value, "length-percentage-auto") / 100.0)
            }
            _ => LengthPercentageAuto::length(self.resolve("length-percentage-auto")),
        }
    }

    fn resolve(self, property: &str) -> f32 {
        match self {
            Size::Px(value) => finite(value, property),
            Size::Vw(value) => relative(value, viewport::read().get().width, 100.0, property),
            Size::Vh(value) => relative(value, viewport::read().get().height, 100.0, property),
            Size::Percent(value) => {
                finite(value, property);
                panic!("{property} cannot resolve Size::Percent without a containing block")
            }
            Size::Auto => panic!("{property} cannot resolve Size::Auto to pixels"),
        }
    }
}

/// Resolve a font declaration against inherited and root computed sizes.
pub trait FontSizeExt: private::Sealed {
    fn resolve(self, parent: f32, root: f32) -> f32;
}

impl FontSizeExt for FontSize {
    fn resolve(self, parent: f32, root: f32) -> f32 {
        let (value, divisor) = match self {
            FontSize::Px(value) => return non_negative(value, "font-size px"),
            FontSize::Percent(value) => (non_negative(value, "font-size percent"), 100.0),
            FontSize::Em(value) => (non_negative(value, "font-size em"), 1.0),
            FontSize::Rem(value) => (non_negative(value, "font-size rem"), 1.0),
            FontSize::Vw(value) => (non_negative(value, "font-size vw"), 100.0),
            FontSize::Vh(value) => (non_negative(value, "font-size vh"), 100.0),
        };
        let base = match self {
            FontSize::Percent(_) | FontSize::Em(_) => parent,
            FontSize::Rem(_) => root,
            FontSize::Vw(_) => viewport::read().get().width,
            FontSize::Vh(_) => viewport::read().get().height,
            FontSize::Px(_) => unreachable!(),
        };
        relative(value, base, divisor, "font-size")
    }
}

/// Resolve a scalar `Size` at its consuming property boundary.
pub fn resolve_length(value: Prop<Size>, property: &'static str) -> Prop<f32> {
    match value {
        Prop::Static(Size::Vw(_) | Size::Vh(_)) | Prop::Dynamic(_) => {
            computed(move |_| value.get().resolve(property)).into()
        }
        Prop::Static(value) => Prop::Static(value.resolve(property)),
    }
}
