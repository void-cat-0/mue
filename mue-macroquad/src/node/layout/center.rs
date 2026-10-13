use super::flexbox::render_container;
use crate::style::{Style, StyleableExt};

/// Center children on both axes. Set width and height to define the available space.
#[mue_macros::node]
pub fn center(style: &mut Style) {
    style.provide_defaults(Style::new().items_center().justify_center());
    render_container(style);
}
