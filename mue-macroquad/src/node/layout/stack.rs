use super::flexbox::{FlexboxBuilder, flexbox, render_container};
use crate::style::{Style, StyleableExt};

/// A vertical flex stack with 8 pixels between children.
#[mue_macros::node]
pub fn stack(style: &mut Style) {
    style.provide_defaults(Style::new().flex_column().gap(8.));
    render_container(style);
}

/// A horizontal stack with centered cross-axis alignment.
pub fn hstack() -> FlexboxBuilder {
    flexbox().flex_row().items_center().gap(8.)
}

/// A vertical stack; equivalent to `stack`.
pub fn vstack() -> StackBuilder {
    stack()
}
