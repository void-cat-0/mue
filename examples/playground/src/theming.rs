use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    shader::Brush,
    style::{Styleable, StyleableExt, Transform, TransformOp},
};
use std::rc::Rc;

const PAGE: &str = "Theme & media";

// Only the component's own state lives here; the theme is application tokens.
#[component]
fn tile(caption: Rc<str>, #[default] accent: bool, #[default] selected: bool) -> impl Component {
    let theme = Theme::current().expect("theme installed");
    let scale = accent.pick(
        theme.get::<f32>("tile.scale.accent"),
        theme.get::<f32>("tile.scale.panel"),
    );
    center()
        .style(style! {
            flex: 1;
            min-width: {u(120.)};
            height: {u(64.)};
            border-radius: {u(10.)};
            background: {accent.pick(token!("tile.accent"), token!("tile.panel"))};
            transform: {scale.map(|scale| Transform(vec![TransformOp::Scale(scale, scale)]))};
            transition: 220ms spring;
        })
        .with_hovered(move |view, hovered| {
            view.opacity(computed(move |_| {
                if accent.get() && hovered.get() {
                    0.7
                } else if selected.get() {
                    0.85
                } else {
                    1.
                }
            }))
        })
        .children(text(caption).style(style! {
            font-size: {f(14.)};
            line-height: {u(20.)};
            color: {foreground()};
        }))
}

fn color_mode_card() -> Node {
    let color = use_color_mode();
    let modes = [
        ("Light", ColorMode::Light),
        ("Dark", ColorMode::Dark),
        ("System", ColorMode::System),
    ];
    let system = signal(ColorScheme::Dark);
    card(
        "Color mode",
        "use_color_mode: a light, dark, or system preference and the resolved scheme. Hosts report the system scheme; here you can simulate it.",
        (
            field(
                "Preference",
                row(modes
                    .into_iter()
                    .map(|(label, mode)| {
                        chip(
                            label,
                            color.mode.map(move |current| current == mode),
                            move || color.mode.set(mode),
                        )
                    })
                    .collect::<Vec<_>>()),
            ),
            field(
                "System scheme",
                row((
                    chip(
                        "Light",
                        system.map(|s| s == ColorScheme::Light),
                        move || {
                            system.set(ColorScheme::Light);
                            set_system_color_scheme(ColorScheme::Light);
                        },
                    ),
                    chip("Dark", system.map(|s| s == ColorScheme::Dark), move || {
                        system.set(ColorScheme::Dark);
                        set_system_color_scheme(ColorScheme::Dark);
                    }),
                )),
            ),
            mono(
                color
                    .scheme
                    .map(|scheme| Rc::<str>::from(format!("resolved scheme: {scheme:?}"))),
            ),
        ),
    )
}

fn tokens_card() -> Node {
    let selected = signal(true);
    card(
        "Theme tokens",
        "Each tile picks between two token handles. Switching the scheme rewrites every token in place, so no component rebuilds.",
        (
            row((button("Toggle selected").on_activate(move || selected.toggle()),)),
            row((
                tile("Default"),
                tile("Accent").accent(true),
                tile("Selected accent").accent(true).selected(selected),
            )),
            code_block(
                "background: {accent.pick(token!(\"tile.accent\"), token!(\"tile.panel\"))};\n// Switching a theme writes new values into the same signals.\ntheme.apply(&dark_palette);",
            )
            .style(mue_macroquad::style! {
                width: 100%; min-width: 0px;
                text-wrap: {true};
                font-size: {f(13.)}; line-height: {u(19.)};
                padding: {(u(12.),)};
                border-radius: {u(6.)};
            }),
        ),
    )
}

fn media_card() -> Node {
    let queries = [
        "(min-width: 640px)",
        "(min-width: 1024px)",
        "(orientation: portrait)",
        "(orientation: landscape)",
        "(hover: hover)",
        "(pointer: coarse)",
        "(prefers-color-scheme: dark)",
        "(prefers-reduced-motion: reduce)",
        "(min-width: 640px) and (orientation: landscape)",
        "(max-width: 480px), (orientation: portrait)",
    ];
    let reduced = signal(false);
    let rows = queries
        .into_iter()
        .map(|query| {
            let matches = use_media_query(query);
            hstack()
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    gap: {u(10.)};
                    padding: {(u(8.), u(4.))};
                    align-items: center;
                })
                .children((
                    text(matches.map(|on| Rc::<str>::from(if on { "✓" } else { "·" }))).style(
                        mue_macroquad::style! {
                            flex: none; width: {u(16.)};
                            font-size: {f(15.)}; line-height: {u(20.)};
                            font-family: monospace;
                            color: {matches.map(|on| Brush::Solid(if on { GREEN } else { mue_macroquad::color::Color::from_rgba(146, 157, 178, 255) }))};
                        },
                    ),
                    mono(query),
                ))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Media queries",
        "use_media_query evaluates CSS media queries against the window and input. Resize, rotate, or switch between mouse and touch.",
        (
            stack()
                .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: 0px; })
                .children(rows),
            field(
                "Reduced motion",
                chip(
                    reduced.map(|on| {
                        Rc::<str>::from(if on {
                            "Simulated: reduce"
                        } else {
                            "Simulated: no preference"
                        })
                    }),
                    reduced,
                    move || {
                        reduced.toggle();
                        set_prefers_reduced_motion(reduced.get_untracked());
                    },
                ),
            ),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        "Color modes, application tokens, and media queries.",
        (
            color_mode_card(),
            tokens_card(),
            media_card(),
            checklist(
                PAGE,
                &[
                    "Light, Dark, and System switch the whole app at once",
                    "Width and orientation queries change when resizing or rotating",
                    "Hover and pointer queries follow the last input device",
                ],
            ),
        ),
    )
}
