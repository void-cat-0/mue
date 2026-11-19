#[path = "lib.rs"]
mod portal;

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - portal".into(),
            window_width: 960,
            window_height: 540,
            ..Default::default()
        },
        portal::run(),
    );
}
