//! Optional reactive token store for application-defined themes.
//!
//! A [`TokenSet`] is a named bag of values of one type. Build one with
//! [`token_set!`](crate::token_set) for literal keys, or with [`TokenSet::with`]
//! when a key is computed. A [`Theme`] registers every key once, at startup, into live signals,
//! and hands out [`Prop`] handles that stay bound to those signals:
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
//! the wrong type. A missing key panics with the value type, the keys registered for
//! that type, and any other types that own the same key.
//!
//! This module is a value store, not a theme engine: it has no selectors, no
//! cascade, and no specificity. Its macros only construct sets and read values;
//! components still read ordinary properties, and the store only decides which
//! value a property receives.

use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    rc::Rc,
};

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

/// Build a [`TokenSet`] from `key: value` entries.
///
/// A key is either a bare identifier, which is stringified, or a string literal.
/// The two forms may be mixed, so dotted keys can sit next to plain ones. Later
/// entries replace earlier ones with the same key. The value type is inferred
/// from the entries, so an empty set needs an explicit type.
///
/// ```
/// use mue_components::{token_set, tokens::TokenSet};
///
/// let spacing: TokenSet<f32> = token_set! {
///     gap: 8.,
///     "gap.tight": 4.,
/// };
///
/// assert_eq!(spacing.get("gap"), Some(&8.));
/// assert_eq!(spacing.get("gap.tight"), Some(&4.));
/// ```
#[macro_export]
macro_rules! token_set {
    () => {
        $crate::tokens::TokenSet::new()
    };
    ($($key:tt : $value:expr),+ $(,)?) => {{
        let mut set = $crate::tokens::TokenSet::new();
        $(
            set.set($crate::__token_set_key!($key), $value);
        )+
        set
    }};
}

/// Turn a [`token_set!`] key into its string form.
///
/// Internal helper: bare identifiers are stringified, string literals pass
/// through unchanged.
#[doc(hidden)]
#[macro_export]
macro_rules! __token_set_key {
    ($key:ident) => {
        ::core::stringify!($key)
    };
    ($key:literal) => {
        $key
    };
}

/// Read a theme token as a live [`Prop`].
///
/// `token!(key)` uses the nearest installed [`Theme`]; `token!(theme, key)`
/// uses an explicit theme handle. A key is a bare identifier, which is
/// stringified, or a string literal. The value type is inferred from context,
/// so inside [`style!`](crate::style) the style macro supplies it; elsewhere,
/// annotate the binding or argument when inference is ambiguous.
///
/// ```
/// use mue_components::{token, token_set, tokens::Theme};
/// use mue_core::{FontSize, prelude::*};
/// use std::rc::Rc;
///
/// let owner = mue_core::scope::OwnedScope::new();
/// owner.run(|| {
///     let theme = Rc::new(Theme::new());
///     theme.register(&token_set! { title: FontSize::Px(28.) });
///     Theme::install(&theme);
///
///     let title: Prop<FontSize> = token!(title);
///     assert_eq!(title.get(), FontSize::Px(28.));
/// });
/// ```
#[macro_export]
macro_rules! token {
    ($theme:expr, $key:ident) => {
        ($theme).get(::core::stringify!($key))
    };
    ($theme:expr, $key:literal) => {
        ($theme).get($key)
    };
    ($key:ident) => {
        $crate::tokens::Theme::current()
            .expect("token! needs an installed theme")
            .get(::core::stringify!($key))
    };
    ($key:literal) => {
        $crate::tokens::Theme::current()
            .expect("token! needs an installed theme")
            .get($key)
    };
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

    /// Create signals for keys that are not registered yet, returning those keys.
    fn register(&mut self, set: &TokenSet<T>) -> Vec<Rc<str>> {
        let mut inserted = Vec::new();
        for (key, value) in set.iter() {
            if let Entry::Vacant(slot) = self.slots.entry(key.clone()) {
                slot.insert(Signal::new(value.clone()));
                inserted.push(key.clone());
            }
        }
        inserted
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
    registrations: RefCell<Vec<Registration>>,
}

/// One `(key, value type)` pair, kept for diagnostics.
struct Registration {
    key: Rc<str>,
    type_name: &'static str,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register every key in `set`. Keys that already exist keep their signal.
    pub fn register<T: Clone + PartialEq + 'static>(&self, set: &TokenSet<T>) {
        let inserted = self
            .stores
            .borrow_mut()
            .entry::<TokenStore<T>>()
            .or_insert_with(TokenStore::new)
            .register(set);
        if !inserted.is_empty() {
            self.registrations
                .borrow_mut()
                .extend(inserted.into_iter().map(|key| Registration {
                    key,
                    type_name: std::any::type_name::<T>(),
                }));
        }
    }

    /// Read a token as a live property.
    ///
    /// Panics when the key is not registered for this value type. The panic
    /// names the type, lists the keys registered for it, and reports other
    /// types that own the same key.
    pub fn get<T: Clone + PartialEq + 'static>(&self, key: &str) -> Prop<T> {
        self.try_get(key)
            .unwrap_or_else(|| panic!("{}", self.missing_token_message::<T>(key)))
    }

    fn missing_token_message<T: Clone + PartialEq + 'static>(&self, key: &str) -> String {
        let type_name = std::any::type_name::<T>();
        let mut registered = self.keys::<T>();
        registered.sort();
        let mut other_types: Vec<&'static str> = self
            .registrations
            .borrow()
            .iter()
            .filter(|registration| {
                registration.key.as_ref() == key && registration.type_name != type_name
            })
            .map(|registration| registration.type_name)
            .collect();
        other_types.sort_unstable();
        if other_types.is_empty() {
            format!(
                "token key {key:?} is not registered for {type_name}; registered keys for this type: {registered:?}"
            )
        } else {
            format!(
                "token key {key:?} is not registered for {type_name}; the same key is registered for: {other_types:?}; registered keys for this type: {registered:?}"
            )
        }
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
    fn missing_token_message_names_the_type_and_other_owners() {
        in_scope(|| {
            let theme = Theme::new();
            theme.register(&TokenSet::new().with("primary", 1_f32));
            theme.register(&TokenSet::new().with("gap", Size::Px(2.)));

            let message = theme.missing_token_message::<Size>("primary");
            assert!(message.contains("Size"), "{message}");
            assert!(message.contains("f32"), "{message}");
            assert!(
                message.contains("the same key is registered for"),
                "{message}"
            );
            assert!(message.contains("\"gap\""), "{message}");

            let message = theme.missing_token_message::<Size>("missing");
            assert!(message.contains("Size"), "{message}");
            assert!(message.contains("\"gap\""), "{message}");
            assert!(
                !message.contains("the same key is registered for"),
                "{message}"
            );
        });
    }

    #[test]
    #[should_panic(expected = "the same key is registered for")]
    fn reading_a_key_under_the_wrong_type_names_the_other_type() {
        in_scope(|| {
            let theme = Theme::new();
            theme.register(&TokenSet::new().with("primary", 1_f32));
            let _ = theme.get::<Size>("primary");
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
    fn token_set_macro_accepts_bare_and_quoted_keys() {
        let set: TokenSet<f32> = token_set! {
            gap: 8.,
            "body.opacity": 1.,
            gap: 9.,
        };
        assert_eq!(set.get("gap"), Some(&9.), "later entries win");
        assert_eq!(set.get("body.opacity"), Some(&1.));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn token_set_macro_handles_empty_and_trailing_comma() {
        let empty: TokenSet<f32> = token_set! {};
        assert!(empty.is_empty());
        let one: TokenSet<f32> = token_set! { gap: 8., };
        assert_eq!(one.get("gap"), Some(&8.));
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
