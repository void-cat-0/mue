//! Unit-bearing declarations and standard property conversions.
//!
//! ```
//! use mue_core::{FontSize, Size, prop::Prop, signal::{signal, Access}};
//! let size: Size = 12_f32.into();
//! let font: FontSize = 18_f32.into();
//! let pixels: Prop<Size> = 12_f32.into();
//! let source = signal(18_f32);
//! let reactive_font: Prop<FontSize> = source.into();
//! source.set(24.);
//! assert_eq!(reactive_font.get(), FontSize::Px(24.));
//! ```

use crate::{
    prop::{Prop, PropValue},
    signal::{ReadSignal, Signal},
};

#[cfg(feature = "impl-taffy")]
use taffy::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto};

/// A length declaration; percentages use 50 for 50%, not 0.5.
/// Resolution and valid variants depend on the consuming property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Px(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
    Auto,
}

impl Size {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }
    pub const fn vw(value: f32) -> Self {
        Self::Vw(value)
    }
    pub const fn vh(value: f32) -> Self {
        Self::Vh(value)
    }
    pub const fn auto() -> Self {
        Self::Auto
    }
}
impl Default for Size {
    fn default() -> Self {
        Self::Px(0.)
    }
}
impl PropValue for Size {}

/// A font-size declaration, independent of layout dimensions.
/// Percent/em are relative to the parent font, rem to the root font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontSize {
    Px(f32),
    Percent(f32),
    Em(f32),
    Rem(f32),
    Vw(f32),
    Vh(f32),
}
impl FontSize {
    pub const DEFAULT_PX: f32 = 32.;
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }
    pub const fn em(value: f32) -> Self {
        Self::Em(value)
    }
    pub const fn rem(value: f32) -> Self {
        Self::Rem(value)
    }
    pub const fn vw(value: f32) -> Self {
        Self::Vw(value)
    }
    pub const fn vh(value: f32) -> Self {
        Self::Vh(value)
    }
}
impl Default for FontSize {
    fn default() -> Self {
        Self::Px(Self::DEFAULT_PX)
    }
}
impl PropValue for FontSize {}

impl From<f32> for Prop<Size> {
    fn from(value: f32) -> Self {
        Prop::Static(Size::Px(value))
    }
}
impl From<f32> for Prop<FontSize> {
    fn from(value: f32) -> Self {
        Prop::Static(FontSize::Px(value))
    }
}
impl From<Prop<f32>> for Prop<Size> {
    fn from(value: Prop<f32>) -> Self {
        value.map(Size::Px)
    }
}
impl From<ReadSignal<f32>> for Prop<Size> {
    fn from(value: ReadSignal<f32>) -> Self {
        value.map(Size::Px).into()
    }
}
impl From<Signal<f32>> for Prop<Size> {
    fn from(value: Signal<f32>) -> Self {
        (*value).map(Size::Px).into()
    }
}
impl From<Prop<f32>> for Prop<FontSize> {
    fn from(value: Prop<f32>) -> Self {
        value.map(FontSize::Px)
    }
}
impl From<ReadSignal<f32>> for Prop<FontSize> {
    fn from(value: ReadSignal<f32>) -> Self {
        value.map(FontSize::Px).into()
    }
}
impl From<Signal<f32>> for Prop<FontSize> {
    fn from(value: Signal<f32>) -> Self {
        (*value).map(FontSize::Px).into()
    }
}

impl From<f32> for Size {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}
impl From<f32> for FontSize {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}

#[cfg(feature = "impl-taffy")]
fn size_from_raw(raw: CompactLength, source: &str) -> Size {
    assert!(
        raw.tag() & 0b111 != 0,
        "{source}: calc() is not supported by Size"
    );
    let finite = |v: f32| {
        assert!(v.is_finite(), "{source} length must be finite, got {v:?}");
        v
    };
    match raw.tag() {
        CompactLength::LENGTH_TAG => Size::Px(finite(raw.value())),
        CompactLength::PERCENT_TAG => {
            Size::Percent(finite((f64::from(finite(raw.value())) * 100.) as f32))
        }
        CompactLength::AUTO_TAG => Size::Auto,
        _ => panic!("{source}: unsupported Taffy length variant"),
    }
}

#[cfg(feature = "impl-taffy")]
macro_rules! taffy_inputs {
    ($($ty:ty),*) => { $(
        impl From<$ty> for Size {
            fn from(value: $ty) -> Self { size_from_raw(value.into_raw(), stringify!($ty)) }
        }
        impl From<$ty> for Prop<Size> {
            fn from(value: $ty) -> Self { Self::Static(Size::from(value)) }
        }
        impl From<Prop<$ty>> for Prop<Size> {
            fn from(value: Prop<$ty>) -> Self { value.map(Size::from) }
        }
        impl From<ReadSignal<$ty>> for Prop<Size> {
            fn from(value: ReadSignal<$ty>) -> Self { value.map(Size::from).into() }
        }
        impl From<Signal<$ty>> for Prop<Size> {
            fn from(value: Signal<$ty>) -> Self { (*value).map(Size::from).into() }
        }
    )* };
}
#[cfg(feature = "impl-taffy")]
taffy_inputs!(Dimension, LengthPercentage, LengthPercentageAuto);
