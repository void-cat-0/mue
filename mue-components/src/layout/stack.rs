use mue_macroquad::node::{
    NodePlan,
    flexbox::{FlexboxBuilder, flexbox, render_container},
};
use mue_macroquad::style::{Style, StyleableExt};

/// A vertical flex stack with 8 pixels between children.
#[mue_macros::node]
#[theme]
pub fn stack() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().flex_column().gap(8.), move |style| {
        render_container(style)
    })
}

/// A horizontal stack with centered cross-axis alignment.
pub fn hstack() -> FlexboxBuilder {
    flexbox().flex_row().items_center().gap(8.)
}

/// A vertical stack; equivalent to `stack`.
pub fn vstack() -> StackBuilder {
    stack()
}
