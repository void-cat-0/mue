//! Optional reactive token store for application-defined themes.
//!
//! A [`TokenSet`] is a named bag of values of one type. A [`Theme`] registers every
//! key once, at startup, into live signals, and hands out [`Prop`] handles that stay
//! bound to those signals:
//!
//! ```
//! use mue_components::tokens::{Theme, TokenSet};
//! use mue_core::prelude::*;
//!
//! let _ = Scope::null().run(|| {
//!     let theme = Theme::new();
//!     theme.register(&TokenSet::<f32>::new().with("gap", 8_f32));
//!
//!     let gap: Prop<f32> = theme.get("gap");
//!     assert_eq!(gap.get(), 8.);
//!
//!     // Switching themes writes values in place; existing bindings update.
//!     theme.apply(&TokenSet::<f32>::new().with("gap", 16_f32));
//!     assert_eq!(gap.get(), 16.);
//! });
//! ```
//!
//! The store is indexed by value type first and by name second, so the same key can
//! hold a `Brush` and a `FontSize` without colliding, and a key can never be read as
//! the wrong type.
//!
//! This module is a value store, not a theme engine: it has no selectors, no
//! cascade, no specificity, and no dedicated syntax. Components still read ordinary
//! properties; the store only decides which value a property receives.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use mue_core::{
    prelude::{Prop, Signal},
    scope::{OwnedScope, current_scope, inject},
};
use mue_macroquad::node::{IntoNode, Node};
use type_map::TypeMap;

/// A named set of values of one type, for example one theme's brushes.
#[derive(Clone, Debug)]
pub struct TokenSet<T> {
    entries: HashMap<Rc<str>, T>,
}

impl<T> Default for TokenSet<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> TokenSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace one named value.
    pub fn with(mut self, key: impl Into<Rc<str>>, value: T) -> Self {
        self.entries.insert(key.into(), value);
        self
    }

    /// Add or replace one named value in place.
    pub fn set(&mut self, key: impl Into<Rc<str>>, value: T) {
        self.entries.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Rc<str>, &T)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Live signals for one value type.
struct TokenStore<T> {
    slots: HashMap<Rc<str>, Signal<T>>,
}

impl<T: Clone + PartialEq + 'static> TokenStore<T> {
    fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    fn register(&mut self, set: &TokenSet<T>) {
        for (key, value) in set.iter() {
            self.slots
                .entry(key.clone())
                .or_insert_with(|| Signal::new(value.clone()));
        }
    }

    fn get(&self, key: &str) -> Option<Prop<T>> {
        self.slots.get(key).map(|signal| Prop::from(*signal))
    }

    fn apply(&self, set: &TokenSet<T>) -> usize {
        let mut changed = 0;
        for (key, value) in set.iter() {
            let Some(signal) = self.slots.get(key) else {
                continue;
            };
            if signal.set_with(|_| value.clone()) {
                changed += 1;
            }
        }
        changed
    }
}

/// A set of live token stores, indexed by value type and then by name.
///
/// Register every theme's keys before building the view, then read keys as
/// [`Prop`] handles. Switching a theme writes new values into the same signals, so
/// every handle that was already handed out stays live.
///
/// Signals are created by [`Theme::register`], so call it inside a scope that
/// outlives the view.
#[derive(Default)]
pub struct Theme {
    stores: RefCell<TypeMap>,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register every key in `set`. Keys that already exist keep their signal.
    pub fn register<T: Clone + PartialEq + 'static>(&self, set: &TokenSet<T>) {
        self.stores
            .borrow_mut()
            .entry::<TokenStore<T>>()
            .or_insert_with(TokenStore::new)
            .register(set);
    }

    /// Read a token as a live property. Panics when the key is not registered.
    pub fn get<T: Clone + PartialEq + 'static>(&self, key: &str) -> Prop<T> {
        self.try_get(key)
            .unwrap_or_else(|| panic!("token key {key:?} is not registered for this value type"))
    }

    /// Read a token as a live property, or `None` when it is not registered.
    pub fn try_get<T: Clone + PartialEq + 'static>(&self, key: &str) -> Option<Prop<T>> {
        self.stores
            .borrow()
            .get::<TokenStore<T>>()
            .and_then(|store| store.get(key))
    }

    /// Write the values in `set` into their signals, returning how many changed.
    ///
    /// Keys missing from `set` keep their current value. Equal values do not
    /// notify, so switching between themes that share a value costs nothing.
    pub fn apply<T: Clone + PartialEq + 'static>(&self, set: &TokenSet<T>) -> usize {
        self.stores
            .borrow()
            .get::<TokenStore<T>>()
            .map_or(0, |store| store.apply(set))
    }

    /// Registered keys for one value type, in unspecified order.
    pub fn keys<T: Clone + PartialEq + 'static>(&self) -> Vec<Rc<str>> {
        self.stores
            .borrow()
            .get::<TokenStore<T>>()
            .map(|store| store.slots.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Provide this theme to the current scope.
    ///
    /// Panics when there is no live scope, because the theme would never be
    /// visible and the failure would otherwise be silent. Use [`provide_theme`] at
    /// the root of the app, where no scope exists yet.
    pub fn install(theme: &Rc<Theme>) {
        let scope = current_scope();
        assert!(
            !scope.is_null(),
            "Theme::install needs a live scope; use provide_theme at the app root"
        );
        scope.provide(Rc::clone(theme));
    }

    /// The nearest installed theme, if any.
    pub fn current() -> Option<Rc<Theme>> {
        inject::<Rc<Theme>>()
    }
}

/// Build a subtree with an installed theme, using the ordinary scope context.
///
/// This is the counterpart of [`mue_macroquad::inherit::provide_foreground`] for
/// tokens: it creates a scope, provides the theme, and retains that scope on the
/// resulting node.
pub fn provide_theme<T>(
    theme: Rc<Theme>,
    view: impl FnOnce() -> T,
) -> ThemeProvider<impl FnOnce() -> T> {
    ThemeProvider { theme, view }
}

pub struct ThemeProvider<F> {
    theme: Rc<Theme>,
    view: F,
}

impl<F, R> IntoNode for ThemeProvider<F>
where
    F: FnOnce() -> R,
    R: IntoNode,
{
    fn into_node(self) -> Node {
        let scope = OwnedScope::new_inherited();
        scope.provide(Rc::clone(&self.theme));
        let node = scope.run(|| (self.view)().into_node());
        node.retain_owned_scope(scope);
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mue_core::{Size, prelude::*, scope::OwnedScope};

    fn in_scope<R>(f: impl FnOnce() -> R) -> R {
        Scope::null().run(f)
    }

    #[test]
    fn handles_keep_updating_after_apply() {
        in_scope(|| {
            let theme = Theme::new();
            theme.register(&TokenSet::new().with("gap", 8_f32));
            let gap = theme.get::<f32>("gap");
            assert_eq!(gap.get(), 8.);
            assert_eq!(theme.apply(&TokenSet::new().with("gap", 16_f32)), 1);
            assert_eq!(gap.get(), 16.);
        });
    }

    #[test]
    fn equal_values_do_not_recompute() {
        in_scope(|| {
            let theme = Theme::new();
            theme.register(&TokenSet::new().with("gap", 8_f32));
            let gap = theme.get::<f32>("gap");
            let runs = Rc::new(std::cell::Cell::new(0u32));
            let counter = Rc::clone(&runs);
            let derived = computed(move |_| {
                counter.set(counter.get() + 1);
                gap.get()
            });
            assert_eq!(derived.get(), 8.);
            let before = runs.get();
            assert_eq!(theme.apply(&TokenSet::new().with("gap", 8_f32)), 0);
            assert_eq!(derived.get(), 8.);
            assert_eq!(runs.get(), before, "an equal value must not recompute");
        });
    }

    #[test]
    fn keys_are_scoped_per_value_type() {
        in_scope(|| {
            let theme = Theme::new();
            theme.register(&TokenSet::new().with("primary", 1_f32));
            theme.register(&TokenSet::new().with("primary", Size::Px(2.)));
            assert_eq!(theme.get::<f32>("primary").get(), 1.);
            assert_eq!(theme.get::<Size>("primary").get(), Size::Px(2.));
            assert!(theme.try_get::<Size>("missing").is_none());
            assert_eq!(theme.keys::<f32>(), vec![Rc::from("primary")]);
        });
    }

    #[test]
    #[should_panic(expected = "not registered")]
    fn unknown_keys_panic() {
        in_scope(|| {
            let theme = Theme::new();
            let _ = theme.get::<f32>("missing");
        });
    }

    #[test]
    #[should_panic(expected = "needs a live scope")]
    fn install_without_a_scope_panics() {
        let theme = Rc::new(Theme::new());
        Theme::install(&theme);
    }

    #[test]
    fn installed_theme_is_visible_to_children() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let theme = Rc::new(Theme::new());
            theme.register(&TokenSet::new().with("gap", 8_f32));
            Theme::install(&theme);
            let current = Theme::current().expect("installed theme");
            assert_eq!(current.get::<f32>("gap").get(), 8.);
        });
    }
}
