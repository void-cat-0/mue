//! Each panel owns a recognizer. Pinch and rotate need two touch contacts.
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
};

fn panel(title: &str, value: ReadSignal<String>) -> StackBuilder {
    stack()
        .flex_basis(responsive::ui(250.))
        .flex_grow(1.)
        .min_width(0.)
        .max_width(percent(100.))
        .min_height(responsive::ui(180.))
        .padding((responsive::ui(20.),))
        .radius(responsive::ui(12.))
        .background(Color::from_rgba(30, 40, 58, 255))
        .children((
            heading(title)
                .font_size(responsive::font(24.))
                .line_height(responsive::ui(32.)),
            text(value)
                .font_size(responsive::font(18.))
                .line_height(responsive::ui(28.)),
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
            .size_full()
            .font_size(responsive::root_font())
            .options(ScrollOptions {
                // Content drags belong to the panels; scroll with the wheel or scrollbar.
                touch: false,
                ..Default::default()
            })
            .children(
                wrap()
                    .w_full()
                    .padding((vw(3.), vh(3.5)))
                    .column_gap(vw(2.))
                    .row_gap(vh(3.))
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
