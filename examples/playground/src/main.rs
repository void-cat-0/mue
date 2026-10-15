use playground::run;

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - playground".into(),
            window_width: 1120,
            window_height: 800,
            ..Default::default()
        },
        run(),
    );
}
