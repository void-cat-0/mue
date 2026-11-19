use mue_macroquad::node::{Component, FlexboxBuilder, flexbox};
use mue_macroquad::style::StyleableExt;

/// A vertical flex column with an 8-pixel gap.
///
/// Set layout properties with [`Styleable`](mue_macroquad::style::Styleable)
/// and supply children as a tuple or [`Vec`]. Child component bodies run
/// when [`IntoNode::into_node`](crate::IntoNode::into_node) consumes the parent.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _view = stack().children((text("First"), text("Second")));
/// ```
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
