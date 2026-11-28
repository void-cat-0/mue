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

/// Seconds since start, updated once per frame (tracked).
pub fn animation_time() -> ReadSignal<f32> {
    ANIMATION_TIME.with(|time| **time)
}

// Separate construction frames keep the style builders' debug temporaries
// from exhausting the default Windows main-thread stack.
fn lengths_card() -> Node {
    let tile = |label: &'static str, style: mue_macroquad::style::Style| {
        center()
            .style(mue_macroquad::style! {
                flex-basis: 30%;
                flex-grow: 1;
                min-width: {u(140.)};
                min-height: {u(80.)};
                border-radius: {u(10.)};
                border-width: 1px;
                border-color: {border()};
                background: {raised()};
            })
            .style(style)
            .children(text(label).style(mue_macroquad::style! {
                font-size: {f(13.)}; line-height: {u(19.)};
                font-family: monospace;
                text-align: center;
                color: {foreground()};
            }))
            .into_node()
    };
    card(
        "Lengths",
        row((
            tile("padding: 12px 20px;", mue_macroquad::style! { padding: 12px 20px; }),
            tile("width: 45%;", mue_macroquad::style! { width: 45%; flex-grow: 0; }),
            tile("padding: {(u(24.),)};", mue_macroquad::style! { padding: {(u(24.),)}; }),
            tile("height: 4rem;", mue_macroquad::style! { height: 4rem; min-height: 0px; }),
        )),
    )
}

fn paint_card() -> Node {
    let swatch = |code: &'static str, style: mue_macroquad::style::Style| {
        stack()
            .style(mue_macroquad::style! {
                flex-basis: 30%;
                flex-grow: 1;
                min-width: {u(180.)};
                gap: {u(10.)};
                align-items: center;
            })
            .children((
                flexbox()
                    .style(mue_macroquad::style! {
                        width: {u(120.)}; height: {u(72.)};
                        background: {ACCENT_B};
                    })
                    .style(style),
                text(code).style(mue_macroquad::style! {
                    width: 100%;
                    font-size: {f(12.)}; line-height: {u(17.)};
                    font-family: monospace;
                    text-align: center;
                    color: {muted()};
                }),
            ))
            .into_node()
    };
    card(
        "Paint declarations",
        row((
            swatch(
                "border: 3px solid #ffd36e;\nborder-radius: 18px 4px;",
                mue_macroquad::style! { border: 3px solid #ffd36e; border-radius: 18px 4px; },
            ),
            swatch(
                "border-radius: 24px;\ncorner-shape: squircle;",
                mue_macroquad::style! { border-radius: 24px; corner-shape: squircle; },
            ),
            swatch(
                "outline: 2px #7fd1a8;\noutline-offset: 4px;",
                mue_macroquad::style! { border-radius: 12px; outline: 2px #7fd1a8; outline-offset: 4px; },
            ),
            swatch(
                "box-shadow: 0 10px 24px\n  rgba(0, 0, 0, 0.45);",
                mue_macroquad::style! { border-radius: 12px; box-shadow: 0 10px 24px rgba(0, 0, 0, 0.45); },
            ),
            swatch(
                "box-shadow: 0 0 22px 2px\n  rgba(124, 92, 255, 0.9);",
                mue_macroquad::style! { border-radius: 12px; box-shadow: 0 0 22px 2px rgba(124, 92, 255, 0.9); },
            ),
            swatch(
                "opacity: 40%;\ntransform: rotate(-6deg);",
                mue_macroquad::style! { border-radius: 12px; opacity: 40%; transform: rotate(-6deg); },
            ),
        )),
    )
}

fn reactive_card() -> Node {
    let (active, control) = option_switch("Accent", false);
    card(
        "Reactive declarations",
        (
            control,
            stack()
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    padding: {(u(24.), u(20.))};
                    gap: {u(8.)};
                    border-radius: {super::responsive::ui(active.select(28., 14.))};
                    background: {active.map(|on| Brush::Solid(if on { CORAL } else { ACCENT_B }))};
                    transform: {computed(move |_| Transform(vec![TransformOp::Rotate(
                        if active.get() { 0.035 } else { 0. },
                    )]))};
                    transition: 420ms spring 0.35;
                })
                .children((
                    heading(active.select("Reactive styles", "Static structure")).style(
                        mue_macroquad::style! {
                            font-size: {f(22.)}; line-height: {u(30.)};
                            color: #131720;
                        },
                    ),
                    text("background: {active.map(..)};\ntransform: {computed(..)};\ntransition: 420ms spring 0.35;")
                        .style(mue_macroquad::style! {
                            font-size: {f(13.)}; line-height: {u(19.)};
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
        text("font-weight: 600; font-family: monospace;\ntext-decoration: underline; line-height: 1.5;")
            .style(mue_macroquad::style! {
                width: 100%;
                padding: {(u(20.), u(16.))};
                border-radius: {u(10.)};
                background: {raised()};
                color: {CORAL};
                font-size: {f(15.)};
                font-weight: 600;
                font-family: monospace;
                text-decoration: underline;
                line-height: 1.5;
            }),
    )
}

fn transform_card() -> Node {
    let (spin, control) = option_switch("Animate", true);
    let time = animation_time();
    card(
        "3D transforms",
        (
            control,
            center()
                .style(mue_macroquad::style! { width: 100%; height: {u(220.)}; })
                .children(
                    center()
                        .style(mue_macroquad::style! {
                            width: 60%;
                            height: {u(140.)};
                            border-radius: {u(16.)};
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
                        .background(accent_gradient())
                        .box_shadow(mue_macroquad::BoxShadow::new(0., 12., 32., Color::new(0.3, 0.2, 0.9, 0.45)))
                        .children(text("perspective(900px) rotateX rotateY").style(
                            mue_macroquad::style! {
                                font-size: {f(15.)}; line-height: {u(22.)};
                                color: #ffffff;
                            },
                        )),
                ),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            lengths_card(),
            paint_card(),
            reactive_card(),
            text_card(),
            transform_card(),
            checklist(
                PAGE,
                &[
                    "Literal lengths stay fixed, design units follow the zoom",
                    "Corner shapes, outlines, and shadows match their declarations",
                    "Accent animates color, radius, and rotation together",
                    "The 3D card rotates in perspective without tearing",
                ],
            ),
        ),
    )
}
