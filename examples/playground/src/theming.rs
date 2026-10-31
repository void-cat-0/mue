use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    style::Styleable,
    theme::{Theme, Themeable, class, theme},
};
use std::{collections::HashSet, rc::Rc};

#[component]
#[theme(accent)]
fn tile(caption: Rc<str>, #[default] accent: bool) -> impl Component {
    let _ = accent;
    center().children(text(caption).style(style! {
        font-size: {responsive::font(22.)};
        line-height: {responsive::ui(32.)};
    }))
}

// Installed once by the playground's App::with_theme entry point. New pages
// inherit that application context, including pages built by map_keyed.
pub fn appearance() -> ReadSignal<Theme> {
    let light = theme! {
        Tile { background: {BLUE}; transform: scale(1.02); }
        Tile[$accent] { background: {CORAL}; transform: scale(1.04); }
    };
    let dark = theme! {
        Tile { background: {panel_brush()}; transform: scale(1.); }
        Tile[$accent] { background: {BLUE}; transform: scale(1.); }
    };
    let shared = theme! {
        Tile.selected { opacity: 82%; }
        Tile[$accent]:hover { opacity: 65%; }
        Tile { transition: 220ms spring; }
    };
    dark_theme().select(
        Theme::compose([dark, shared.clone()]),
        Theme::compose([light, shared]),
    )
}

pub fn page() -> Node {
    let dark = dark_theme();
    let selected = signal(true);
    let classes = selected.map(|active| {
        if active {
            HashSet::from([class!("selected")])
        } else {
            HashSet::new()
        }
    });
    let sample = r#"use std::{collections::HashSet, rc::Rc};
use mue_core::prelude::*;
use mue_components::prelude::*;
use mue_macroquad::App;

#[component]
#[theme(accent)]
fn tile(caption: Rc<str>, #[default] accent: bool) -> impl Component {
    let _ = accent;
    center().children(text(caption))
}

let light = theme! {
    Tile { background: blue; }
    Tile[$accent] { background: coral; }
};
let dark = theme! {
    Tile { background: #263244; }
    Tile[$accent] { background: blue; }
};
let shared = theme! {
    Tile.selected { opacity: 82%; }
    Tile[$accent]:hover { opacity: 65%; }
};
let dark_mode = signal(false);
let theme = dark_mode.select(
    Theme::compose([dark, shared.clone()]),
    Theme::compose([light, shared]),
);
let mut app = App::with_theme(theme, || {
    tile("Accent")
        .accent(true)
        .classes(HashSet::from([class!("selected")]))
        .enable_hovered()
});"#;
    stack()
        .style(style! {
            width: 100%; min-width: 0px;
            gap: {responsive::ui(20.)};
        })
        .children((
            button(
                dark.map(|is_dark| format!("Theme: {}", if is_dark { "Dark" } else { "Light" })),
                toggle_theme,
            ),
            button("Toggle selected class", move || selected.toggle()),
            tile("Default"),
            tile("Accent").accent(true).enable_hovered(),
            tile("Selected accent")
                .accent(true)
                .classes(classes)
                .enable_hovered(),
            text("Theme changes update existing styles. Class sets replace the whole set; hover is enabled explicitly."),
            code_block(sample).style(style! {
                width: 100%; min-width: 0px;
                text-wrap: {true};
                font-size: {responsive::font(16.)};
                line-height: {responsive::ui(24.)};
                padding: {(responsive::ui(16.),)};
                border-radius: {responsive::ui(6.)};
            }),
        ))
        .into_node()
}
