//! A conventional Scene application: catalog, views, Scenes and artwork effects.
mod application;
mod artwork;
mod catalog;
mod effects;
mod input;
mod scene;
mod settings;
mod ui;
mod view;

use anyhow::Result;
use macroquad::prelude::Conf;
use mue_macroquad::font;

async fn run() -> Result<()> {
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans");
    font::initialize("en-US", db)?;
    application::Application::new()?.run().await
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Artwork expansion / return".into(),
            window_width: 1000,
            window_height: 720,
            ..Default::default()
        },
        async {
            run().await.expect("Song artwork transition failed");
        },
    );
}
