use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::{Styleable, StyleableExt};
use taffy::LengthPercentageAuto;

/// A centered, padded column with a default maximum width of 1280 pixels.
#[mue_macros::component]
pub fn container() -> impl Component {
    flexbox()
        .flex_column()
        .w_full()
        .max_width(1280.)
        .margin_x(LengthPercentageAuto::auto())
        .padding_x(24.)
}
