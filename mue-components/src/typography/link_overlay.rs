use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::StyleableExt;

/// A whole-card action surface. Supply `on_activate` and arbitrary children.
/// Child tap handlers take priority through the existing gesture arena.
#[mue_macros::component]
pub fn link_overlay() -> impl Component {
    flexbox().flex_column()
}
