//! The host owns cross-Scene resources, not a Scene or a framework service locator.

use crate::{
    catalog::Catalog,
    localization::{ReadTranslations, load_i18n},
    scene::{LibraryScene, SettingsFactory, SettingsScene},
    settings::Settings,
    toast::Notices,
};
use macroquad::window::next_frame;
use mue_components::{IntoNode, ToastPalette, dialog_host, toast_host};
use mue_core::scope::OwnedScope;
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
    // Drop Scenes and the overlay before their application resources.
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
        let (toast_root, toast_sender) = toast_host();
        let notices = Notices::new(toast_sender, translations.common.clone());
        let (modal, dialogs) = dialog_host();
        let theme_bindings = OwnedScope::new();
        let toast_palette = theme_bindings.run(|| {
            settings.read().theme.map(|theme| {
                let palette = theme.palette();
                ToastPalette {
                    background: palette.selected,
                    foreground: palette.foreground,
                    muted: palette.muted,
                }
            })
        });
        let editor = settings.editor();
        // Only the Settings Scene receives writable i18n. UI gets strong read-only translators.
        let open_settings: SettingsFactory = {
            let i18n = i18n.clone();
            let translations = translations.clone();
            let notices = notices.clone();
            Rc::new(move || {
                SettingsScene::new(editor, translations.clone(), i18n.clone(), notices.clone())
            })
        };
        let mut app = App::with_scene(LibraryScene::new(
            catalog.clone(),
            settings.read(),
            translations,
            open_settings,
            notices,
            dialogs,
        ));
        let toast_root = toast_root.palette(toast_palette).into_node();
        toast_root.retain_owned_scope(theme_bindings);
        app.set_overlay_root(toast_root);
        app.set_modal_root(modal);
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
