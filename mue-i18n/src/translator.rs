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
/// A fallback-language pattern is a normal successful lookup. `None` means no
/// pattern was formatted (invalid key/arguments or a missing message). Empty
/// text is not a missing pattern. An empty `issues` list is not a proof that
/// every Fluent function or expression was semantically valid.
#[derive(Clone, Debug, PartialEq)]
pub struct Translation {
    pub text: Rc<str>,
    pub resolved_locale: Option<LanguageIdentifier>,
    pub issues: Rc<[TranslationIssue]>,
}

impl Translation {
    fn unavailable(key: &str, issues: Vec<TranslationIssue>) -> Self {
        Self {
            text: key.into(),
            resolved_locale: None,
            issues: issues.into(),
        }
    }
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
/// This write capability is deliberately not Clone. A read-only Translator
/// shares ownership of the entire service, including its reactive scope.
/// Dropping I18n therefore does not invalidate remaining Translators. The
/// service is released after its last owner; bindings themselves still belong
/// to the consumer scopes in which they were created.
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
/// It tracks the service current language, not a frozen bundle. It may be
/// explicitly provided through ordinary mue scope context; no global or TLS
/// translator is installed. Strong ownership keeps service resources alive,
/// not arbitrary parameter signals or the consumer scope.
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
        self.format_in(&locale, key, args)
    }

    /// Immediately bind a parameter-free text to the current language.
    #[track_caller]
    pub fn text(&self, key: impl Into<Rc<str>>) -> Prop<Rc<str>> {
        self.text_with(key, FluentArgs::new)
    }

    /// Immediately bind text to the language and tracked parameter reads.
    ///
    /// Call inside a live non-null consumer scope. Each provider evaluation
    /// builds owned arguments once, even when fallback is needed. It may read
    /// Signal/Prop values but must not mutate state or create reactive resources.
    /// The captured parameter sources must outlive this binding.
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
            translator.format_in(&locale, &key, Some(&args)).text
        }))
    }

    /// Bind the complete result when the UI or application needs diagnostics.
    ///
    /// This has the same scope/provider contract as text_with. Project its text
    /// with `result.map(|result| result.text)` in the same consumer scope.
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
            translator.format_in(&locale, &key, Some(&args))
        }))
    }

    fn format_in(
        &self,
        selected: &LanguageIdentifier,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Translation {
        let Some((id, attribute)) = parse_key(key) else {
            return Translation::unavailable(key, vec![TranslationIssue::InvalidKey]);
        };
        if let Some(args) = args {
            let issues = validate_arguments(args);
            if !issues.is_empty() {
                return Translation::unavailable(key, issues);
            }
        }

        let normalized = args.and_then(normalized_arguments);
        let args = normalized.as_ref().or(args);
        let fallback = self.state.catalog.fallback_locale();
        let mut missing = TranslationIssue::MissingMessage;
        for locale in std::iter::once(selected).chain((selected != fallback).then_some(fallback)) {
            let Some(bundle) = self.state.catalog.bundle(locale, &self.namespace) else {
                continue;
            };
            let Some(message) = bundle.get_message(id) else {
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
            // NUMBER is the only installed function. Its private diagnostics
            // supplement Fluent errors, which do not report every function failure.
            self.state
                .catalog
                .take_number_issues(locale, &self.namespace);
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, args, &mut errors);
            let mut issues = self
                .state
                .catalog
                .take_number_issues(locale, &self.namespace);
            issues.extend(errors.into_iter().map(TranslationIssue::Fluent));
            return Translation {
                text: Rc::from(text.as_ref()),
                resolved_locale: Some(locale.clone()),
                issues: issues.into(),
            };
        }
        Translation::unavailable(key, vec![missing])
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
