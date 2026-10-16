use mue_core::prelude::*;
use mue_macroquad::{
    node::*,
    style::{FontSize, Size, Style, Styleable, StyleableExt, percent, px, vh, vw},
};

#[mue_macros::node]
fn length_node(
    style: &mut Style,
    required: Size,
    #[default(mue_macroquad::style::Size::Percent(50.))] optional: Size,
    #[default(1.0)] weight: f32,
) {
    let _ = (style, required, optional, weight);
}

#[mue_macros::component]
fn length_component(required: Size, #[default(Size::Px(8.0))] optional: Size) -> impl Component {
    flexbox().width(required).column_gap(optional)
}

#[mue_macros::component]
fn font_component(size: FontSize, #[default] width: Size) -> impl Component {
    flexbox().font_size(size).width(width)
}

#[mue_macros::component]
fn model_component(#[model] size: Size, #[model] font_size: FontSize) -> impl Component {
    size.set(Size::Percent(50.0));
    font_size.set(FontSize::Percent(150.0));
    flexbox().width(size).font_size(font_size)
}

#[mue_macros::component]
fn default_size_component(
    #[default(mue_macroquad::style::Size::Percent(50.))] width: Size,
) -> impl Component {
    flexbox().width(width)
}

#[test]
fn generated_node_length_parameters_resolve_size_units_and_compatible_scalars() {
    let value = signal(12.0);
    let builder = length_node(value).optional(px(16.0)).weight(*value);
    assert_eq!(builder.required.get(), Size::Px(12.0));
    assert_eq!(builder.optional.unwrap().get(), Size::Px(16.0));
    assert_eq!(builder.weight.unwrap().get(), 12.0);
    value.set(24.0);
    assert_eq!(builder.required.get(), Size::Px(24.0));
    assert_eq!(builder.weight.unwrap().get(), 24.0);
    let _ = length_node(12.0).required(vw(10.0)).optional(vh(5.0));
}

#[test]
fn generated_component_length_parameters_support_constructors_and_setters() {
    let builder = length_component(px(12.0)).optional(px(16.0));
    assert_eq!(builder.required.get(), Size::Px(12.0));
    assert_eq!(builder.optional.unwrap().get(), Size::Px(16.0));
    let _ = length_component(vw(10.0)).required(vh(5.0));
}

#[test]
fn ordinary_font_size_parameters_accept_standard_pixel_conversions() {
    let source = signal(18_f32);
    let builder = font_component(source).width(240_f32);
    assert_eq!(builder.size.get(), FontSize::Px(18.));
    assert_eq!(builder.width.unwrap().get(), Size::Px(240.));
    source.set(24.);
    assert_eq!(builder.size.get(), FontSize::Px(24.));
    let _ = font_component(18_f32).into_node();
}

#[test]
fn generated_length_defaults_accept_size_expressions() {
    let _ = default_size_component().into_node();
    let _ = default_size_component().width(Size::Vw(10.0));
}

#[test]
fn generated_model_parameters_keep_writable_size_values() {
    let size = signal(Size::Px(12.0));
    let font_size = signal(FontSize::Px(18.0));
    let node = model_component(size, font_size).into_node();

    assert_eq!(size.get(), Size::Percent(50.0));
    assert_eq!(font_size.get(), FontSize::Percent(150.0));
    drop(node);
}

#[test]
fn existing_length_nodes_accept_size_values_and_scalar_compatibility() {
    let size = signal(Size::Px(8.0));
    let _ = float().offset_x(size).offset_y(vh(1.0));
    let _ = separator().thickness(vw(0.25));
    let _ = separator().thickness(signal(2.0));
    let _ = aspect_ratio().ratio(1.5);
}

#[test]
fn layout_traits_accept_size_values_and_f32_compatibility() {
    let scalar = signal(12.0);
    let dimension = signal(Size::Auto);
    let _ = Style::new()
        .width(12.0)
        .width(scalar)
        .width(*scalar)
        .width(Prop::Static(Size::Px(12.0)))
        .width(Size::Auto)
        .width(dimension)
        .width(*dimension)
        .width(Prop::Static(Size::Percent(50.0)))
        .gap(scalar)
        .padding((scalar,))
        .padding((px(1.0), percent(2.0)))
        .padding((px(1.0), vw(2.0), vh(3.0), percent(4.0)))
        .margin((Size::Auto,))
        .border((Size::Px(1.0),))
        .font_size(scalar)
        .font_size(FontSize::Px(18.0))
        .line_height(*scalar)
        .radius(Size::Px(4.0))
        .opacity(*scalar)
        .flex_grow(1.0)
        .flex_shrink(0.0);
}

struct CustomLength(f32);

impl From<CustomLength> for Prop<Size> {
    fn from(value: CustomLength) -> Self {
        Prop::Static(Size::Px(value.0))
    }
}

fn generic_width(value: impl Into<Prop<Size>>) -> Style {
    Style::new().width(value)
}

fn generic_font_size(value: impl Into<Prop<FontSize>>) -> Style {
    Style::new().font_size(value)
}

#[test]
fn standard_into_bounds_work_without_length_specific_traits() {
    let number = signal(12_f32);
    let _ = generic_width(12_f32);
    let _ = generic_width(number);
    let _ = generic_width(*number);
    let _ = generic_width(Prop::Static(12_f32));
    let _ = generic_width(Size::Auto);
    let _ = generic_width(taffy::Dimension::percent(0.5));
    let _ = generic_font_size(18_f32);
    let _ = generic_font_size(number);
    let _ = generic_font_size(*number);
    let _ = generic_font_size(Prop::Static(18_f32));
    let _ = generic_font_size(FontSize::Rem(1.5));
}

#[test]
fn standard_from_supports_custom_size_inputs() {
    let _ = Style::new()
        .width(CustomLength(100.0))
        .gap(CustomLength(8.0));
}
