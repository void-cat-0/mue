use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::{Styleable, StyleableExt};
use taffy::FlexWrap;

/// A wrapping flex row with centered items and 8-pixel row and column gaps.
///
/// The [`StyleableExt::gap`] setter can
/// override both gaps, or set row and column spacing individually.
#[mue_macros::component]
pub fn wrap() -> impl Component {
    flexbox().flex_wrap(FlexWrap::Wrap).items_center().gap(8.)
}
