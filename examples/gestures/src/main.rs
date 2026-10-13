//! Each panel owns a recognizer. Pinch and rotate need two touch contacts.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt},
};

fn panel(title: &str, value: ReadSignal<String>) -> StackBuilder {
    stack()
        .width(250.)
        .height(180.)
        .padding((20.,))
        .radius(12.)
        .background(Color::from_rgba(30, 40, 58, 255))
        .children((
            heading(title).font_size(24.).line_height(32.),
            text(value).font_size(18.).line_height(28.),
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
    let mut app = App::new(
        scroll_area()
            .size_full()
            .options(ScrollOptions {
                // Content drags belong to the panels; scroll with the wheel or scrollbar.
                touch: false,
                ..Default::default()
            })
            .children(
                wrap().padding((24.,)).gap(18.).children((
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
        app.frame();
        next_frame().await;
    }
}
