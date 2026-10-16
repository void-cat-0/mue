use super::flexbox::render_container;
use crate::{
    color::Color,
    style::{Style, Styleable, StyleableExt},
};
use mue_core::prop::PropValue;
use taffy::Dimension;

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
    #[default(1.)]
    #[length]
    thickness: f32,
) {
    style.provide_defaults(
        Style::new()
            .flex_none()
            .width(mue_core::effect::computed(move |_| {
                let axis = mue_core::signal::Access::get(&orientation);
                let thickness = mue_core::signal::Access::get(&thickness).max(0.);
                if axis == Orientation::Horizontal {
                    Dimension::percent(1.)
                } else {
                    Dimension::length(thickness)
                }
            }))
            .height(mue_core::effect::computed(move |_| {
                let axis = mue_core::signal::Access::get(&orientation);
                let thickness = mue_core::signal::Access::get(&thickness).max(0.);
                if axis == Orientation::Vertical {
                    Dimension::percent(1.)
                } else {
                    Dimension::length(thickness)
                }
            }))
            .background(Color::from_rgba(128, 128, 128, 96)),
    );
    render_container(style);
}
