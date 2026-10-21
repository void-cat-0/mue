use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as UiColor,
    runtime::set_timeout,
    style::{Styleable, StyleableExt, vh, vw},
};

const BLUE: UiColor = UiColor::from_rgba(37, 58, 94, 255);
const CANVAS: UiColor = UiColor::BLACK;

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
    .style(mue_macroquad::style! {
    font-size: {responsive::font(32.)};
    line-height: {responsive::ui(44.)};
    })
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

pub async fn run() {
    responsive::sync();
    let mut app = App::new(
        stack()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            padding: {(vw(4.), vh(4.))};
            })
            .children((
                heading("Pointer events").style(mue_macroquad::style! {
                font-size: {responsive::font(32.)};
                line-height: {responsive::ui(42.)};
                margin-bottom: {vh(2.)};
                }),
                text("Click, hold, hover, or scroll over the panel below.").style(
                    mue_macroquad::style! {
                    font-size: {responsive::font(18.)};
                    line-height: {responsive::ui(28.)};
                    margin-bottom: {vh(4.)};
                    },
                ),
                view().style(mue_macroquad::style! {
                padding: {(responsive::ui(24.),)};
                width: 100%;
                max-width: {responsive::ui(440.)};
                min-height: {responsive::ui(240.)};
                background: {BLUE};
                }),
            )),
    );

    loop {
        clear_background(CANVAS.into());
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue – events".to_string(),
            ..Default::default()
        },
        run(),
    );
}
