use mue_macroquad::node::{NodePlan, flexbox::render_container};
use mue_macroquad::style::{Style, StyleableExt};

/// A whole-card action surface. Supply `on_activate` and arbitrary children.
/// Child tap handlers take priority through the existing gesture arena.
#[mue_macros::node]
#[theme]
pub fn link_overlay() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().flex_column(), render_container)
}
