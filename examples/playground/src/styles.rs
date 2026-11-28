use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    paint::color::Color,
    paint::shader::Brush,
    style::{Styleable, Transform, TransformOp},
};
use std::f32::consts::TAU;

const PAGE: &str = "Style declarations";

thread_local! {
    static ANIMATION_TIME: Signal<f32> = Scope::null().run(|| signal(0.));
}

pub fn update_animation_time(time: f32) {
    ANIMATION_TIME.with(|value| value.set(time));
}

// Separate construction frames keep the style builders' debug temporaries
// from exhausting the default Windows main-thread stack.
fn lengths_card() -> Node {
    let tile = |label: &'static str, style: mue_macroquad::style::Style| {
        center()
            .style(mue_macroquad::style! {
                flex-basis: 30%;
                flex-grow: 1;
                min-width: 140px;
                min-height: 80px;
                border-radius: 10px;
            })
            .style(style)
            .children(text(label).style(mue_macroquad::style! {
                font-size: 14px; line-height: 20px;
                text-align: center;
                color: {foreground()};
            }))
            .into_node()
    };
    card(
        "Lengths and spacing",
        "Static px values, percentages, and design-unit bindings in braces.",
        row((
            tile(
                "padding: 12px 20px",
                mue_macroquad::style! { padding: 12px 20px; background: {raised()}; },
            ),
            tile(
                "width: 45%",
                mue_macroquad::style! { width: 45%; flex-grow: 0; background: {raised()}; },
            ),
            tile(
                "padding: {u(24.)}",
                mue_macroquad::style! { padding: {(u(24.),)}; background: {raised()}; },
            ),
        )),
    )
}

fn reactive_card() -> Node {
    let (active, control) = option_switch("Accent state", false);
    card(
        "Reactive declarations",
        "Values in braces are Rust expressions; signals inside them keep the style live, and transitions animate the change.",
        (
            control,
            stack()
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    padding: 20px 24px;
                    gap: 8px;
                    border-radius: 14px;
                    background: {active.map(|on| Brush::Solid(if on { CORAL } else { BLUE }))};
                    transform: {computed(move |_| Transform(vec![TransformOp::Rotate(
                        if active.get() { 0.035 } else { 0. },
                    )]))};
                    transition: 320ms spring 0.35;
                })
                .children((
                    heading(active.select("Reactive styles", "Static structure")).style(
                        mue_macroquad::style! {
                            font-size: 22px; line-height: 30px;
                            color: #131720;
                        },
                    ),
                    text("background: {active.map(..)}; transform: {computed(..)}; transition: 320ms spring 0.35;")
                        .style(mue_macroquad::style! {
                            font-size: 13px; line-height: 19px;
                            font-family: monospace;
                            color: #131720;
                        }),
                )),
        ),
    )
}

fn text_card() -> Node {
    card(
        "Text declarations",
        "Named weights, families, decorations, and a unitless line height.",
        text("font-weight: 600; font-family: monospace; text-decoration: underline; line-height: 1.5;")
            .style(mue_macroquad::style! {
                width: 100%;
                padding: 16px 20px;
                border-radius: 8px;
                background: {raised()};
                color: {CORAL};
                font-size: 16px;
                font-weight: 600;
                font-family: monospace;
                text-decoration: underline;
                line-height: 1.5;
            }),
    )
}

fn transform_card() -> Node {
    let (spin, control) = option_switch("Animate", true);
    let time = ANIMATION_TIME.with(|time| **time);
    card(
        "3D transforms",
        "perspective, rotateX, rotateY, translateZ, and rotate driven by a clock signal.",
        (
            control,
            center()
                .style(mue_macroquad::style! { width: 100%; height: 200px; })
                .children(
                    center()
                        .style(mue_macroquad::style! {
                            width: 60%;
                            height: 140px;
                            border-radius: 12px;
                            background: {Color::from_rgba(80, 120, 220, 255)};
                            transform: {computed(move |_| {
                                let t = if spin.get() { time.get() } else { 0. };
                                Transform(vec![
                                    TransformOp::Perspective(900.),
                                    TransformOp::RotateX((t * TAU * 0.07).sin() * 0.35),
                                    TransformOp::RotateY((t * TAU * 0.11).sin() * 0.45),
                                    TransformOp::Rotate((t * TAU * 0.05).sin() * 0.12),
                                    TransformOp::TranslateZ(8.),
                                ])
                            })};
                        })
                        .children(text("perspective(900px) rotateX rotateY").style(
                            mue_macroquad::style! {
                                font-size: 15px; line-height: 22px;
                                color: #ffffff;
                            },
                        )),
                ),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        "The style! macro: static values, bindings, reactive values, text, and transforms.",
        (
            lengths_card(),
            reactive_card(),
            text_card(),
            transform_card(),
            checklist(
                PAGE,
                &[
                    "The accent switch animates color and rotation together",
                    "The 3D card rotates in perspective without tearing",
                ],
            ),
        ),
    )
}
