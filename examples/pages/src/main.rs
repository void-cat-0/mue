//! Switch ordered pages while retaining each page's local state. Run with `cargo run -p pages`.
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::Animation,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt, vh, vw},
};

fn page(index: usize) -> Node {
    let count = signal(0);
    center()
        .size_full()
        .background(
            [
                Color::from_rgba(37, 58, 94, 255),
                Color::from_rgba(46, 82, 63, 255),
                Color::from_rgba(88, 52, 84, 255),
            ][index],
        )
        .children(
            stack()
                .w_full()
                .padding((vw(4.),))
                .items_center()
                .gap(vh(3.))
                .children((
                    heading(["Home", "Notes", "Settings"][index])
                        .font_size(responsive::font(46.4))
                        .line_height(responsive::ui(63.8)),
                    link(count.map(|n| format!("Count: {n}")))
                        .font_size(responsive::font(32.))
                        .line_height(responsive::ui(44.))
                        .min_height(responsive::ui(44.))
                        .on_activate(move || count.update(|n| *n += 1)),
                )),
        )
        .into_node()
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - pages".into(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    responsive::sync();
    let selected = signal(0usize);
    let bar = wrap()
        .w_full()
        .flex_none()
        .padding((vw(2.), vh(2.5)))
        .column_gap(vw(1.))
        .row_gap(vh(1.))
        .justify_center()
        .children(
            ["Home", "Notes", "Settings"]
                .into_iter()
                .enumerate()
                .map(|(index, title)| {
                    link(title)
                        .font_size(responsive::font(20.))
                        .line_height(responsive::ui(28.))
                        .padding((responsive::ui(12.), responsive::ui(12.)))
                        .min_height(responsive::ui(44.))
                        .on_activate(move || selected.set(index))
                        .into_node()
                })
                .collect::<Vec<_>>(),
        );
    let pages = pages(selected, [page(0), page(1), page(2)])
        .transition_animation(Animation::spring(0.5, 0.15))
        .flex_1()
        .min_width(0.)
        .min_height(0.);
    let mut app = App::new(
        stack()
            .size_full()
            .font_size(responsive::root_font())
            .gap(0.)
            .children((bar, separator(), pages)),
    );
    loop {
        responsive::sync();
        clear_background(macroquad::color::BLACK);
        app.frame();
        next_frame().await;
    }
}
