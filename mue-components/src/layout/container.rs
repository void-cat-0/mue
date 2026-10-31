use mue_macroquad::node::{NodePlan, flexbox::render_container};
use mue_macroquad::style::{Style, Styleable, StyleableExt};
use taffy::LengthPercentageAuto;

/// A centered, padded column with a default maximum width of 1280 pixels.
#[mue_macros::node]
#[theme]
pub fn container() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(
        Style::new()
            .flex_column()
            .w_full()
            .max_width(1280.)
            .margin_x(LengthPercentageAuto::auto())
            .padding_x(24.),
        move |style| render_container(style),
    )
}
