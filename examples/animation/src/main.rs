//! Implicit style animation and explicit state transactions.
//! Run with `cargo run -p animation`.
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::{Animation, with_animation},
    color::Color as UiColor,
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt, percent, vh, vw},
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
    responsive::sync();
    let mut app = App::new(
        stack()
            .size_full()
            .font_size(responsive::root_font())
            .padding((vw(5.), vh(5.)))
            .background(CANVAS)
            .children((
                wrap()
                    .flex_none()
                    .column_gap(vw(2.))
                    .row_gap(vh(1.5))
                    .children((
                        heading("Motion study")
                            .font_size(responsive::font(32.))
                            .line_height(responsive::ui(44. * 1.45))
                            .min_width(0.)
                            .flex_grow(1.),
                        flexbox()
                            .background(SURFACE)
                            .padding((responsive::ui(16.), responsive::ui(10.)))
                            .min_width(responsive::ui(44.))
                            .min_height(responsive::ui(44.))
                            .max_width(percent(100.))
                            .toggle_on_tap(spring)
                            .children(
                                text(spring.select("Spring", "Ease in out"))
                                    .font_size(responsive::font(18.))
                                    .line_height(responsive::ui(44.)),
                            ),
                    )),
                flexbox()
                    .flex_1()
                    .min_height(0.)
                    .padding((0., vh(2.)))
                    .justify_center()
                    .items_center()
                    .children(
                        flexbox()
                            .width(expanded.select(percent(64.), percent(24.)))
                            .max_width(percent(100.))
                            .max_height(percent(100.))
                            .aspect_ratio(expanded.select(Some(2.), Some(1.)))
                            .radius(responsive::ui(expanded.select(24., 90.)))
                            .background(expanded.select(Brush::Solid(PLUM), Brush::Solid(BLUE)))
                            .items_center()
                            .justify_center()
                            .children(
                                text(expanded.select("Open", "Closed"))
                                    .font_size(responsive::font(28.))
                                    .line_height(responsive::ui(44.)),
                            ),
                    ),
                center()
                    .flex_none()
                    .min_height(responsive::ui(44.))
                    .padding((responsive::ui(16.), responsive::ui(8.)))
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
                    .children(
                        text(expanded.select("Close", "Open"))
                            .font_size(responsive::font(20.))
                            .line_height(responsive::ui(44.)),
                    ),
            )),
    );
    loop {
        clear_background(CANVAS.into());
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}
