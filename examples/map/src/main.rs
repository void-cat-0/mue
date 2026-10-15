fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - deer map".into(),
            window_width: 1000,
            window_height: 720,
            ..Default::default()
        },
        map::run(),
    );
}
