#[path = "lib.rs"]
mod atlas;

fn main() {
    let options = atlas::options();
    if options.print_licenses_if_requested() {
        return;
    }
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - glyph atlas stress".into(),
            window_width: 900,
            window_height: 700,
            #[cfg(feature = "vulkan")]
            platform: macroquad::miniquad::conf::Platform {
                rendering_backend: macroquad::miniquad::conf::RenderingBackend::Vulkan,
                ..Default::default()
            },
            ..Default::default()
        },
        atlas::run(options),
    );
}
