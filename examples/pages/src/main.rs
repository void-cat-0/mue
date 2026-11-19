//! Switch ordered pages while retaining each page's local state. Run with `cargo run -p pages`.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    style::{Styleable, StyleableExt, vh, vw},
    transition::Transition,
};

fn page(index: usize) -> Node {
    let count = signal(0);
    center()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        background: {[
                    Color::from_rgba(37, 58, 94, 255),
                    Color::from_rgba(46, 82, 63, 255),
                    Color::from_rgba(88, 52, 84, 255),
                ][index]};
        })
        .children(
            stack()
                .style(mue_macroquad::style! {
                width: 100%;
                padding: {(vw(4.),)};
                align-items: center;
                gap: {vh(3.)};
                })
                .children((
                    heading(["Home", "Notes", "Settings"][index]).style(mue_macroquad::style! {
                    font-size: {responsive::font(46.4)};
                    line-height: {responsive::ui(63.8)};
                    }),
                    link(count.map(|n| format!("Count: {n}")))
                        .style(mue_macroquad::style! {
                        font-size: {responsive::font(32.)};
                        line-height: {responsive::ui(44.)};
                        min-height: {responsive::ui(44.)};
                        })
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
        .style(mue_macroquad::style! {
        width: 100%;
        flex: none;
        padding: {(vw(2.), vh(2.5))};
        column-gap: {vw(1.)};
        row-gap: {vh(1.)};
        justify-content: center;
        })
        .children(
            ["Home", "Notes", "Settings"]
                .into_iter()
                .enumerate()
                .map(|(index, title)| {
                    link(title)
                        .style(mue_macroquad::style! {
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(28.)};
                        padding: {(responsive::ui(12.), responsive::ui(12.))};
                        min-height: {responsive::ui(44.)};
                        })
                        .on_activate(move || selected.set(index))
                        .into_node()
                })
                .collect::<Vec<_>>(),
        );
    let pages = pages(selected, [page(0), page(1), page(2)])
        .transition_curve(Transition::spring(0.5, 0.15))
        .style(mue_macroquad::style! {
        flex: 1;
        min-width: 0px;
        min-height: 0px;
        });
    let mut app = App::new(
        stack()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            gap: 0px;
            })
            .children((bar, separator(), pages)),
    );
    loop {
        responsive::sync();
        clear_background(macroquad::color::BLACK);
        app.frame();
        next_frame().await;
    }
}
