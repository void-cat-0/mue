use crate::{layout::use_layout, paint::use_background, style::Style};

/// A flex container. The default direction is a row.
#[mue_macros::node]
pub fn flexbox(style: &mut Style) {
    render_container(style);
}

pub(crate) fn render_container(style: &mut Style) {
    let layout = use_layout(style);
    use_background(style, layout.rect);
}
