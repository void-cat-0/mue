//! Layout, typography, and graphics components for the Macroquad renderer.
//!
//! Components are styleable builders. Constructors and setters configure them, and
//! consuming a builder through [`IntoNode::into_node`] builds its node synchronously.
//! [`mue_macroquad::App::new`] performs this conversion for the root builder.
//!
//! # Author a component
//!
//! Use [`#[component]`](macro@component) to compose existing builders. Return
//! `impl Component` or a concrete builder to carry the caller's styles and the
//! same node owner through to the terminal declaration.
//!
//! ```no_run
//! use mue_components::prelude::*;
//! use std::rc::Rc;
//!
//! #[component]
//! fn greeting(caption: Rc<str>, #[default] loud: bool) -> impl Component {
//!     center()
//!         .opacity(loud.map(|enabled| if enabled { 1.0 } else { 0.75 }))
//!         .children(text(caption))
//! }
//!
//! let _node = greeting("Hello").loud(true).into_node();
//! ```
//!
//! Required parameters appear in the generated constructor. Every ordinary
//! parameter has a same-named setter. Mark an input with `#[default]` to omit
//! it from the constructor and use the type's [`Default`] value, or with
//! `#[default(expr)]` to supply a value. Defaults run once for unset inputs at
//! construction time and cannot refer to any declaration parameter.
//!
//! An ordinary input of type `T` is a [`Prop<T>`](mue_core::prop::Prop) inside the
//! body. Pass that property to a style setter, or derive a binding with
//! [`Prop::map`](mue_core::prop::Prop::map). Reading a property in a plain `if` samples
//! it once. Changing a signal updates existing bindings without rerunning the
//! body. With `#[model]`, the builder takes a writable
//! [`Signal<T>`](mue_core::signal::Signal). A model default initializes a fresh
//! signal in the node's scope.
//!
//! # Author a terminal node
//!
//! Use [`#[node]`](macro@node) to initialize layout, painting, or input directly.
//! The macro supplies [`NodeContext`] as the first parameter. The declaration
//! returns a [`Node`] through [`NodeContext::finish`].
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_macroquad::layout::use_layout;
//!
//! #[node]
//! fn spacer(ctx: NodeContext, #[default(8.0)] height: f32) -> Node {
//!     ctx.finish(Style::new().height(height), |style| {
//!         use_layout(style);
//!     })
//! }
//!
//! let _node = spacer().height(12.0).into_node();
//! ```
//!
//! `finish` merges defaults, binds the parent's transition, opacity, font, and
//! transform, builds the declared children, then runs the initialization closure
//! with the resulting [`Style`](mue_macroquad::style::Style). It attaches the
//! registered hooks afterward. Append callbacks with fields such as
//! [`Style::on_render`](mue_macroquad::style::Style::on_render), or register them
//! up front through [`Styleable::on_render`](mue_macroquad::style::Styleable::on_render).
//! For a handwritten builder, implement [`BuildNode`] to receive the same context
//! and a blanket [`IntoNode`] implementation.
//!
//! See [`layout`] for deferred children, context, and lifetime requirements.
//!
//! # Style a builder
//!
//! [`Styleable`](mue_macroquad::style::Styleable) setters take static values and
//! reactive bindings. On one builder, a later setter replaces the same property.
//! At construction, caller and outer component styles take precedence over inner
//! builder styles, then node defaults and framework defaults or inheritance.
//! Children and hooks have separate merge rules.
//!
//! [`style!`](macro@style) produces the same typed [`Style`](mue_macroquad::style::Style)
//! from semicolon-separated declarations. Put Rust expressions in braces to pass
//! reactive values directly to a setter. See [`tokens`] for the optional
//! application token store.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::prelude::*;
//!
//! let owner = mue_core::scope::OwnedScope::new();
//! owner.run(|| {
//!     let opacity = signal(0.8_f32);
//!     let declarations = style! {
//!         width: 100%;
//!         max-width: 640px;
//!         padding: 16px 24px;
//!         gap: 8px 12px;
//!         opacity: {opacity};
//!         transform: translateY(8px) rotate(-2deg);
//!         transition: 240ms ease-in-out 0.65;
//!     };
//!     let binding = declarations.declared_opacity().unwrap();
//!     assert_eq!(binding.get(), 0.8);
//!     opacity.set(0.4);
//!     assert_eq!(binding.get(), 0.4);
//!     let _view = flexbox().style(declarations);
//! });
//! ```

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
    KeyedItem, Node, NodeContext, NodeRef, PathBuilder, ShapeBuilder, Span, SpanStyle,
    TextOverflow, TextVerticalAlign, TextWrap, flexbox, join_children, keyed, map_keyed,
    map_keyed_prop, path, shape, show_if, text,
};
pub use mue_macroquad::path::{FillRule, PathData, PathFit, StrokeStyle};
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
