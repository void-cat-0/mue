use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    shader::Brush,
    style::{Styleable, StyleableExt, Transform, TransformOp},
};
use std::rc::Rc;

// Only the component's own state lives here; the theme is application tokens.
#[component]
fn tile(caption: Rc<str>, #[default] accent: bool, #[default] selected: bool) -> impl Component {
    let theme = Theme::current().expect("theme installed");
    let panel = theme.get::<Brush>("tile.panel");
    let accent_brush = theme.get::<Brush>("tile.accent");
    let scale = accent.pick(
        theme.get::<f32>("tile.scale.accent"),
        theme.get::<f32>("tile.scale.panel"),
    );
    center()
        .style(style! {
            background: {accent.pick(accent_brush, panel)};
            transform: {scale.map(|scale| Transform(vec![TransformOp::Scale(scale, scale)]))};
            transition: 220ms spring;
        })
        .with_hovered(move |view, hovered| {
            view.opacity(computed(move |_| {
                if accent.get() && hovered.get() {
                    0.65
                } else if selected.get() {
                    0.82
                } else {
                    1.
                }
            }))
        })
        .children(text(caption).style(style! {
            font-size: {responsive::font(22.)};
            line-height: {responsive::ui(32.)};
        }))
}

fn token_examples(selected: Signal<bool>) -> Node {
    stack()
        .children((
            tile("Default"),
            tile("Accent").accent(true),
            tile("Selected accent").accent(true).selected(selected),
        ))
        .into_node()
}

pub fn page() -> Node {
    let dark = dark_theme();
    let selected = signal(true);
    let sample = r#"// Tokens live in the application, not in the component.
let theme = Theme::current().expect("theme installed");

center()
    .background(accent.pick(
        theme.get::<Brush>("tile.accent"),
        theme.get::<Brush>("tile.panel"),
    ))
    .transform(scale.map(|scale| {
        Transform(vec![TransformOp::Scale(scale, scale)])
    }))

// Switching a theme writes new values into the same signals.
toggle_theme();"#;
    stack()
        .style(style! {
            width: 100%; min-width: 0px;
            gap: {responsive::ui(20.)};
        })
        .children((
            button(
                dark.map(|is_dark| format!("Appearance: {}", if is_dark { "Dark" } else { "Light" })),
                toggle_theme,
            ),
            button("Toggle selected state", move || selected.toggle()),
            token_examples(selected),
            text("Local state stays local. Each tile picks between two token handles, and switching the theme rewrites every token in place."),
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
