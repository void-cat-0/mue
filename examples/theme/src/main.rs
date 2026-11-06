//! Application-owned color tokens shared through ordinary provide/inject context.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{
    prelude::*,
    scope::{inject, provide},
};
use mue_macroquad::{
    App,
    color::Color,
    inherit::provide_foreground,
    shader::Brush,
    style::{Styleable, StyleableExt, vh, vw},
};
use std::rc::Rc;

const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const NIGHT: Color = Color::from_rgba(22, 30, 44, 255);
const INK: Color = Color::from_rgba(30, 40, 55, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const MUTED: Color = Color::from_rgba(118, 130, 149, 255);
const GREEN: Color = Color::from_rgba(46, 110, 76, 255);

// These tokens belong to this application; components choose which fields to use.
#[derive(Clone, Copy, PartialEq)]
struct Tokens {
    canvas: Color,
    foreground: Color,
    neutral: Color,
    primary: Color,
    selected: Color,
}
impl PropValue for Tokens {}

impl Tokens {
    fn for_mode(dark: bool) -> Self {
        Self {
            canvas: if dark { NIGHT } else { PAPER },
            foreground: if dark { Color::WHITE } else { INK },
            neutral: if dark {
                MUTED
            } else {
                Color::from_rgba(175, 185, 198, 255)
            },
            primary: if dark { GREEN } else { BLUE },
            selected: if dark { BLUE } else { GREEN },
        }
    }
}

#[derive(Clone)]
struct Palette(Prop<Tokens>);

fn palette() -> Prop<Tokens> {
    inject::<Palette>()
        .expect("palette provided by the application")
        .0
}

#[component]
fn label(caption: Rc<str>) -> impl Component {
    text(caption).style(style! {
        font-size: 22px;
        line-height: 1.45;
        transition: 280ms ease-in-out;
    })
}

#[component]
fn button(
    caption: Rc<str>,
    #[default] primary: bool,
    #[default] progress: i32,
    #[default] selected: bool,
) -> impl Component {
    let tokens = palette();
    flexbox()
        .style(style! {
            align-items: center;
            justify-content: center;
            min-height: 44px;
            padding: 12px 20px;
            border-radius: 12px;
            background: {computed(move |_| {
                let tokens = tokens.get();
                Brush::Solid(if primary.get() {
                    if selected.get() { tokens.selected } else { tokens.primary }
                } else {
                    tokens.neutral
                })
            })};
            transition: 280ms ease-in-out;
        })
        .with_hovered(move |view, hovered| {
            view.opacity(computed(move |_| {
                if hovered.get() {
                    0.82
                } else if progress.get() < 32 {
                    0.96
                } else {
                    1.
                }
            }))
        })
        .children(label(caption).style(style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(26.)};
            color: {Color::WHITE};
            min-width: 0px;
        }))
}

#[component]
fn canvas() -> impl Component {
    scroll_area()
        .style(style! {
            width: 100%; height: 100%;
            background: {palette().map(|tokens| Brush::Solid(tokens.canvas))};
            font-size: {responsive::root_font()};
            transition: 280ms ease-in-out;
        })
        .content_style(style! {
            padding: {(vw(6.), vh(6.))};
            flex-direction: column;
        })
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - explicit color tokens".into(),
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
    let tokens = dark.map(Tokens::for_mode);
    let mut app = App::new(provide_foreground(
        tokens.map(|tokens| Brush::Solid(tokens.foreground)),
        move || {
            provide(Palette(tokens.into()));
            canvas().children((
                label("Daily practice").style(style! { font-size: 32px; line-height: 1.4; }),
                label(count.map(|n| format!("{n} sessions complete")))
                    .style(style! { margin-top: 5vh; margin-bottom: 6.5vh; }),
                wrap().style(style! { gap: {vw(2.)}; }).children((
                    button("Complete session")
                        .primary(true)
                        .progress(count)
                        .selected(selected)
                        .on_activate(move || count.update(|n| *n += 1)),
                    button("Reset").on_activate(move || count.set(0)),
                )),
                button(dark.select("Light mode", "Dark mode"))
                    .style(style! { margin-top: 6.5vh; })
                    .on_activate(move || {
                        dark.toggle();
                        selected.toggle();
                    }),
            ))
        },
    ));

    loop {
        responsive::sync();
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}
