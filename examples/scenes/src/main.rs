//! A Scene stack preserves gameplay and UI while a result Scene is pushed.
mod input;
mod model;
mod scene;
mod ui;

use anyhow::Result;
use macroquad::prelude::*;
use mue_macroquad::{App, font};

use scene::GameScene;

async fn run() -> Result<()> {
    // Initialize before creating UI. No system font lookup or runtime assets.
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans");
    font::initialize("en-US", db)?;

    let mut app = App::with_scene(GameScene::new());
    while !app.is_exited() {
        app.try_frame()?;
        if !app.is_exited() {
            next_frame().await;
        }
    }
    Ok(())
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Scenes".to_owned(),
            window_width: 960,
            window_height: 640,
            ..Default::default()
        },
        async {
            run().await.expect("Scene example failed");
        },
    );
}
