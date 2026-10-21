//! Implicit style animation and explicit state transactions.
//! Run with `cargo run -p animation`.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as UiColor,
    shader::Brush,
    style::{Styleable, StyleableExt, percent, vh, vw},
    transition::Transition,
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
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            padding: {(vw(5.), vh(5.))};
            background: {CANVAS};
            })
            .children((
                wrap()
                    .style(mue_macroquad::style! {
                    flex: none;
                    column-gap: {vw(2.)};
                    row-gap: {vh(1.5)};
                    })
                    .children((
                        heading("Motion study").style(mue_macroquad::style! {
                        font-size: {responsive::font(32.)};
                        line-height: {responsive::ui(44. * 1.45)};
                        min-width: 0px;
                        flex-grow: {1.};
                        }),
                        flexbox()
                            .style(mue_macroquad::style! {
                            background: {SURFACE};
                            padding: {(responsive::ui(16.), responsive::ui(10.))};
                            min-width: {responsive::ui(44.)};
                            min-height: {responsive::ui(44.)};
                            max-width: 100%;
                            })
                            .toggle_on_tap(spring)
                            .children(text(spring.select("Spring", "Ease in out")).style(
                                mue_macroquad::style! {
                                font-size: {responsive::font(18.)};
                                line-height: {responsive::ui(44.)};
                                },
                            )),
                    )),
                flexbox()
                    .style(mue_macroquad::style! {
                    flex: 1;
                    min-height: 0px;
                    padding: {(0., vh(2.))};
                    justify-content: center;
                    align-items: center;
                    })
                    .children(
                        flexbox()
                            .style(mue_macroquad::style! {
                            width: {expanded.select(percent(64.), percent(24.))};
                            max-width: 100%;
                            max-height: 100%;
                            aspect-ratio: {expanded.select(Some(2.), Some(1.))};
                            border-radius: {responsive::ui(expanded.select(24., 90.))};
                            background: {expanded.select(Brush::Solid(PLUM), Brush::Solid(BLUE))};
                            align-items: center;
                            justify-content: center;
                            })
                            .children(text(expanded.select("Open", "Closed")).style(
                                mue_macroquad::style! {
                                font-size: {responsive::font(28.)};
                                line-height: {responsive::ui(44.)};
                                },
                            )),
                    ),
                center()
                    .style(mue_macroquad::style! {
                    flex: none;
                    min-height: {responsive::ui(44.)};
                    padding: {(responsive::ui(16.), responsive::ui(8.))};
                    background: {hovered.select(Brush::Solid(HIGHLIGHT), Brush::Solid(ACTIVE))};
                    transition: {computed(move |_| {
                                            if spring.get() {
                                                Transition::spring(0.8, 0.35)
                                            } else {
                                                Transition::ease_in_out(0.65)
                                            }
                                        })};
                    })
                    .use_hovered(hovered)
                    .on_activate(move || expanded.toggle())
                    .children(text(expanded.select("Close", "Open")).style(
                        mue_macroquad::style! {
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(44.)};
                        },
                    )),
            )),
    );
    loop {
        clear_background(CANVAS.into());
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}
