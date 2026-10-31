use mue_macroquad::node::{NodePlan, flexbox::render_container};
use mue_macroquad::style::{Style, Styleable, StyleableExt};
use taffy::FlexWrap;

/// A row that wraps onto additional lines, with 8 pixel row and column gaps.
#[mue_macros::node]
#[theme]
pub fn wrap() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(
        Style::new()
            .flex_wrap(FlexWrap::Wrap)
            .items_center()
            .gap(8.),
        move |style| render_container(style),
    )
}
