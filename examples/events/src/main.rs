use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as UiColor,
    node::*,
    runtime::set_timeout,
    style::{Styleable, StyleableExt},
};

const BLUE: UiColor = UiColor::from_rgba(37, 58, 94, 255);
const CANVAS: UiColor = UiColor::BLACK;

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue – events".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        run(),
    );
}

#[mue_macros::component]
fn view() -> impl Component {
    let pressed = signal(false);
    let hovered = signal(false);
    let wheel = signal(0.0_f32);
    let count = signal(0);
    let long_pressed = signal(false);
    let mut long_pressed_handle = None;

    text(computed(move |_| {
        format!(
            "Clicked {} times\nPressed: {}{}{}\nScroll: {:.0}",
            count.get(),
            pressed.get(),
            if long_pressed.get() {
                "\nLong pressed"
            } else {
                ""
            },
            if hovered.get() { "\nHovered" } else { "" },
            wheel.get()
        )
    }))
    .use_pressed(pressed)
    .use_hovered(hovered)
    .on_activate(move || count.update(|c| *c += 1))
    .on_long_press(move |_| {
        long_pressed.set(true);
        if let Some(old) = long_pressed_handle.replace(set_timeout(1., move || {
            long_pressed.set(false);
        })) {
            old.cancel();
        }
    })
    .on_wheel_event(move |event| wheel.update(|value| *value += event.delta.y))
}

async fn run() {
    let mut app = App::new(
        stack().size_full().padding((28.,)).children((
            heading("Pointer events")
                .font_size(32.)
                .line_height(42.)
                .margin_bottom(12.),
            text("Click, hold, hover, or scroll over the panel below.")
                .font_size(18.)
                .line_height(28.)
                .margin_bottom(24.),
            view()
                .padding((24.,))
                .width(440.)
                .height(240.)
                .background(BLUE),
        )),
    );

    loop {
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
