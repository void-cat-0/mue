//! Conventions for component state: controlled values and compound parts.
//!
//! # Controlled and uncontrolled values
//!
//! A value-carrying control declares a `#[model]` parameter with a
//! `#[default]` and an `#[event] on_change: Callback<T>`:
//!
//! - **Controlled**: the caller passes its own signal (`.checked(signal)`), so
//!   the caller owns the state and sees every change.
//! - **Uncontrolled**: the caller passes nothing (or only a starting value
//!   through the default), and the control keeps its own signal.
//! - **Notified**: either way, `.on_change(|value| ..)` runs after each change
//!   made by the control.
//!
//! Inside the body, wrap the two in [`Controllable`] and change the value only
//! through [`Controllable::set`], which updates the signal and emits.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::prelude::*;
//! use mue_macroquad::color::Color;
//!
//! #[component]
//! fn toggle_chip(
//!     #[model] #[default(false)] on: bool,
//!     #[event] on_change: Callback<bool>,
//! ) -> impl Component {
//!     let state = Controllable::new(on, on_change);
//!     let value = state.signal();
//!     flexbox()
//!         .focusable(true)
//!         .on_activate(move || state.set(!state.get_untracked()))
//!         .background(value.map(|on| {
//!             if on { Color::from_rgba(64, 120, 255, 255) } else { Color::from_rgba(46, 56, 82, 255) }.into()
//!         }))
//! }
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let shared = signal(true);
//! let _controlled = toggle_chip().on(shared);
//! let _uncontrolled = toggle_chip().on_change(|on: &bool| println!("now {on}"));
//! # });
//! ```
//!
//! # Compound components
//!
//! A multi-part component (`tabs::root`, `tabs::tab`, `tabs::panel`) shares
//! state from the root to its parts through context. The root calls
//! [`provide_part`] before returning its builder; each part calls
//! [`use_part`], which panics with a clear message when the part is used
//! outside its root. The parts are ordinary components, so callers compose and
//! style them freely; a convenience constructor can assemble the common
//! arrangement.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::prelude::*;
//! use std::rc::Rc;
//!
//! #[derive(Clone, Copy)]
//! struct TabsContext {
//!     selected: Signal<usize>,
//! }
//!
//! #[component]
//! fn tabs_root(#[model] #[default(0)] selected: usize) -> impl Component {
//!     provide_part(TabsContext { selected });
//!     stack()
//! }
//!
//! #[component]
//! fn tab(index: usize, label: Rc<str>) -> impl Component {
//!     let tabs: TabsContext = use_part("tab", "tabs_root");
//!     let index = index.get();
//!     button(label).primary(tabs.selected.get_untracked() == index)
//!         .on_activate(move || tabs.selected.set(index))
//! }
//!
//! let _tabs = tabs_root().children((tab(0, "Songs"), tab(1, "Charts")));
//! ```

use mue_core::{
    prelude::*,
    scope::{inject, provide},
};
use mue_macroquad::node::Callback;

/// A value that may be owned by the caller (controlled) or by the component.
pub struct Controllable<T: 'static> {
    signal: Signal<T>,
    on_change: Callback<T>,
}

impl<T: 'static> Clone for Controllable<T> {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal,
            on_change: self.on_change.clone(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Controllable<T> {
    /// Combine a `#[model]` signal and an `#[event]` callback.
    pub fn new(signal: Signal<T>, on_change: Callback<T>) -> Self {
        Self { signal, on_change }
    }

    /// The current value. Tracked.
    pub fn get(&self) -> T {
        self.signal.get_clone()
    }

    pub fn get_untracked(&self) -> T {
        self.signal.get_clone_untracked()
    }

    /// The value as a read-only signal, for style bindings.
    pub fn signal(&self) -> ReadSignal<T> {
        *self.signal
    }

    /// Change the value. When it differs, update the signal and emit
    /// `on_change` with the new value.
    pub fn set(&self, value: T) {
        if self.signal.get_clone_untracked() == value {
            return;
        }
        self.signal.set(value.clone());
        self.on_change.emit(&value);
    }
}

/// Share a compound component's state with its parts. Call in the root
/// component's body before returning its builder.
pub fn provide_part<T: 'static>(context: T) {
    provide(context);
}

/// Read the state shared by the nearest root of a compound component.
///
/// # Panics
///
/// When `part` is used outside `root`.
pub fn use_part<T: Clone + 'static>(part: &str, root: &str) -> T {
    inject::<T>().unwrap_or_else(|| panic!("`{part}` must be used inside `{root}`"))
}

/// Read the shared state, or `None` outside the root.
pub fn try_use_part<T: Clone + 'static>() -> Option<T> {
    inject::<T>()
}
