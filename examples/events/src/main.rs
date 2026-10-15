fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue – events".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        events::run(),
    );
}
