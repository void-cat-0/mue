//! Application-owned localization, with only strong read-only translators given to UI.

use crate::application::Result;
use mue_core::signal::ReadSignal;
use mue_i18n::{CatalogBuilder, I18n, LanguageIdentifier, Translator, langid};
use std::rc::Rc;

pub fn load_i18n() -> Result<Rc<I18n>> {
    // CatalogBuilder fixes the fallback to en-US. No runtime file reads or downloads.
    let mut catalog = CatalogBuilder::new();
    for (locale, namespace, source) in [
        (
            langid!("en-US"),
            "common",
            include_str!("../locales/en-US/common.ftl"),
        ),
        (
            langid!("en-US"),
            "library",
            include_str!("../locales/en-US/library.ftl"),
        ),
        (
            langid!("en-US"),
            "details",
            include_str!("../locales/en-US/details.ftl"),
        ),
        (
            langid!("en-US"),
            "settings",
            include_str!("../locales/en-US/settings.ftl"),
        ),
        (
            langid!("zh-CN"),
            "common",
            include_str!("../locales/zh-CN/common.ftl"),
        ),
        (
            langid!("zh-CN"),
            "library",
            include_str!("../locales/zh-CN/library.ftl"),
        ),
        (
            langid!("zh-CN"),
            "details",
            include_str!("../locales/zh-CN/details.ftl"),
        ),
        (
            langid!("zh-CN"),
            "settings",
            include_str!("../locales/zh-CN/settings.ftl"),
        ),
    ] {
        catalog.add_ftl(locale, namespace, source)?;
    }
    Ok(Rc::new(I18n::new(catalog.build()?, langid!("en-US"))?))
}

/// No locale setter or application access. Translator clones keep i18n resources alive;
/// the copied locale handle alone does not. Views install this context explicitly.
#[derive(Clone)]
pub struct ReadTranslations {
    pub common: Translator,
    pub library: Translator,
    pub details: Translator,
    pub settings: Translator,
    pub locale: ReadSignal<LanguageIdentifier>,
}

impl ReadTranslations {
    pub fn new(i18n: &I18n) -> std::result::Result<Self, mue_i18n::I18nError> {
        Ok(Self {
            common: i18n.translator("common")?,
            library: i18n.translator("library")?,
            details: i18n.translator("details")?,
            settings: i18n.translator("settings")?,
            locale: i18n.locale(),
        })
    }
}
