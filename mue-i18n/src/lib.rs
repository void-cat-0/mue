//! Optional Fluent localization for mue applications.
//!
//! Resources belong to an immutable [`Catalog`]. An [`I18n`] controls the
//! current language, while a cloneable, read-only [`Translator`] keeps that
//! service alive and selects one namespace. Missing messages fall back to
//! English (`en-US`); formatting diagnostics are returned, never logged.
//!
//! [`Translator::format`] is a snapshot. [`Translator::text`],
//! [`Translator::text_with`], [`Translator::bind`] and [`tl!`] immediately
//! create reactive bindings in the current **live, non-null consumer scope**.
//! They are not lazy builder expressions. Parameter providers only read
//! signals/properties and assemble owned Fluent arguments; they must not
//! create reactive resources or mutate application state.
//!
//! This crate does not initialize fonts, detect the system language, read
//! files, alter Scene scheduling or install a global localization service.
//! Catalogs and services using the ordinary Fluent bundle are thread-local.

mod catalog;
mod error;
mod number;
mod translator;

pub use catalog::{Catalog, CatalogBuilder};
pub use error::{CatalogError, I18nError, TranslationIssue};
pub use fluent::{FluentArgs, fluent_args};
pub use translator::{I18n, Translation, Translator};
pub use unic_langid::{LanguageIdentifier, langid};

/// The default missing-message fallback, independent of the initial language.
pub const FALLBACK_LOCALE: &str = "en-US";

/// Bind a translated UI string using an explicit namespace translator.
///
/// `tl!(tr, "title")` expands to `tr.text("title")`. Named arguments expand
/// to a `move` provider passed to `text_with`, and are evaluated once per
/// formatting evaluation, not snapshotted at the macro call. The translator
/// and key expressions are evaluated once when the binding is created.
///
/// Dynamic strings must become owned arguments, for example
/// `tl!(tr, "greeting", "name" => name.get_clone().to_string())`. Values read
/// through tracked Signal/Prop getters remain reactive. This is a convenience
/// macro, not compile-time validation of keys against FTL resources. It has
/// the same consumer-scope requirements as `Translator::text_with`.
#[macro_export]
macro_rules! tl {
    ($translator:expr, $key:expr $(,)?) => {
        ($translator).text($key)
    };
    ($translator:expr, $key:expr, $($name:expr => $value:expr),+ $(,)?) => {
        ($translator).text_with($key, move || {
            let mut args = $crate::FluentArgs::new();
            $(args.set($name, $value);)+
            args
        })
    };
}
