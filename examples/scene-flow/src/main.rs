//! An offline consumer: retained details -> loading -> ready -> run -> results.
mod assets;
mod flow;
mod input;
mod scene;
mod ui;

use anyhow::Result;
use macroquad::prelude::*;
use mue_macroquad::{App, font};
use scene::Details;

async fn run() -> Result<()> {
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans");
    font::initialize("en-US", db)?;

    let mut app = App::with_scene(Details::new());
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
            window_title: "mue - Scene flow".into(),
            window_width: 960,
            window_height: 640,
            ..Default::default()
        },
        async {
            run().await.expect("Scene flow failed");
        },
    );
}
