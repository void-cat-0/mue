use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::StyleableExt;

/// Center children on both axes. Set width and height to define the available space.
#[mue_macros::component]
pub fn center() -> impl Component {
    flexbox().items_center().justify_center()
}
