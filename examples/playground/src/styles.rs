use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    shader::Brush,
    style::{Styleable, Transform, TransformOp, vh, vw},
};

pub fn page() -> Node {
    let (active, control) = toggle("Accent state");
    stack()
        .style(mue_macroquad::style! {
            width: 100%;
            min-width: 0px;
            gap: {vh(2.5)};
        })
        .children((
            control,
            section(
                "Length and spacing",
                wrap()
                    .style(mue_macroquad::style! { width: 100%; gap: {vw(1.5)}; })
                    .children((
                        center()
                            .style(mue_macroquad::style! {
                                flex-basis: 30%;
                                flex-grow: 1;
                                min-width: 120px;
                                min-height: 88px;
                                padding: 12px 20px;
                                border-radius: 10px;
                                background: {PANEL};
                            })
                            .children(text("Padding, radius and responsive flex basis").style(
                                mue_macroquad::style! {
                                    font-size: {responsive::font(16.)};
                                    line-height: {responsive::ui(23.)};
                                    text-align: center;
                                },
                            )),
                        center()
                            .style(mue_macroquad::style! {
                                flex-basis: 30%;
                                flex-grow: 1;
                                min-width: 120px;
                                min-height: 88px;
                                padding: 8px 16px;
                                border-radius: 10px;
                                background: {Color::from_rgba(57, 68, 86, 255)};
                            })
                            .children(text("Static lengths and percentages").style(
                                mue_macroquad::style! {
                                    font-size: {responsive::font(16.)};
                                    line-height: {responsive::ui(23.)};
                                    text-align: center;
                                },
                            )),
                    )),
            ),
            section(
                "Reactive declarations",
                flexbox()
                    .style(mue_macroquad::style! {
                        width: 100%;
                        min-width: 0px;
                        min-height: 160px;
                        padding: 20px 24px;
                        gap: 16px;
                        border-radius: 14px;
                        background: {active.map(|on| Brush::Solid(if on { CORAL } else { BLUE }))};
                        transform: {computed(move |_| Transform(vec![TransformOp::Rotate(
                            if active.get() { 0.035 } else { 0. },
                        )]))};
                        transition: 320ms spring 0.35;
                    })
                    .children((
                        heading(active.select("Reactive styles", "Static structure"))
                            .style(mue_macroquad::style! {
                                font-size: {responsive::font(24.)};
                                line-height: {responsive::ui(32.)};
                                color: {BG};
                            }),
                        text("Values in braces are Rust expressions, so signals and responsive units stay reactive.")
                            .style(mue_macroquad::style! {
                                max-width: 620px;
                                font-size: {responsive::font(16.)};
                                line-height: {responsive::ui(24.)};
                                color: {BG};
                            }),
                    )),
            ),
            section(
                "Text properties",
                text("Readable text can use named weights, families, decorations and line-height values.")
                    .style(mue_macroquad::style! {
                        width: 100%;
                        padding: 16px 20px;
                        border-radius: 8px;
                        background: {PANEL};
                        color: {CORAL};
                        font-size: {responsive::font(18.)};
                        font-weight: 600;
                        font-family: monospace;
                        text-decoration: underline;
                        line-height: 1.5;
                    }),
            ),
        ))
        .into_node()
}
