//! Implicit style animation and explicit state transactions.
//! Run with `cargo run -p animation`.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::{Animation, with_animation},
    color::Color as UiColor,
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt},
};

const ACTIVE: UiColor = UiColor::from_rgba(70, 95, 130, 255);
const BLUE: UiColor = UiColor::from_rgba(37, 58, 94, 255);
const CANVAS: UiColor = UiColor::BLACK;
const HIGHLIGHT: UiColor = UiColor::from_rgba(100, 135, 180, 255);
const PLUM: UiColor = UiColor::from_rgba(88, 52, 84, 255);
const SURFACE: UiColor = UiColor::from_rgba(25, 30, 40, 255);

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - animation".into(),
            window_width: 840,
            window_height: 620,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let expanded = signal(false);
    let spring = signal(true);
    let hovered = signal(false);
    let mut app = App::new(
        flexbox()
            .size_full()
            .flex_column()
            .padding((40.,))
            .background(CANVAS)
            .children((
                flexbox().flex_row().items_center().children((
                    text("Motion study").font_size(32.).flex_1(),
                    flexbox()
                        .background(SURFACE)
                        .padding((16., 10.))
                        .toggle_on_tap(spring)
                        .children(text(spring.select("Spring", "Ease in out")).font_size(18.)),
                )),
                flexbox().flex_1().justify_center().items_center().children(
                    flexbox()
                        .width(expanded.select(480., 180.))
                        .height(expanded.select(240., 180.))
                        .radius(expanded.select(24., 90.))
                        .background(expanded.select(Brush::Solid(PLUM), Brush::Solid(BLUE)))
                        .items_center()
                        .justify_center()
                        .children(text(expanded.select("Open", "Closed")).font_size(28.)),
                ),
                flexbox()
                    .items_center()
                    .justify_center()
                    .height(56.)
                    .background(hovered.select(Brush::Solid(HIGHLIGHT), Brush::Solid(ACTIVE)))
                    .animation(Animation::ease_in_out(0.18))
                    .use_hovered(hovered)
                    .on_activate(move || {
                        let curve = if spring.get() {
                            Animation::spring(0.8, 0.35)
                        } else {
                            Animation::ease_in_out(0.65)
                        };
                        with_animation(curve, || expanded.toggle());
                    })
                    .children(text(expanded.select("Close", "Open")).font_size(20.)),
            )),
    );
    loop {
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
