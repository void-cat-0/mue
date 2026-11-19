//! Optional reactive token store for application-defined values.
//!
//! A [`TokenSet`] holds named values of one type. Build one with
//! [`token_set!`](crate::token_set) for literal keys or [`TokenSet::with`] for a
//! runtime key. Register keys once with [`Theme::register`]; lookup returns a
//! live [`Prop`] bound to a signal. Applying another set updates those signals
//! in place.
//!
//! The store supplies values through ordinary properties. Applications can also
//! define their own token types and share them through
//! [`provide`](mue_core::scope::provide) and [`inject`].
//!
//! ```
//! use mue_components::{token_set, tokens::Theme};
//! use mue_core::prelude::*;
//!
//! let owner = mue_core::scope::OwnedScope::new();
//! owner.run(|| {
//!     let theme = Theme::new();
//!     theme.register(&token_set! { gap: 8_f32 });
//!     let gap: Prop<f32> = theme.get("gap");
//!     theme.apply(&token_set! { gap: 16_f32 });
//!     assert_eq!(gap.get(), 16.);
//! });
//! ```

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
/// `token!(key)` uses the nearest installed [`Theme`]. Pass an explicit theme
/// with `token!(theme, key)`. Keys are identifiers or string literals.
/// [`style!`](crate::style) supplies the property type for direct token values
/// and rewrites unqualified `token!(...)` calls in supported braced expressions.
/// Elsewhere, annotate the binding if Rust cannot infer the type.
///
/// Resolve handles while the theme scope is active. Capture those handles in
/// deferred computations so later evaluations can read the same signals.
///
/// ```
/// use mue_components::{style, token, token_set, tokens::Theme};
/// use mue_core::{FontSize, prelude::*};
/// use std::rc::Rc;
///
/// let owner = mue_core::scope::OwnedScope::new();
/// owner.run(|| {
///     let theme = Rc::new(Theme::new());
///     theme.register(&token_set! { title: FontSize::Px(28.), body: FontSize::Px(18.) });
///     Theme::install(&theme);
///
///     let title: Prop<FontSize> = token!(title);
///     let emphasized = Prop::from(signal(true));
///     let declarations = style! {
///         font-size: {emphasized.pick(token!(theme, title), token!(theme, body))};
///     };
///     let bound_size = declarations.declared_font_size().unwrap();
///     assert_eq!(title.get(), FontSize::Px(28.));
///
///     theme.apply(&token_set! { title: FontSize::Px(32.) });
///     assert_eq!(bound_size.get(), FontSize::Px(32.));
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
/// The same name can hold values of different types. Use [`Self::register`] in a
/// scope that outlives every consumer of the registered signals. Reading gives
/// a [`Prop`] bound to its signal, so applying a set updates existing bindings.
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

    /// Register new keys of type `T`. Existing keys keep their values and signals.
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
    /// # Panics
    ///
    /// If the key is missing for `T`. The diagnostic lists the available keys
    /// for `T` and other types that have registered this key. For optional
    /// lookups, use [`Self::try_get`].
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
    /// This updates registered keys of type `T` present in `set`. Unregistered
    /// keys are skipped. Equal values do not notify consumers.
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
/// When converted to a node, the provider runs `view` in an inherited scope and
/// retains that scope on the resulting node. Create token signals with
/// [`Theme::register`] in a scope that outlives the view. For an inherited text
/// foreground, use [`mue_macroquad::inherit::provide_foreground`].
pub fn provide_theme<T>(
    theme: Rc<Theme>,
    view: impl FnOnce() -> T,
) -> ThemeProvider<impl FnOnce() -> T> {
    ThemeProvider { theme, view }
}

/// Deferred subtree produced by [`provide_theme`].
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
