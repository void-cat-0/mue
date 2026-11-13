//! Layout, typography, and graphics components for the Macroquad renderer.
pub mod controls;
pub mod graphics;
pub mod layout;
pub mod overlay;
pub mod tokens;
pub mod typography;
pub use controls::*;
pub use graphics::*;
pub use layout::*;
pub use mue_macroquad::node::{
    BuildNode, Children, ChildrenPlan, Component, FlexboxBuilder, IntoChildren, IntoNode,
    KeyedItem, Node, NodeContext, NodeRef, ShapeBuilder, Span, SpanStyle, TextOverflow,
    TextVerticalAlign, TextWrap, flexbox, join_children, keyed, map_keyed, map_keyed_prop, shape,
    show_if, text,
};
pub use mue_macros::{component, node, style};
pub use overlay::*;
pub use tokens::{Theme, ThemeProvider, TokenSet, provide_theme};
pub use typography::*;

pub mod prelude {
    pub use crate::*;
    pub use mue_macroquad::node::{flexbox, shape, text};
    pub use mue_macroquad::style::{Style, Styleable, StyleableExt};
}

#[doc(hidden)]
pub mod __private {
    pub use mue_macroquad as host;
}
