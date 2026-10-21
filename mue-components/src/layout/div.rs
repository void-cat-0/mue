use mue_macroquad::node::flexbox::{FlexboxBuilder, flexbox};
use mue_macroquad::style::Styleable;
use taffy::Display;

/// A block container using Taffy's block layout instead of flex layout.
pub fn div() -> FlexboxBuilder {
    flexbox().display(Display::Block)
}
