#[path = "lib.rs"]
mod playground;

use playground::run;

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - playground".into(),
            window_width: 1120,
            window_height: 800,
            #[cfg(feature = "vulkan")]
            platform: macroquad::miniquad::conf::Platform {
                rendering_backend: macroquad::miniquad::conf::RenderingBackend::Vulkan,
                ..Default::default()
            },
            ..Default::default()
        },
        run(),
    );
}
