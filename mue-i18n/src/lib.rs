//! Optional Fluent localization for mue applications.
//!
//! # Build a catalog and select a language
//!
//! Register complete Fluent (FTL) resources with [`CatalogBuilder`] before
//! constructing localized views. A (locale, namespace) pair can contain
//! several resources. Different namespaces may reuse message IDs. Every
//! namespace needs an [`en-US`](FALLBACK_LOCALE) resource, but other languages
//! may omit messages or entire namespaces. File loading, directory layout,
//! locale preferences, and system-language negotiation belong to the app.
//! Pass a source string or use `include_str!` to embed a resource.
//!
//! [`I18n`] owns the language-changing capability. [`I18n::set_locale`] accepts
//! exact identifiers from [`I18n::available_locales`] and preserves the current
//! locale on error. The initial locale may be any registered identifier.
//! Each cloneable [`Translator`] selects a namespace and keeps the catalog and
//! selector alive, even after the [`I18n`] handle is dropped. You can pass a
//! translator explicitly or share it through [mue scope context](mue_core::scope)
//! using [`provide`](mue_core::scope::provide) and [`inject`](mue_core::scope::inject).
//! Context lookup follows the scope chain. The catalog and service are confined
//! to the reactive runtime's thread.
//!
//! ```
//! use mue_core::{OwnedScope, signal::{Access, signal}};
//! use mue_i18n::{CatalogBuilder, I18n, fluent_args, langid, tl};
//!
//! let mut builder = CatalogBuilder::new();
//! builder.add_ftl(
//!     langid!("en-US"),
//!     "app",
//!     "greeting = Hello, { $name }\nitems = { $count } items\n",
//! )?;
//! builder.add_ftl(langid!("fr"), "app", "greeting = Bonjour, { $name }\n")?;
//! let i18n = I18n::new(builder.build()?, langid!("fr"))?;
//! let app = i18n.translator("app")?;
//!
//! let fallback = app.format("items", Some(&fluent_args!["count" => 2]));
//! // Fluent adds bidirectional isolation marks around interpolated values.
//! assert_eq!(fallback.text.as_ref(), "\u{2068}2\u{2069} items");
//! assert_eq!(fallback.resolved_locale, Some(langid!("en-US")));
//!
//! let owner = OwnedScope::new();
//! let (greeting, name) = owner.run(|| {
//!     let name = signal(String::from("Ada"));
//!     let greeting = tl!(app, "greeting", "name" => name.get_clone());
//!     (greeting, name)
//! });
//! assert_eq!(greeting.get_clone().as_ref(), "Bonjour, \u{2068}Ada\u{2069}");
//! i18n.set_locale(langid!("en-US"))?;
//! assert_eq!(greeting.get_clone().as_ref(), "Hello, \u{2068}Ada\u{2069}");
//! name.set(String::from("Grace"));
//! assert_eq!(greeting.get_clone().as_ref(), "Hello, \u{2068}Grace\u{2069}");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Snapshots and reactive text
//!
//! [`Translator::format`] returns an owned, untracked [`Translation`] snapshot.
//! [`Translator::text`] handles messages without parameters and
//! [`Translator::text_with`] or [`tl!`] binds parameterized text. Use
//! [`Translator::bind`] when the UI also needs reactive diagnostics. Create
//! bindings and any derived projections inside a live, non-null consumer
//! scope, such as a view's [`OwnedScope::run`](mue_core::scope::Scope::run).
//! A builder argument is evaluated immediately during configuration, so the
//! scope must already be installed when you create the binding.
//!
//! Parameter providers build [`FluentArgs<'static>`](FluentArgs) from tracked
//! reads. Return owned dynamic strings and keep numeric values as numbers so
//! Fluent can select plural variants. The parameter sources must outlive the
//! binding. Providers should only read tracked values and build arguments.
//! Locale and parameter changes can update the current view in place. The
//! host manages view lifetimes, fonts, and layout direction. Fluent's
//! bidirectional isolation remains enabled.
//!
//! # Lookup and diagnostics
//!
//! Use `message-id` or `message-id.attribute` keys in the translator's
//! namespace. Fluent terms are internal to the FTL. Only a missing message,
//! value, or attribute advances lookup from the selected language to English.
//! Each pattern resolves within its own bundle, using that locale's plural
//! rules and entries. Formatting problems retain the chosen pattern's
//! best-effort text and diagnostics. If no pattern is formatted, the original
//! key is returned with no resolved locale. A successful English fallback
//! records English as the resolved locale.
//!
//! Resource and configuration errors use [`CatalogError`] and [`I18nError`].
//! [`Translator::format`] and [`Translator::bind`] expose [`TranslationIssue`]
//! values through [`Translation::issues`]. The app decides which issues to report.
//! Fluent can leave some invalid expressions undiagnosed.
//!
//! # Numeric arguments and `NUMBER`
//!
//! Fluent arguments can be strings or checked numbers. Numbers must be finite,
//! at most `2^53 - 1` in magnitude, and representable with up to 18 decimal
//! fraction digits. Fraction digit options use `0..=18`, while minimum integer
//! and minimum/maximum significant digit options use `1..=21`. If both ends
//! of a range are supplied, the minimum may not exceed the maximum.
//!
//! [`CatalogBuilder::add_ftl`] checks numeric resource literals and the static
//! shape of `NUMBER` calls. Each call takes one numeric literal, variable, or
//! nested `NUMBER` call, with literal named options. Arguments and merged
//! options are also checked at runtime. Invalid inputs can be inspected through
//! [`TranslationIssue::InvalidArgument`].
//!
//! - `type: "cardinal"` or `type: "ordinal"` selects the bundle's plural rules.
//! - `minimumFractionDigits` pads the visible fraction with zeros and can change
//!   the plural category. A zero minimum is normalized to Fluent's default.
//! - The other digit options and `useGrouping`, `style`, `currency`, and
//!   `currencyDisplay` are validated. Rendering leaves the number unrounded
//!   and applies only the fraction padding described above. These options
//!   can still affect exact numeric variant matching because Fluent includes
//!   the number's options in equality checks.
//!
//! Applications that need full regional number, currency, or date formatting
//! should format display strings separately and keep numeric selector inputs.

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
/// `tl!(tr, "title")` calls [`Translator::text`]. Named arguments use a `move`
/// provider passed to [`Translator::text_with`] and are evaluated once for each
/// formatting evaluation. The translator and key expressions are evaluated
/// once when the binding is created.
///
/// Create bindings in a live consumer scope and pass owned dynamic strings,
/// for example `tl!(tr, "greeting", "name" => name.get_clone().to_string())`.
/// Tracked reads remain reactive. Keys and arguments are checked at runtime.
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
