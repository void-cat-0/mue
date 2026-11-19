//! Public Scene transitions with a normal macroquad application loop.
mod color_scene;
mod effects;
mod input;

use color_scene::ColorScene;
use macroquad::{prelude::Conf, window::next_frame};
use mue_macroquad::{App, Result};

fn window_config() -> Conf {
    Conf {
        window_title: "mue - Scene transitions (1/2/3, Esc, Q)".into(),
        window_width: 640,
        window_height: 400,
        ..Default::default()
    }
}

async fn run() -> Result<()> {
    let mut app = App::with_scene(ColorScene::root());
    while !app.is_exited() {
        app.try_frame()?;
        if !app.is_exited() {
            next_frame().await;
        }
    }
    Ok(())
}

fn main() {
    macroquad::Window::from_config(window_config(), async {
        run().await.expect("Scene transitions failed");
    });
}
