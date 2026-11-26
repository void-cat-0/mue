//! Slots and event callbacks for component authors (Vue's slots and emits).
//!
//! In a [`#[component]`](mue_macros::component) declaration, mark a parameter
//! `#[slot]` with type [`Slot`] or [`ScopedSlot<T>`], or `#[event]` with type
//! [`Callback<T>`]. The generated builder gets a setter that takes a view or a
//! closure, and the body receives the value below. See the `mue-components`
//! crate documentation for a complete example.

use std::{cell::RefCell, rc::Rc};

use mue_core::Owned;

use super::{Children, IntoChildren, IntoNode, Node};

/// An optional child view supplied by the component's caller.
#[derive(Default)]
pub struct Slot(Option<Box<dyn FnOnce() -> Node>>);

impl Slot {
    pub fn new(view: impl IntoNode + 'static) -> Self {
        Self(Some(Box::new(move || view.into_node())))
    }

    /// Whether the caller supplied this slot.
    pub fn is_set(&self) -> bool {
        self.0.is_some()
    }

    /// Build the slot's node, if supplied.
    pub fn render(self) -> Option<Node> {
        self.0.map(|view| view())
    }

    /// Build the slot, or `fallback` when the caller left it empty.
    pub fn or_else<N: IntoNode>(self, fallback: impl FnOnce() -> N) -> Node {
        match self.0 {
            Some(view) => view(),
            None => fallback().into_node(),
        }
    }
}

impl IntoChildren for Slot {
    fn into_children(self) -> Owned<Children> {
        match self.render() {
            Some(node) => node.into_children(),
            None => ().into_children(),
        }
    }
}

/// A child template that receives data from the component (a scoped slot).
pub struct ScopedSlot<T>(Option<Rc<dyn Fn(T) -> Node>>);

impl<T> Default for ScopedSlot<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Clone for ScopedSlot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> ScopedSlot<T> {
    pub fn new<N: IntoNode>(template: impl Fn(T) -> N + 'static) -> Self {
        Self(Some(Rc::new(move |value| template(value).into_node())))
    }

    pub fn is_set(&self) -> bool {
        self.0.is_some()
    }

    /// Build one instance for `value`; an empty node when unset.
    pub fn render(&self, value: T) -> Node {
        match &self.0 {
            Some(template) => template(value),
            None => Node::empty(),
        }
    }

    /// Build one instance, or `fallback` when unset.
    pub fn render_or<N: IntoNode>(&self, value: T, fallback: impl FnOnce(T) -> N) -> Node {
        match &self.0 {
            Some(template) => template(value),
            None => fallback(value).into_node(),
        }
    }
}

/// Event handlers registered by a component's caller.
///
/// Every registered handler runs, in registration order, on [`Callback::emit`].
type Handlers<T> = Rc<RefCell<Vec<Box<dyn FnMut(&T)>>>>;

pub struct Callback<T>(Handlers<T>);

impl<T> Default for Callback<T> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Callback<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a handler.
    pub fn add(&self, handler: impl FnMut(&T) + 'static) {
        self.0.borrow_mut().push(Box::new(handler));
    }

    /// Whether any handler is registered.
    pub fn is_set(&self) -> bool {
        !self.0.borrow().is_empty()
    }

    /// Call every handler with `value`. Handlers may register more handlers;
    /// those run from the next emit.
    pub fn emit(&self, value: &T) {
        let mut handlers = std::mem::take(&mut *self.0.borrow_mut());
        for handler in &mut handlers {
            handler(value);
        }
        let mut current = self.0.borrow_mut();
        let added = std::mem::replace(&mut *current, handlers);
        current.extend(added);
    }
}
