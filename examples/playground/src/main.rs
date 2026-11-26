#[path = "lib.rs"]
mod playground;

use playground::run;

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - playground".into(),
            window_width: 1100,
            window_height: 640,
            ..Default::default()
        },
        run(),
    );
}
