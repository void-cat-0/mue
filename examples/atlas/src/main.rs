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
            ..Default::default()
        },
        atlas::run(options),
    );
}
