use taffy::Display;

use crate::{
    layout::use_layout,
    paint::use_background,
    style::{Style, Styleable},
};

#[mue_macros::node]
pub fn flexbox(style: &mut Style) {
    let layout = use_layout(style);
    use_background(style, layout.rect);
}

pub fn div() -> FlexboxBuilder {
    flexbox().display(Display::Block)
}
