use crate::{layout::use_layout, paint::use_background, style::Style};

/// A flex container. The default direction is a row.
#[mue_macros::node]
#[theme]
pub fn flexbox() -> super::NodePlan<impl FnOnce(&mut Style)> {
    super::NodePlan::new(Style::new(), render_container)
}

pub fn render_container(style: &mut Style) {
    let layout = use_layout(style);
    use_background(style, layout.rect);
}
