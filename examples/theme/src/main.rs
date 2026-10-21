//! Selector-based themes, factory options, classes, and direct constructors.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{Owned, prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    color::Color,
    factory::factory,
    node::TextBuilder,
    style::{Styleable, StyleableExt, vh, vw},
    theme::{provide_theme, theme},
};
use std::rc::Rc;

const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const NIGHT: Color = Color::from_rgba(22, 30, 44, 255);
const INK: Color = Color::from_rgba(30, 40, 55, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const MUTED: Color = Color::from_rgba(118, 130, 149, 255);
const GREEN: Color = Color::from_rgba(46, 110, 76, 255);
const WHITE: Color = Color::WHITE;

#[derive(Clone)]
struct ButtonOptions {
    primary: bool,
    progress: i32,
}

fn build_label(caption: Prop<Rc<str>>) -> TextBuilder {
    text(caption)
}

fn build_canvas(children: Owned<Children>) -> ScrollAreaBuilder {
    scroll_area()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        font-size: {responsive::root_font()};
        })
        .children(
            stack()
                .style(mue_macroquad::style! {
                width: 100%;
                padding: {(vw(6.), vh(6.))};
                })
                .children(children),
        )
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - theme".into(),
            window_width: 640,
            window_height: 480,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();
    let label = factory(build_label);
    let button_label = label.clone();
    let button = factory(move |caption: Prop<Rc<str>>| {
        flexbox()
            .style(mue_macroquad::style! {
            align-items: center;
            justify-content: center;
            })
            .children(button_label.create(caption).style(style! {
                font-size: {responsive::font(18.)};
                line-height: {responsive::ui(26.)};
                color: {WHITE};
                min-width: 0px;
            }))
    })
    .defaults(ButtonOptions {
        primary: false,
        progress: 0,
    });
    let canvas = factory(build_canvas);
    let dark = signal(false);
    let count = signal(0);
    let primary_active = signal(true);
    let base = theme! {
        canvas {
            background: { if dark.get() { NIGHT } else { PAPER } };
        }
        label {
            color: { if dark.get() { WHITE } else { INK } };
            font-size: 22px;
            line-height: 1.45;
        }
        button[primary] {
            background: { if dark.get() { GREEN } else { BLUE } };
        }
        button[progress < 32] {
            opacity: 96%;
        }
        button {
            background: { if dark.get() { MUTED } else { Color::from_rgba(175, 185, 198, 255) } };
        }
        button:hover {
            opacity: 82%;
        }
        * {
            transition: 280ms ease-in-out;
        }
    };

    let accent = theme! {
        button[primary] {
            background: { if dark.get() { BLUE } else { GREEN } };
        }
    };

    let mut app = App::new(provide_theme(base, move || {
        canvas
            .create(
                (
                    label
                        .create("Daily practice")
                        .style(style! { font-size: 32px; line-height: 1.4; }),
                    label
                        .create(count.map(|n| format!("{n} sessions complete")))
                        .style(style! { margin-top: 5vh; margin-bottom: 6.5vh; }),
                    wrap()
                        .style(mue_macroquad::style! { gap: {vw(2.)}; })
                        .children((
                            provide_theme(accent, {
                                let button = button.clone();
                                let primary_active = primary_active;
                                move || {
                                    button
                                        .create("Complete session")
                                        .options(|options| {
                                            options.primary = true;
                                            options.progress = count.get();
                                        })
                                        .class("primary", primary_active)
                                        .style(style! {
                                            min-height: 44px;
                                            padding: 12px 20px;
                                            border-radius: 12px;
                                        })
                                        .on_activate(move || count.update(|n| *n += 1))
                                }
                            }),
                            button
                                .create("Reset")
                                .options(|options| options.primary = false)
                                .style(style! {
                                    min-height: 44px;
                                    padding: 12px 20px;
                                    border-radius: 12px;
                                })
                                .on_activate(move || count.set(0)),
                        )),
                    button
                        .create(dark.select("Light theme", "Dark theme"))
                        .style(style! {
                            margin-top: 6.5vh;
                            min-height: 44px;
                            padding: 12px 20px;
                            border-radius: 12px;
                        })
                        .on_activate(move || {
                            dark.toggle();
                            primary_active.toggle();
                        }),
                )
                    .into_children(),
            )
            .style(style! { width: 100%; height: 100%; })
    }));

    loop {
        responsive::sync();
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}
