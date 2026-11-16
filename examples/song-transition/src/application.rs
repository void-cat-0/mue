//! The host owns cross-Scene resources, not a Scene or a framework service locator.

use crate::{
    catalog::Catalog,
    scene::{LibraryScene, SettingsFactory, SettingsScene},
    settings::Settings,
};
use anyhow::Result;
use macroquad::prelude::next_frame;
use mue_macroquad::App;
use std::rc::Rc;

pub struct Application {
    // Rust drops fields in declaration order: App and its consumers go first.
    app: App,
    _catalog: Rc<Catalog>,
    _settings: Settings,
}

impl Application {
    pub fn new() -> Result<Self> {
        let settings = Settings::new();
        let catalog = Rc::new(Catalog::load()?);
        let editor = settings.editor();
        // Library/Details can open the editor Scene, but cannot write the settings.
        let open_settings: SettingsFactory = Rc::new(move || SettingsScene::new(editor));
        let app = App::with_scene(LibraryScene::new(
            catalog.clone(),
            settings.read(),
            open_settings,
        ));
        Ok(Self {
            app,
            _catalog: catalog,
            _settings: settings,
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
