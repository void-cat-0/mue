use mue_macroquad::node::flexbox::render_container;
use mue_macroquad::style::{Style, Styleable, StyleableExt};
use taffy::FlexWrap;

/// A row that wraps onto additional lines, with 8 pixel row and column gaps.
#[mue_macros::node]
pub fn wrap(style: &mut Style) {
    style.provide_defaults(
        Style::new()
            .flex_wrap(FlexWrap::Wrap)
            .items_center()
            .gap(8.),
    );
    render_container(style);
}
