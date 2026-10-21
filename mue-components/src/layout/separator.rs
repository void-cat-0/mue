use mue_core::prop::PropValue;
use mue_macroquad::node::flexbox::render_container;
use mue_macroquad::{
    color::Color,
    style::{Size, Style, Styleable, StyleableExt},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}
impl PropValue for Orientation {}

/// A one-pixel divider. A vertical divider requires a bounded parent height.
#[mue_macros::node]
pub fn separator(
    style: &mut Style,
    #[default] orientation: Orientation,
    #[default(Size::Px(1.))] thickness: Size,
) {
    style.provide_defaults(
        Style::new()
            .flex_none()
            .width(mue_core::effect::computed(move |_| {
                let axis = mue_core::signal::Access::get(&orientation);
                let thickness = mue_core::signal::Access::get(&thickness);
                let thickness = match thickness {
                    Size::Px(value) => Size::Px(value.max(0.)),
                    value => value,
                };
                if axis == Orientation::Horizontal {
                    Size::Percent(100.)
                } else {
                    thickness
                }
            }))
            .height(mue_core::effect::computed(move |_| {
                let axis = mue_core::signal::Access::get(&orientation);
                let thickness = mue_core::signal::Access::get(&thickness);
                let thickness = match thickness {
                    Size::Px(value) => Size::Px(value.max(0.)),
                    value => value,
                };
                if axis == Orientation::Vertical {
                    Size::Percent(100.)
                } else {
                    thickness
                }
            }))
            .background(Color::from_rgba(128, 128, 128, 96)),
    );
    render_container(style);
}
