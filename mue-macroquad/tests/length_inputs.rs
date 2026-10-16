use mue_core::prelude::*;
use mue_macroquad::{
    node::{Component, IntoNode, flexbox},
    style::{FontSize, Size, Styleable},
};

#[mue_macros::component]
fn model_component(#[model] size: Size, #[model] font_size: FontSize) -> impl Component {
    size.set(Size::Percent(50.0));
    font_size.set(FontSize::Percent(150.0));
    flexbox().width(size).font_size(font_size)
}

#[test]
fn generated_model_parameters_write_back_unit_changes() {
    let size = signal(Size::Px(12.0));
    let font_size = signal(FontSize::Px(18.0));
    let _node = model_component(size, font_size).into_node();

    assert_eq!(size.get(), Size::Percent(50.0));
    assert_eq!(font_size.get(), FontSize::Percent(150.0));
}
