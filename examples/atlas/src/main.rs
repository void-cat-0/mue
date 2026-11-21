#[path = "lib.rs"]
mod atlas;
#[path = "../../shared/render_backend.rs"]
mod render_backend;

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
            platform: miniquad::conf::Platform {
                rendering_backend: render_backend::preferred(),
                ..Default::default()
            },
            ..Default::default()
        },
        atlas::run(options),
    );
}
