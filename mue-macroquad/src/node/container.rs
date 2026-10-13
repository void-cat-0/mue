use taffy::Display;

use crate::{
    layout::use_layout,
    paint::use_background,
    style::{Style, Styleable},
};

/// A flex container that lays out children and paints its background.
/// Use `.flex_column()` for vertical content; the default direction is a row.
#[mue_macros::node]
pub fn flexbox(style: &mut Style) {
    let layout = use_layout(style);
    use_background(style, layout.rect);
}

/// A block container using Taffy's block layout instead of flex layout.
pub fn div() -> FlexboxBuilder {
    flexbox().display(Display::Block)
}
