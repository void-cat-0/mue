//! Layout, typography, and graphics components for the Macroquad renderer.
pub mod graphics;
pub mod layout;
pub mod typography;
pub use graphics::*;
pub use layout::*;
pub use mue_macroquad::factory::FactoryUse;
pub use mue_macroquad::node::{
    Children, Component, FlexboxBuilder, IntoChildren, IntoNode, Node, NodeRef, ShapeBuilder, Span,
    SpanStyle, TextOverflow, TextVerticalAlign, TextWrap, flexbox, join_children, map_keyed, shape,
    show_if, text,
};
pub use mue_macros::{component, node, style};
pub use typography::*;

pub mod prelude {
    pub use crate::*;
    pub use mue_macroquad::factory::FactoryUse;
    pub use mue_macroquad::node::{flexbox, shape, text};
    pub use mue_macroquad::style::{Style, Styleable, StyleableExt};
}

#[doc(hidden)]
pub mod __private {
    pub use mue_macroquad as host;
}
