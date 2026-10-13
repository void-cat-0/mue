use crate::node::layout::flexbox::render_container;
use crate::style::{Style, StyleableExt};

/// A whole-card action surface. Supply `on_activate` and arbitrary children.
/// Child tap handlers take priority through the existing gesture arena.
#[mue_macros::node]
pub fn link_overlay(style: &mut Style) {
    style.provide_defaults(Style::new().flex_column());
    render_container(style);
}
