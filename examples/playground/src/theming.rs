use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    paint::shader::Brush,
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
            border-radius: {mue_components::system::radius::MD};
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
    watch(*system, |system| set_system_color_scheme(system.get()));
    card(
        "Color mode",
        (
            field("Preference", picker(modes.to_vec(), color.mode)),
            field(
                "System scheme",
                picker(
                    vec![("Light", ColorScheme::Light), ("Dark", ColorScheme::Dark)],
                    system,
                ),
            ),
            mono(
                color
                    .scheme
                    .map(|scheme| Rc::<str>::from(format!("resolved {scheme:?}"))),
            ),
        ),
    )
}

fn tokens_card() -> Node {
    let selected = signal(true);
    card(
        "Theme tokens",
        (
            switch().checked(selected).label("Selected").size(mue_components::system::ControlSize::Sm),
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
                            color: {matches.map(|on| Brush::Solid(if on { GREEN } else { mue_macroquad::paint::color::Color::from_rgba(146, 157, 178, 255) }))};
                        },
                    ),
                    mono(query),
                ))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Media queries",
        (
            stack()
                .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: 0px; })
                .children(rows),
            switch()
                .checked(reduced)
                .label("Reduced motion")
                .on_change(|on: &bool| set_prefers_reduced_motion(*on)),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            color_mode_card(),
            tokens_card(),
            media_card(),
            checklist(
                PAGE,
                &[
                    "Light, Dark, and System switch the whole app at once",
                    "Width and orientation queries follow resizing and rotation",
                    "Hover and pointer queries follow the last input device",
                ],
            ),
        ),
    )
}
