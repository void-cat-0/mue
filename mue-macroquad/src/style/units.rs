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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::Viewport;
    use mue_core::{effect::computed, signal::signal};
    use std::{cell::Cell, panic::catch_unwind, rc::Rc};
    use taffy::{AlignItems, AvailableSpace, FlexDirection, Size as TaffySize, Style, TaffyTree};

    fn assert_panic<R>(f: impl FnOnce() -> R + std::panic::UnwindSafe, message: &str) {
        let error = match catch_unwind(f) {
            Ok(_) => panic!("expected a panic"),
            Err(error) => error,
        };
        let text = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic");
        assert!(text.contains(message), "expected {message:?}, got {text:?}");
    }

    fn set_viewport(width: f32, height: f32) {
        viewport::set(Viewport { width, height });
    }

    #[test]
    fn taffy_inputs_unnormalize_percentages_and_follow_signals() {
        macro_rules! check {
            ($ty:ty) => {{
                let input = signal(<$ty>::length(-8.0));
                let converted: Prop<Size> = input.into();
                let read: Prop<Size> = (*input).into();
                let prop: Prop<Size> = Prop::Dynamic(*input).into();
                assert_eq!(converted.get(), Size::Px(-8.0));
                assert_eq!(read.get(), Size::Px(-8.0));
                assert_eq!(prop.get(), Size::Px(-8.0));
                input.set(<$ty>::percent(1.5));
                assert_eq!(converted.get(), Size::Percent(150.0));
                assert_eq!(read.get(), Size::Percent(150.0));
                assert_eq!(prop.get(), Size::Percent(150.0));
                let static_value: Prop<Size> = <$ty>::percent(0.5).into();
                assert_eq!(static_value.get(), Size::Percent(50.0));
                assert_eq!(Size::from(<$ty>::percent(-0.25)), Size::Percent(-25.0));
            }};
        }
        check!(Dimension);
        check!(LengthPercentage);
        check!(LengthPercentageAuto);
        let input = signal(Dimension::auto());
        let value: Prop<Size> = input.into();
        assert_eq!(value.get(), Size::Auto);
        input.set(Dimension::percent(0.25));
        assert_eq!(value.get(), Size::Percent(25.0));
        input.set(Dimension::auto());
        assert_eq!(value.get(), Size::Auto);
    }

    #[test]
    fn unsupported_property_units_are_rejected() {
        assert_panic(
            || Size::Auto.to_length_percentage("padding_left"),
            "padding_left",
        );
        assert_panic(
            || Size::percent(25.0).resolve("radius"),
            "radius cannot resolve Size::Percent",
        );
        assert_panic(
            || Size::Auto.resolve("letter_spacing"),
            "letter_spacing cannot resolve Size::Auto",
        );
        assert_panic(
            || resolve_length(Size::percent(50.0).into(), "radius"),
            "radius",
        );
        assert_panic(
            || resolve_length(Size::Auto.into(), "letter_spacing"),
            "letter_spacing",
        );
    }

    #[test]
    fn static_viewport_lengths_track_resize_without_changing_declarations() {
        set_viewport(800.0, 600.0);
        let width = resolve_length(Size::vw(50.0).into(), "radius");
        let height = resolve_length(Size::vh(25.0).into(), "line_height");
        assert!(matches!(width, Prop::Dynamic(_)));
        assert_eq!(width.get(), 400.0);
        assert_eq!(height.get(), 150.0);
        set_viewport(1000.0, 800.0);
        assert_eq!(width.get(), 500.0);
        assert_eq!(height.get(), 200.0);
        assert_eq!(Size::vw(50.0).to_dimension().into_raw().value(), 500.0);
        assert_eq!(
            Size::vh(25.0)
                .to_length_percentage("gap")
                .into_raw()
                .value(),
            200.0
        );
        assert!(matches!(
            resolve_length(12.0.into(), "radius"),
            Prop::Static(12.0)
        ));
    }

    #[test]
    fn only_viewport_units_subscribe_to_viewport_changes() {
        set_viewport(800.0, 600.0);
        let source = signal(Size::px(12.0));
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let resolved = computed(move |_| {
            count.set(count.get() + 1);
            source.get().resolve("offset")
        });
        assert_eq!(resolved.get(), 12.0);
        set_viewport(900.0, 700.0);
        assert_eq!(resolved.get(), 12.0);
        assert_eq!(calls.get(), 1);
        source.set(Size::vw(10.0));
        assert_eq!(resolved.get(), 90.0);
        set_viewport(1000.0, 800.0);
        assert_eq!(resolved.get(), 100.0);
        source.set(Size::px(12.0));
        set_viewport(1100.0, 900.0);
        assert_eq!(resolved.get(), 12.0);
    }

    #[test]
    fn reactive_units_produce_real_taffy_layouts() {
        set_viewport(800.0, 600.0);
        let source = signal(Size::px(120.0));
        let declaration: Prop<Size> = source.into();
        let lowered = computed(move |_| Style {
            size: TaffySize {
                width: declaration.get().to_dimension(),
                height: Size::px(20.0).to_dimension(),
            },
            flex_shrink: 0.0,
            ..Style::default()
        });
        let parent = |width| Style {
            size: TaffySize {
                width: Size::px(width).to_dimension(),
                height: Size::px(300.0).to_dimension(),
            },
            flex_direction: FlexDirection::Column,
            align_items: Some(AlignItems::Start),
            ..Style::default()
        };
        let mut tree = TaffyTree::<()>::new();
        let content = tree
            .new_leaf(Style {
                size: TaffySize {
                    width: Size::px(64.0).to_dimension(),
                    height: Size::px(10.0).to_dimension(),
                },
                ..Style::default()
            })
            .unwrap();
        let child = tree
            .new_with_children(lowered.get_clone(), &[content])
            .unwrap();
        let root = tree.new_with_children(parent(600.0), &[child]).unwrap();
        let measure = |tree: &mut TaffyTree<()>| {
            tree.set_style(child, lowered.get_clone()).unwrap();
            tree.compute_layout(
                root,
                TaffySize {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            )
            .unwrap();
            tree.layout(child).unwrap().size.width
        };
        assert_eq!(measure(&mut tree), 120.0);
        source.set(Size::percent(50.0));
        assert_eq!(measure(&mut tree), 300.0);
        tree.set_style(root, parent(400.0)).unwrap();
        assert_eq!(measure(&mut tree), 200.0);
        source.set(Size::vw(25.0));
        set_viewport(1000.0, 800.0);
        assert_eq!(measure(&mut tree), 250.0);
        source.set(Size::Auto);
        assert_eq!(measure(&mut tree), 64.0);
        assert_eq!(declaration.get(), Size::Auto);
    }

    #[test]
    fn font_units_resolve_parent_root_viewport_and_switch_units() {
        assert_eq!(FontSize::px(18.0).resolve(f32::NAN, f32::NAN), 18.0);
        set_viewport(800.0, 600.0);
        let font = signal(FontSize::percent(150.0));
        let parent = signal(20.0);
        let root = signal(32.0);
        let resolved = computed(move |_| font.get().resolve(parent.get(), root.get()));
        assert_eq!(resolved.get(), 30.0);
        parent.set(24.0);
        assert_eq!(resolved.get(), 36.0);
        font.set(FontSize::rem(2.0));
        root.set(40.0);
        assert_eq!(resolved.get(), 80.0);
        font.set(FontSize::vw(5.0));
        assert_eq!(resolved.get(), 40.0);
        set_viewport(1000.0, 800.0);
        assert_eq!(resolved.get(), 50.0);
    }

    #[test]
    fn illegal_values_and_calc_are_rejected() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for size in [
                Size::px(invalid),
                Size::percent(invalid),
                Size::vw(invalid),
                Size::vh(invalid),
            ] {
                assert_panic(|| size.to_dimension(), "finite");
                assert_panic(|| size.to_length_percentage("padding"), "finite");
                assert_panic(|| size.to_length_percentage_auto(), "finite");
                assert_panic(|| size.resolve("radius"), "finite");
            }
            assert_panic(|| Size::from(Dimension::length(invalid)), "finite");
            assert_panic(|| Size::from(LengthPercentage::percent(invalid)), "finite");
        }
        for invalid in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for font in [
                FontSize::px(invalid),
                FontSize::percent(invalid),
                FontSize::em(invalid),
                FontSize::rem(invalid),
                FontSize::vw(invalid),
                FontSize::vh(invalid),
            ] {
                assert_panic(|| font.resolve(20.0, 32.0), "font-size");
            }
            assert_panic(|| FontSize::em(1.0).resolve(invalid, 32.0), "font-size");
            assert_panic(|| FontSize::rem(1.0).resolve(20.0, invalid), "font-size");
        }
        static HANDLE: u64 = 0;
        let handle = (&HANDLE as *const u64).cast::<()>();
        assert_panic(
            || Size::from(Dimension::calc(handle)),
            "calc() is not supported",
        );
        assert_panic(
            || Size::from(LengthPercentage::calc(handle)),
            "calc() is not supported",
        );
        assert_panic(
            || Size::from(LengthPercentageAuto::calc(handle)),
            "calc() is not supported",
        );
    }
}
