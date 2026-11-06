#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod perf;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod visual_tests;

use super::{IntoNode, Node};
use crate::style::Style;
use mue_core::{
    scope::{OwnedScope, Scope},
    signal::{Signal, signal},
};
use std::rc::Rc;

pub(crate) struct HoverOwner {
    pub(crate) scope: OwnedScope,
    pub(crate) signal: Signal<bool>,
}
impl HoverOwner {
    pub(crate) fn new() -> Rc<Self> {
        let scope = OwnedScope::new_inherited();
        let signal = scope.run(|| signal(false));
        Rc::new(Self { scope, signal })
    }
}

/// Construct one real node inside the context the host provides.
///
/// Every `BuildNode` also implements [`IntoNode`] through a blanket impl, so an
/// author only writes `build`; `into_node` starts a fresh context for it. A type that
/// implements `BuildNode` therefore must not implement `IntoNode` itself.
pub trait BuildNode: Sized {
    fn build(self, ctx: NodeContext) -> Node;
}

impl<T: BuildNode> IntoNode for T {
    fn into_node(self) -> Node {
        let context = NodeContext::new();
        let scope = context.scope();
        scope.run(|| self.build(context))
    }
}

/// The owner and accumulated styles for one real node's construction.
pub struct NodeContext {
    owner: OwnedScope,
    style: Style,
}
impl Default for NodeContext {
    fn default() -> Self {
        Self::new()
    }
}
impl NodeContext {
    pub fn new() -> Self {
        Self {
            owner: OwnedScope::new_inherited(),
            style: Style::new(),
        }
    }
    pub fn scope(&self) -> Scope {
        self.owner.scope()
    }
    #[doc(hidden)]
    pub fn push_style(&mut self, style: Style) {
        self.style.provide_defaults(style);
    }

    /// Complete this node: merge defaults, bind the parent's state, materialize
    /// declared children, then run `body` with the resulting style. The callback
    /// runs synchronously and is never retained by the node.
    pub fn finish(self, defaults: Style, body: impl FnOnce(&mut Style)) -> Node {
        let Self { owner, mut style } = self;
        style.provide_defaults(defaults);
        Node::finish_parts(owner, style, body)
    }
}
