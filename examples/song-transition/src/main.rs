//! A conventional Scene application: catalog, views, Scenes and artwork effects.
mod application;
mod artwork;
mod catalog;
mod effects;
mod input;
mod localization;
mod scene;
mod settings;
mod toast;
mod ui;
mod view;

use application::Result;
use macroquad::prelude::Conf;
use mue_macroquad::font;

async fn run() -> Result<()> {
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    // Reuse the existing workspace font; Latin-only Noto Sans does not cover Chinese.
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSansCJKsc-Regular.otf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans CJK SC");
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
