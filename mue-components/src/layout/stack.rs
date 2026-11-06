use mue_macroquad::node::{Component, FlexboxBuilder, flexbox};
use mue_macroquad::style::StyleableExt;

/// A vertical flex stack with 8 pixels between children.
#[mue_macros::component]
pub fn stack() -> impl Component {
    flexbox().flex_column().gap(8.)
}

/// A horizontal stack with centered cross-axis alignment.
pub fn hstack() -> FlexboxBuilder {
    flexbox().flex_row().items_center().gap(8.)
}

/// A vertical stack; equivalent to `stack`.
pub fn vstack() -> StackBuilder {
    stack()
}
