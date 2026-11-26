//! Layout builders for composing and retaining child nodes.
//!
//! [`Styleable::children`](mue_macroquad::style::Styleable::children) stores a
//! one-shot [`ChildrenPlan`](crate::ChildrenPlan). Child builder bodies run when
//! the parent finishes, after its scope and inherited style state are installed.
//! Tuples can mix builders, prebuilt nodes, plans, and owned child lists. Arrays
//! and vectors preserve order. A prebuilt [`Node`](crate::Node) keeps its original
//! context. Calling [`IntoChildren::into_children`](crate::IntoChildren::into_children)
//! materializes the input immediately.
//!
//! Set shared [`provide`](mue_core::scope::provide) values in the parent
//! component body before returning a builder, or in the node body before
//! [`NodeContext::finish`](crate::NodeContext::finish). The initialization closure
//! can [take](mue_macroquad::node::take_children) and
//! [replace](mue_macroquad::node::set_children) the materialized children before
//! [`use_layout`](mue_macroquad::layout::use_layout) captures them.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::scope::{inject, provide};
//!
//! #[component]
//! fn child() -> impl Component {
//!     assert_eq!(inject::<u32>(), Some(42));
//!     flexbox()
//! }
//!
//! #[component]
//! fn parent() -> impl Component {
//!     provide(42_u32);
//!     stack().children((child(), flexbox()))
//! }
//!
//! let _tree = parent().into_node();
//! ```
//!
//! # Retained children and context
//!
//! - [`map_keyed`](crate::map_keyed) retains each node and its state while its
//!   unique key is present. Reordering neither rebuilds a retained node nor
//!   replaces the original values passed to its construction callback.
//! - [`map_keyed_prop`](crate::map_keyed_prop) also supplies live row value and
//!   index properties through [`KeyedItem`](crate::KeyedItem).
//! - [`keyed`](crate::keyed) replaces one subtree when the selector changes.
//! - [`show_if`](crate::show_if) constructs the node once at mount, even if initially
//!   hidden. Toggling it retains the same node and local state.
//!
//! New keyed children inherit the mount context. Removing a key drops its node
//! and the resources owned by its construction callback.
//!
//! The synchronous node initialization closure may borrow local data. Retained
//! child plans and hooks require `'static` inputs. Nodes own resources created
//! in their construction scope. External signals and dynamic properties still
//! depend on their original [`OwnedScope`](mue_core::scope::OwnedScope).
//! Construction runs synchronously. For scope ownership across asynchronous
//! polls, see [`scope_future`](mue_core::scope::scope_future).

pub mod aspect_ratio;
pub mod center;
pub mod container;
pub mod div;
pub mod float;
pub mod grid;
pub mod pages;
pub mod safe_area;
pub mod scroll_area;
pub mod separator;
pub mod stack;
pub mod wrap;

pub use aspect_ratio::*;
pub use center::*;
pub use container::*;
pub use div::*;
pub use float::*;
pub use grid::*;
pub use pages::*;
pub use safe_area::*;
pub use scroll_area::*;
pub use separator::*;
pub use stack::*;
pub use wrap::*;
