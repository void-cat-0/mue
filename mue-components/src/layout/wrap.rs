use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::{Styleable, StyleableExt};
use taffy::FlexWrap;

/// A row that wraps onto additional lines, with 8 pixel row and column gaps.
#[mue_macros::component]
pub fn wrap() -> impl Component {
    flexbox().flex_wrap(FlexWrap::Wrap).items_center().gap(8.)
}
