use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fluent::{FluentBundle, FluentResource, FluentValue};
use unic_langid::{LanguageIdentifier, langid};

use crate::error::{CatalogError, TranslationIssue};
use crate::number::{safe_number, validate_resource_numbers};

type Resources = BTreeMap<LanguageIdentifier, BTreeMap<Rc<str>, Vec<Rc<FluentResource>>>>;
type Bundles = BTreeMap<LanguageIdentifier, BTreeMap<Rc<str>, BundleRecord>>;

struct BundleRecord {
    bundle: FluentBundle<Rc<FluentResource>>,
    // Fluent requires function closures to be Send + Sync even on its ordinary
    // thread-local bundle. The private buffer contains only static reasons; it
    // does not change the catalog's Rc/memoizer thread affinity.
    number_issues: Arc<Mutex<Vec<&'static str>>>,
}

/// Collects complete FTL resources before publishing an immutable catalog.
///
/// Multiple resources may belong to one (locale, namespace). Duplicate message
/// or term identifiers are rejected at build time, including duplicates inside
/// a single source. Parser errors never retain the parser's recovered resource.
#[derive(Debug, Default)]
pub struct CatalogBuilder {
    resources: Resources,
}

impl CatalogBuilder {
    /// Create an empty builder, equivalent to Default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and validate a complete resource without changing published data.
    ///
    /// NUMBER accepts numeric literals, variables or nested NUMBER calls,
    /// with literal named options; values/digit options must satisfy the safe
    /// range documented by this crate. This is stricter than arbitrary valid FTL.
    ///
    /// An error leaves this builder unchanged. Successfully parsed duplicate
    /// registrations are retained only until build rejects them; they never
    /// override entries in a published catalog.
    pub fn add_ftl(
        &mut self,
        locale: LanguageIdentifier,
        namespace: impl Into<Rc<str>>,
        source: impl Into<String>,
    ) -> Result<(), CatalogError> {
        let namespace = namespace.into();
        if namespace.is_empty() {
            return Err(CatalogError::InvalidResource {
                locale,
                namespace: Arc::from(namespace.as_ref()),
                entry: Arc::from("<resource>"),
                reason: Arc::from("namespace must not be empty"),
            });
        }
        let resource =
            FluentResource::try_new(source.into()).map_err(|(_, errors)| CatalogError::Parse {
                locale: locale.clone(),
                namespace: Arc::from(namespace.as_ref()),
                errors,
            })?;
        validate_resource_numbers(&resource).map_err(|(entry, reason)| {
            CatalogError::InvalidResource {
                locale: locale.clone(),
                namespace: Arc::from(namespace.as_ref()),
                entry: Arc::from(entry.as_ref()),
                reason: Arc::from(reason.as_ref()),
            }
        })?;
        self.resources
            .entry(locale)
            .or_default()
            .entry(namespace)
            .or_default()
            .push(Rc::new(resource));
        Ok(())
    }

    /// Build all bundles atomically. Empty builders are rejected, and every
    /// namespace must have an en-US resource.
    ///
    /// Each bundle has exactly its own locale; fallback is a later lookup in a
    /// different bundle, never a mixed locale list. Bidirectional isolation
    /// remains Fluent's default. Only a private, checked NUMBER is registered;
    /// no user functions or general number/date/currency formatter are installed.
    pub fn build(self) -> Result<Catalog, CatalogError> {
        if self.resources.is_empty() {
            return Err(CatalogError::EmptyCatalog);
        }
        let fallback_locale = langid!("en-US");
        let namespaces: BTreeSet<Rc<str>> = self
            .resources
            .values()
            .flat_map(|resources| resources.keys().cloned())
            .collect();
        for namespace in &namespaces {
            if !self
                .resources
                .get(&fallback_locale)
                .is_some_and(|resources| resources.contains_key(namespace.as_ref()))
            {
                return Err(CatalogError::MissingFallbackResource {
                    locale: fallback_locale.clone(),
                    namespace: Arc::from(namespace.as_ref()),
                });
            }
        }

        let locales = self.resources.keys().cloned().collect();
        let mut bundles: Bundles = BTreeMap::new();
        for (locale, resources) in self.resources {
            let mut locale_bundles = BTreeMap::new();
            for (namespace, resources) in resources {
                let mut bundle = FluentBundle::new(vec![locale.clone()]);
                let number_issues = Arc::new(Mutex::new(Vec::new()));
                let function_issues = Arc::clone(&number_issues);
                bundle
                    .add_function("NUMBER", move |positional, named| {
                        match safe_number(positional, named) {
                            Ok(number) => FluentValue::Number(number),
                            Err(reason) => {
                                function_issues
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .push(reason);
                                FluentValue::Error
                            }
                        }
                    })
                    .map_err(|error| CatalogError::Fluent {
                        locale: locale.clone(),
                        namespace: Arc::from(namespace.as_ref()),
                        errors: vec![error],
                    })?;
                for resource in resources {
                    bundle
                        .add_resource(resource)
                        .map_err(|errors| CatalogError::Fluent {
                            locale: locale.clone(),
                            namespace: Arc::from(namespace.as_ref()),
                            errors,
                        })?;
                }
                locale_bundles.insert(
                    namespace,
                    BundleRecord {
                        bundle,
                        number_issues,
                    },
                );
            }
            bundles.insert(locale, locale_bundles);
        }
        Ok(Catalog {
            bundles,
            locales,
            namespaces,
            fallback_locale,
        })
    }
}

/// Immutable, isolated Fluent bundles indexed by exact locale and namespace.
///
/// All resources are assembled successfully before a Catalog can be obtained.
/// Fluent's ordinary memoizer and Rc resources keep this catalog thread-local.
/// NUMBER accepts finite safe-range numbers and bounded digit options; it is
/// not a promise of full localized number, date or currency formatting.
pub struct Catalog {
    bundles: Bundles,
    locales: Vec<LanguageIdentifier>,
    namespaces: BTreeSet<Rc<str>>,
    fallback_locale: LanguageIdentifier,
}

impl Catalog {
    pub(crate) fn locales(&self) -> &[LanguageIdentifier] {
        &self.locales
    }

    pub(crate) fn has_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains(namespace)
    }

    pub(crate) fn bundle(
        &self,
        locale: &LanguageIdentifier,
        namespace: &str,
    ) -> Option<&FluentBundle<Rc<FluentResource>>> {
        Some(&self.bundles.get(locale)?.get(namespace)?.bundle)
    }

    /// Drain private NUMBER diagnostics before/after one synchronous format.
    pub(crate) fn take_number_issues(
        &self,
        locale: &LanguageIdentifier,
        namespace: &str,
    ) -> Vec<TranslationIssue> {
        let Some(record) = self
            .bundles
            .get(locale)
            .and_then(|bundles| bundles.get(namespace))
        else {
            return Vec::new();
        };
        let mut reasons = record
            .number_issues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *reasons)
            .into_iter()
            .map(|reason| TranslationIssue::InvalidArgument {
                name: Arc::from("NUMBER"),
                reason: Arc::from(reason),
            })
            .collect()
    }

    pub(crate) fn fallback_locale(&self) -> &LanguageIdentifier {
        &self.fallback_locale
    }
}
