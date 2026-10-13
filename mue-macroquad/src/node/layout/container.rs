use super::flexbox::render_container;
use crate::style::{Style, Styleable, StyleableExt};
use taffy::LengthPercentageAuto;

/// A centered, padded column with a default maximum width of 1280 pixels.
#[mue_macros::node]
pub fn container(style: &mut Style) {
    style.provide_defaults(
        Style::new()
            .flex_column()
            .w_full()
            .max_width(1280.)
            .margin_x(LengthPercentageAuto::auto())
            .padding_x(24.),
    );
    render_container(style);
}
