//! Declaration-based themes, reactive options, whole class sets, and explicit hover.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    style::{Styleable, StyleableExt, vh, vw},
    theme::{Theme, Themeable, class, theme},
};
use std::{collections::HashSet, rc::Rc};

const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const NIGHT: Color = Color::from_rgba(22, 30, 44, 255);
const INK: Color = Color::from_rgba(30, 40, 55, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const MUTED: Color = Color::from_rgba(118, 130, 149, 255);
const GREEN: Color = Color::from_rgba(46, 110, 76, 255);
const WHITE: Color = Color::WHITE;

#[component]
#[theme]
fn label(caption: Rc<str>) -> impl Component {
    text(caption)
}

#[component]
#[theme(primary, progress)]
fn button(caption: Rc<str>, #[default] primary: bool, #[default] progress: i32) -> impl Component {
    let _ = (primary, progress);
    flexbox()
        .style(style! {
            align-items: center;
            justify-content: center;
            min-height: 44px;
            padding: 12px 20px;
            border-radius: 12px;
        })
        .children(label(caption).style(style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(26.)};
            color: {WHITE};
            min-width: 0px;
        }))
}

#[component]
#[theme]
fn canvas() -> impl Component {
    scroll_area()
        .style(style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
        })
        .content_style(style! {
            padding: {(vw(6.), vh(6.))};
            flex-direction: column;
        })
}

fn appearance(dark: bool) -> Theme {
    let base = theme! {
        Canvas { background: {if dark { NIGHT } else { PAPER }}; }
        Label {
            color: {if dark { WHITE } else { INK }};
            font-size: 22px;
            line-height: 1.45;
        }
        Button {
            background: {if dark { MUTED } else { Color::from_rgba(175, 185, 198, 255) }};
        }
        Button[$primary] { background: {if dark { GREEN } else { BLUE }}; }
        Button[$progress < 32] { opacity: 96%; }
        Button:hover { opacity: 82%; }
        * { transition: 280ms ease-in-out; }
    };
    // Composition appends rules, not a nested theme scope.
    let accent = theme! {
        Button.selected[$primary] { background: {if dark { BLUE } else { GREEN }}; }
    };
    Theme::compose([base, accent])
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
    let dark = signal(false);
    let count = signal(0);
    let selected = signal(true);
    let classes = selected.map(|active| {
        if active {
            HashSet::from([class!("selected")])
        } else {
            HashSet::new()
        }
    });
    let mut app = App::with_theme(
        dark.select(appearance(true), appearance(false)),
        move || {
            canvas().children((
                label("Daily practice").style(style! { font-size: 32px; line-height: 1.4; }),
                label(count.map(|n| format!("{n} sessions complete")))
                    .style(style! { margin-top: 5vh; margin-bottom: 6.5vh; }),
                wrap().style(style! { gap: {vw(2.)}; }).children((
                    button("Complete session")
                        .primary(true)
                        .progress(count)
                        .classes(classes)
                        .enable_hovered()
                        .on_activate(move || count.update(|n| *n += 1)),
                    button("Reset")
                        .enable_hovered()
                        .on_activate(move || count.set(0)),
                )),
                button(dark.select("Light theme", "Dark theme"))
                    .enable_hovered()
                    .style(style! { margin-top: 6.5vh; })
                    .on_activate(move || {
                        dark.toggle();
                        selected.toggle();
                    }),
            ))
        },
    );

    loop {
        responsive::sync();
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}
