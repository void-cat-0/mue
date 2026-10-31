use mue_macroquad::node::{NodePlan, flexbox::render_container};
use mue_macroquad::style::{Style, StyleableExt};

/// Center children on both axes. Set width and height to define the available space.
#[mue_macros::node]
#[theme]
pub fn center() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().items_center().justify_center(), move |style| {
        render_container(style)
    })
}
