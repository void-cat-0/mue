//! Locale selection, namespace translators, snapshots, and reactive bindings.
//!
//! Bindings are created immediately in the consumer's live scope and track both
//! locale and parameter reads.

use std::rc::Rc;

use fluent::FluentArgs;
use mue_core::{
    effect::computed,
    prop::Prop,
    scope::{OwnedScope, current_scope},
    signal::{Access, ReadSignal, Signal},
};
use unic_langid::LanguageIdentifier;

use crate::{
    Catalog, I18nError, TranslationIssue,
    number::{normalized_arguments, validate_arguments},
};

/// Owned text, the language which supplied its pattern, and collected diagnostics.
///
/// A fallback-language pattern is a successful lookup. Even an empty string
/// can come from a pattern. Fluent can leave some invalid expressions undiagnosed.
#[derive(Clone, Debug, PartialEq)]
pub struct Translation {
    /// The formatted text, or the original key if no pattern was formatted.
    pub text: Rc<str>,
    /// The pattern's language. `None` indicates that no pattern was formatted.
    pub resolved_locale: Option<LanguageIdentifier>,
    /// Lookup and formatting diagnostics for this key.
    pub issues: Rc<[TranslationIssue]>,
}

struct State {
    catalog: Catalog,
    locale: Signal<LanguageIdentifier>,
    // This private detached scope owns only service state. Never install a
    // Translator or a callback capturing State here: that would form an Rc cycle.
    _owner: OwnedScope,
}

/// Application-owned language control.
///
/// Keep this handle wherever the application changes languages. Each
/// [`Translator`] shares ownership of the service and its private reactive
/// scope. The last owner releases the service. Bindings belong to the
/// consumer scopes in which they were created.
pub struct I18n {
    state: Rc<State>,
}

impl I18n {
    /// Start with an explicitly selected, registered language.
    ///
    /// The initial language need not be English. No system language detection
    /// or regional/script negotiation occurs. This and all other methods run
    /// on the thread owning the mue reactive runtime.
    pub fn new(catalog: Catalog, locale: LanguageIdentifier) -> Result<Self, I18nError> {
        if !catalog.locales().contains(&locale) {
            return Err(I18nError::UnsupportedLocale(locale));
        }
        let owner = OwnedScope::new();
        let selector = owner.run(|| Signal::new(locale));
        Ok(Self {
            state: Rc::new(State {
                catalog,
                locale: selector,
                _owner: owner,
            }),
        })
    }

    /// Select an exact registered language; failure leaves the old selection intact.
    ///
    /// Catalog resources are immutable and already built, so one selector
    /// update also selects the corresponding resources atomically. Existing
    /// Translators and bindings remain connected; Scenes/Nodes are not rebuilt.
    pub fn set_locale(&self, locale: LanguageIdentifier) -> Result<(), I18nError> {
        if !self.state.catalog.locales().contains(&locale) {
            return Err(I18nError::UnsupportedLocale(locale));
        }
        self.state.locale.set(locale);
        Ok(())
    }

    /// A read-only reactive selector. Copying it alone does not keep I18n alive.
    pub fn locale(&self) -> ReadSignal<LanguageIdentifier> {
        *self.state.locale
    }

    /// Exact supported identifiers, in deterministic order.
    ///
    /// A language may omit a namespace: that namespace uses its English bundle.
    pub fn available_locales(&self) -> &[LanguageIdentifier] {
        self.state.catalog.locales()
    }

    /// Get a strong, read-only handle for one registered namespace.
    pub fn translator(&self, namespace: &str) -> Result<Translator, I18nError> {
        if !self.state.catalog.has_namespace(namespace) {
            return Err(I18nError::UnknownNamespace(namespace.into()));
        }
        Ok(Translator {
            state: self.state.clone(),
            namespace: namespace.into(),
        })
    }
}

/// An owning, cloneable, read-only translator for one namespace.
///
/// Uses the service's current language and can be shared through ordinary
/// mue scope context. Its ownership covers the catalog and language selector.
/// Keep consumer scopes and parameter sources alive separately.
#[derive(Clone)]
pub struct Translator {
    state: Rc<State>,
    namespace: Rc<str>,
}

impl Translator {
    /// Format a one-time, untracked snapshot using borrowed Fluent arguments.
    ///
    /// Keys are `message-id` or `message-id.attribute`. Bare terms, malformed
    /// identifiers and multiple dots are invalid. Missing message/value/attribute
    /// looks in the selected locale and then en-US, never mixing bundles.
    /// Resolver problems preserve the chosen pattern and best-effort text;
    /// they do not silently change languages. All output is owned.
    pub fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> Translation {
        let locale = self.state.locale.get_clone_untracked();
        self.translation_in(&locale, key, args)
    }

    /// Bind a message without parameters to the current language.
    ///
    /// See [`Self::text_with`] for the consumer scope requirement.
    #[track_caller]
    pub fn text(&self, key: impl Into<Rc<str>>) -> Prop<Rc<str>> {
        self.text_with(key, FluentArgs::new)
    }

    /// Immediately bind text to the language and tracked parameter reads.
    ///
    /// The provider builds owned arguments once per evaluation, including for
    /// fallback. It may read tracked signals and properties. Keep those
    /// sources alive for the lifetime of the binding. Locale and parameter
    /// reads stay tracked even on lookup or argument errors. Dependents are
    /// notified when the resulting text changes.
    ///
    /// # Panics
    ///
    /// Panics outside a live, non-null consumer scope.
    #[track_caller]
    pub fn text_with(
        &self,
        key: impl Into<Rc<str>>,
        args: impl Fn() -> FluentArgs<'static> + 'static,
    ) -> Prop<Rc<str>> {
        assert_consumer_scope();
        let translator = self.clone();
        let key = key.into();
        Prop::from(computed(move |_| {
            // Track even failed lookups/arguments so a subsequent language or
            // parameter correction updates the same binding.
            let locale = translator.state.locale.get_clone();
            let args = args();
            translator.format_in(&locale, &key, Some(&args), None).0
        }))
    }

    /// Bind the complete result when the UI or application needs diagnostics.
    ///
    /// Uses the same scope and provider contract as [`Self::text_with`].
    /// Project the text with `result.map(|result| result.text)` in the same
    /// consumer scope.
    ///
    /// ```
    /// use mue_core::{OwnedScope, signal::{Access, signal}};
    /// use mue_i18n::{CatalogBuilder, I18n, TranslationIssue, fluent_args, langid};
    ///
    /// let mut catalog = CatalogBuilder::new();
    /// catalog.add_ftl(langid!("en-US"), "app", "total = { NUMBER($count) }\n")?;
    /// let i18n = I18n::new(catalog.build()?, langid!("en-US"))?;
    /// let app = i18n.translator("app")?;
    /// let owner = OwnedScope::new();
    /// let (count, result, text) = owner.run(|| {
    ///     let count = signal(f64::NAN);
    ///     let result = app.bind("total", move || fluent_args!["count" => count.get()]);
    ///     let text = result.map(|result| result.text);
    ///     (count, result, text)
    /// });
    /// assert_eq!(text.get_clone().as_ref(), "total");
    /// assert!(matches!(result.get_clone().issues.as_ref(),
    ///     [TranslationIssue::InvalidArgument { .. }]));
    ///
    /// count.set(7.0);
    /// assert_eq!(text.get_clone().as_ref(), "7");
    /// assert!(result.get_clone().issues.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[track_caller]
    pub fn bind(
        &self,
        key: impl Into<Rc<str>>,
        args: impl Fn() -> FluentArgs<'static> + 'static,
    ) -> Prop<Translation> {
        assert_consumer_scope();
        let translator = self.clone();
        let key = key.into();
        Prop::from(computed(move |_| {
            let locale = translator.state.locale.get_clone();
            let args = args();
            translator.translation_in(&locale, &key, Some(&args))
        }))
    }

    fn translation_in(
        &self,
        selected: &LanguageIdentifier,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Translation {
        let mut issues = Vec::new();
        let (text, resolved_locale) = self.format_in(selected, key, args, Some(&mut issues));
        Translation {
            text,
            resolved_locale: resolved_locale.cloned(),
            issues: issues.into(),
        }
    }

    /// One formatting path for snapshots and bindings. Text-only bindings omit
    /// diagnostic packaging and the resolved-locale clone, not input validation.
    fn format_in<'a>(
        &'a self,
        selected: &'a LanguageIdentifier,
        key: &str,
        args: Option<&FluentArgs<'_>>,
        mut issues: Option<&mut Vec<TranslationIssue>>,
    ) -> (Rc<str>, Option<&'a LanguageIdentifier>) {
        let Some((id, attribute)) = parse_key(key) else {
            if let Some(issues) = issues {
                issues.push(TranslationIssue::InvalidKey);
            }
            return (key.into(), None);
        };
        if let Some(args) = args
            && !validate_arguments(args, issues.as_deref_mut())
        {
            return (key.into(), None);
        }

        let normalized = args.and_then(normalized_arguments);
        let args = normalized.as_ref().or(args);
        let fallback = self.state.catalog.fallback_locale();
        let mut missing = TranslationIssue::MissingMessage;
        for locale in std::iter::once(selected).chain((selected != fallback).then_some(fallback)) {
            let Some(record) = self.state.catalog.bundle_record(locale, &self.namespace) else {
                continue;
            };
            let Some(message) = record.bundle.get_message(id) else {
                continue;
            };
            let pattern = if let Some(attribute) = attribute {
                let Some(attribute) = message.get_attribute(attribute) else {
                    missing = TranslationIssue::MissingAttribute;
                    continue;
                };
                attribute.value()
            } else {
                let Some(pattern) = message.value() else {
                    missing = TranslationIssue::MissingValue;
                    continue;
                };
                pattern
            };
            let text = record.format_pattern(pattern, args, issues);
            return (text, Some(locale));
        }
        if let Some(issues) = issues {
            issues.push(missing);
        }
        (key.into(), None)
    }
}

#[track_caller]
fn assert_consumer_scope() {
    assert!(
        current_scope().is_alive(),
        "translation bindings require a live, non-null consumer scope"
    );
}

fn parse_key(key: &str) -> Option<(&str, Option<&str>)> {
    let (id, attribute) = match key.split_once('.') {
        Some((id, attribute)) => (id, Some(attribute)),
        None => (key, None),
    };
    if !valid_identifier(id) || attribute.is_some_and(|value| !valid_identifier(value)) {
        return None;
    }
    Some((id, attribute))
}

fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
