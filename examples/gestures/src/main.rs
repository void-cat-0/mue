//! Each panel owns a recognizer. Pinch and rotate need two touch contacts.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    style::{Styleable, StyleableExt, vh, vw},
};

fn panel(title: &str, value: ReadSignal<String>) -> StackBuilder {
    stack()
        .style(mue_macroquad::style! {
        flex-basis: {responsive::ui(250.)};
        flex-grow: {1.};
        min-width: 0px;
        max-width: 100%;
        min-height: {responsive::ui(180.)};
        padding: {(responsive::ui(20.),)};
        border-radius: {responsive::ui(12.)};
        background: {Color::from_rgba(30, 40, 58, 255)};
        })
        .children((
            heading(title).style(mue_macroquad::style! {
            font-size: {responsive::font(24.)};
            line-height: {responsive::ui(32.)};
            }),
            text(value).style(mue_macroquad::style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(28.)};
            }),
        ))
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - gestures".into(),
            window_width: 860,
            window_height: 660,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    let tap = signal(0);
    let held = signal(0);
    let hovered = signal(false);
    let wheel = signal(0.);
    let pan = signal(String::from("Drag"));
    let swipe = signal(String::from("Swipe"));
    let pinch = signal(1.);
    let rotate = signal(0.);
    responsive::sync();
    let mut app = App::new(
        scroll_area()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            })
            .options(ScrollOptions {
                // Content drags belong to the panels; scroll with the wheel or scrollbar.
                touch: false,
                ..Default::default()
            })
            .children(
                wrap()
                    .style(mue_macroquad::style! {
                    width: 100%;
                    padding: {(vw(3.), vh(3.5))};
                    column-gap: {vw(2.)};
                    row-gap: {vh(3.)};
                    })
                    .children((
                        panel("Tap", tap.map(|n| format!("{n}")))
                            .on_activate(move || tap.update(|n| *n += 1)),
                        panel("Long press", held.map(|n| format!("{n}")))
                            .on_long_press(move |_| held.update(|n| *n += 1)),
                        panel("Hover", hovered.map(|v| format!("{v}"))).use_hovered(hovered),
                        panel("Wheel", wheel.map(|v| format!("{v:.1}")))
                            .on_wheel_event(move |e| wheel.update(|n| *n += e.delta.y)),
                        panel("Pan", *pan).on_pan(move |e| {
                            pan.set(format!(
                                "{:?}\n{:.0}, {:.0}",
                                e.phase, e.translation.x, e.translation.y
                            ))
                        }),
                        panel("Swipe", *swipe)
                            .on_swipe(move |e| swipe.set(format!("{:?}", e.direction))),
                        panel("Pinch", pinch.map(|v| format!("{v:.2}x")))
                            .on_pinch(move |e| pinch.set(e.scale)),
                        panel(
                            "Rotate",
                            rotate.map(|v: f32| format!("{:.0}°", v.to_degrees())),
                        )
                        .on_rotate(move |e| rotate.set(e.rotation)),
                    )),
            ),
    );
    loop {
        clear_background(macroquad::color::BLACK);
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}
