//! Errors from catalog construction and locale/namespace selection, plus
//! per-translation diagnostics.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use fluent::FluentError;
use fluent_syntax::parser::ParserError;
use unic_langid::LanguageIdentifier;

/// A resource could not be accepted, or a complete catalog could not be built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// A catalog needs at least one complete resource.
    EmptyCatalog,
    /// The entire source was rejected; the parser's recovered resource is not kept.
    Parse {
        locale: LanguageIdentifier,
        namespace: Arc<str>,
        errors: Vec<ParserError>,
    },
    /// Fluent rejected a resource or the private NUMBER function registration.
    /// No partially assembled catalog is returned.
    Fluent {
        locale: LanguageIdentifier,
        namespace: Arc<str>,
        errors: Vec<FluentError>,
    },
    /// Every registered namespace needs at least one en-US resource.
    MissingFallbackResource {
        locale: LanguageIdentifier,
        namespace: Arc<str>,
    },
    /// The namespace is empty or the resource violates the numeric or `NUMBER`
    /// validation rules.
    InvalidResource {
        locale: LanguageIdentifier,
        namespace: Arc<str>,
        entry: Arc<str>,
        reason: Arc<str>,
    },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCatalog => f.write_str("catalog has no resources"),
            Self::Parse {
                locale,
                namespace,
                errors,
            } => {
                write!(f, "cannot parse {locale}/{namespace}")?;
                for error in errors {
                    write!(f, ": {error} at bytes {:?}", error.pos)?;
                }
                Ok(())
            }
            Self::Fluent {
                locale,
                namespace,
                errors,
            } => {
                write!(f, "cannot build {locale}/{namespace}")?;
                for error in errors {
                    write!(f, ": {error}")?;
                }
                Ok(())
            }
            Self::MissingFallbackResource { locale, namespace } => {
                write!(f, "namespace {namespace} has no {locale} fallback resource")
            }
            Self::InvalidResource {
                locale,
                namespace,
                entry,
                reason,
            } => write!(
                f,
                "invalid resource {locale}/{namespace}, entry {entry}: {reason}"
            ),
        }
    }
}

impl Error for CatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse { errors, .. } => errors.first().map(|error| error as &dyn Error),
            Self::Fluent { errors, .. } => errors.first().map(|error| error as &dyn Error),
            _ => None,
        }
    }
}

/// A requested language or namespace is not registered in the catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum I18nError {
    UnsupportedLocale(LanguageIdentifier),
    UnknownNamespace(Arc<str>),
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLocale(locale) => write!(f, "unsupported locale: {locale}"),
            Self::UnknownNamespace(namespace) => write!(f, "unknown namespace: {namespace}"),
        }
    }
}

impl Error for I18nError {}

/// A diagnostic associated with a Translation's already-known key.
///
/// Fluent does not diagnose every invalid function invocation. An empty issue
/// list is therefore not a general semantic-validation guarantee.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranslationIssue {
    InvalidKey,
    MissingMessage,
    MissingValue,
    MissingAttribute,
    InvalidArgument { name: Arc<str>, reason: Arc<str> },
    Fluent(FluentError),
}

impl fmt::Display for TranslationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("invalid translation key"),
            Self::MissingMessage => f.write_str("missing message"),
            Self::MissingValue => f.write_str("missing message value"),
            Self::MissingAttribute => f.write_str("missing message attribute"),
            Self::InvalidArgument { name, reason } => {
                write!(f, "invalid argument {name}: {reason}")
            }
            Self::Fluent(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl Error for TranslationIssue {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Fluent(error) => Some(error),
            _ => None,
        }
    }
}
