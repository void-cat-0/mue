use mue_core::prelude::*;
use mue_macroquad::{
    node::*,
    style::{IntoLength, IntoLengthValue, Style, Styleable, StyleableExt, percent, px, vh, vw},
};
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

#[mue_macros::node]
fn length_node(
    style: &mut Style,
    #[length] required: f32,
    #[length]
    #[default(8.0)]
    optional: f32,
    #[default(1.0)] weight: f32,
) {
    let _ = (style, required, optional, weight);
}

#[mue_macros::component]
fn length_component(
    #[length] required: f32,
    #[length]
    #[default(8.0)]
    optional: f32,
) -> impl Component {
    flexbox().width(required).column_gap(optional)
}

#[test]
fn generated_node_length_parameters_resolve_units_without_relaxing_scalar_parameters() {
    let value = signal(12.0);
    let builder = length_node(px(value)).optional(px(16.0)).weight(*value);
    assert_eq!(builder.required.get(), 12.0);
    assert_eq!(builder.optional.unwrap().get(), 16.0);
    assert_eq!(builder.weight.unwrap().get(), 12.0);
    value.set(24.0);
    assert_eq!(builder.required.get(), 24.0);
    assert_eq!(builder.weight.unwrap().get(), 24.0);
    let _ = length_node(12.0).required(vw(10.0)).optional(vh(5.0));
}

#[test]
fn generated_component_length_parameters_support_constructors_and_setters() {
    let builder = length_component(px(12.0)).optional(px(16.0));
    assert_eq!(builder.required.get(), 12.0);
    assert_eq!(builder.optional.unwrap().get(), 16.0);
    let _ = length_component(vw(10.0)).required(vh(5.0));
}

#[test]
fn existing_length_nodes_accept_units() {
    let _ = float().offset_x(px(8.0)).offset_y(vh(1.0));
    let _ = separator().thickness(vw(0.25));
    let _ = aspect_ratio().ratio(1.5);
}

#[test]
fn layout_traits_accept_bare_values_and_taffy_properties() {
    let scalar = signal(12.0);
    let dimension = signal(Dimension::auto());
    let _ = Style::new()
        .width(12.0)
        .width(scalar)
        .width(*scalar)
        .width(Prop::Static(12.0))
        .width(Dimension::auto())
        .width(dimension)
        .width(*dimension)
        .width(Prop::Static(Dimension::percent(0.5)))
        .gap(scalar)
        .padding((scalar,))
        .padding((px(1.0), percent(2.0)))
        .padding((px(1.0), vw(2.0), vh(3.0), percent(4.0)))
        .margin((LengthPercentageAuto::auto(),))
        .border((LengthPercentage::length(1.0),))
        .font_size(scalar)
        .line_height(*scalar)
        .radius(Prop::Static(4.0))
        .opacity(*scalar)
        .flex_grow(1.0)
        .flex_shrink(0.0);
}

struct CustomLength(f32);

impl IntoLengthValue for CustomLength {
    fn into_length_value(self) -> Prop<f32> {
        Prop::Static(self.0)
    }
}

impl IntoLength<Dimension> for CustomLength {
    fn into_length(self) -> Prop<Dimension> {
        Prop::Static(Dimension::length(self.0))
    }
}

impl IntoLength<LengthPercentage> for CustomLength {
    fn into_length(self) -> Prop<LengthPercentage> {
        Prop::Static(LengthPercentage::length(self.0))
    }
}

fn legacy_width(value: impl Into<Prop<Dimension>>) -> Style {
    Style::new().width(value)
}

fn legacy_font_size(value: impl Into<Prop<f32>>) -> Style {
    Style::new().font_size(value)
}

#[test]
fn legacy_generic_conversion_bounds_remain_compatible() {
    let _ = legacy_width(Dimension::auto());
    let _ = legacy_font_size(18.0);
}

#[test]
fn public_length_traits_support_custom_inputs() {
    let _ = Style::new()
        .font_size(CustomLength(18.0))
        .width(CustomLength(100.0))
        .gap(CustomLength(8.0));
}
