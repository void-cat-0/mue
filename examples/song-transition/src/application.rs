//! The host owns cross-Scene resources, not a Scene or a framework service locator.

use crate::{
    catalog::Catalog,
    localization::{ReadTranslations, load_i18n},
    scene::{LibraryScene, SettingsFactory, SettingsScene},
    settings::Settings,
};
use macroquad::prelude::next_frame;
use mue_i18n::I18n;
use mue_macroquad::App;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    #[error("failed to initialize fonts: {0}")]
    Fonts(#[from] mue_macroquad::font::FontsAlreadyInitialized),
    #[error("failed to load artwork: {0}")]
    Artwork(#[from] macroquad::Error),
    #[error(transparent)]
    Catalog(#[from] mue_i18n::CatalogError),
    #[error(transparent)]
    Localization(#[from] mue_i18n::I18nError),
    #[error(transparent)]
    Frame(#[from] mue_macroquad::AppError),
}

pub(crate) type Result<T> = std::result::Result<T, RunError>;

pub struct Application {
    // Rust drops fields in declaration order: App and its consumers go first.
    app: App,
    _catalog: Rc<Catalog>,
    _settings: Settings,
    _i18n: Rc<I18n>,
}

impl Application {
    pub fn new() -> Result<Self> {
        let settings = Settings::new();
        let i18n = load_i18n()?;
        let translations = ReadTranslations::new(&i18n)?;
        let catalog = Rc::new(Catalog::load()?);
        let editor = settings.editor();
        // Only the Settings Scene receives writable i18n. UI gets strong read-only translators.
        let open_settings: SettingsFactory = {
            let i18n = i18n.clone();
            let translations = translations.clone();
            Rc::new(move || SettingsScene::new(editor, translations.clone(), i18n.clone()))
        };
        let app = App::with_scene(LibraryScene::new(
            catalog.clone(),
            settings.read(),
            translations,
            open_settings,
        ));
        Ok(Self {
            app,
            _catalog: catalog,
            _settings: settings,
            _i18n: i18n,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        while !self.app.is_exited() {
            self.app.try_frame()?;
            if !self.app.is_exited() {
                next_frame().await;
            }
        }
        Ok(())
    }
}
