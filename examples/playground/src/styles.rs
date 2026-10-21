use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    shader::Brush,
    style::{Styleable, Transform, TransformOp, vh, vw},
};
use std::f32::consts::TAU;

thread_local! {
    static ANIMATION_TIME: Signal<f32> = Scope::null().run(|| signal(0.));
}

pub fn update_animation_time(time: f32) {
    ANIMATION_TIME.with(|value| value.set(time));
}

pub fn page() -> Node {
    let (active, control) = toggle("Accent state");
    let (three_d, three_d_control) = toggle("3D projection");
    let animation_time = ANIMATION_TIME.with(|time| time.map(|value| value));
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
                                background: {panel_brush()};
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
                                color: {foreground()};
                            }),
                        text("Values in braces are Rust expressions, so signals and responsive units stay reactive.")
                            .style(mue_macroquad::style! {
                                max-width: 620px;
                                font-size: {responsive::font(16.)};
                                line-height: {responsive::ui(24.)};
                                color: {foreground()};
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
                        background: {panel_brush()};
                        color: {CORAL};
                        font-size: {responsive::font(18.)};
                        font-weight: 600;
                        font-family: monospace;
                        text-decoration: underline;
                        line-height: 1.5;
                    }),
            ),
            section(
                "3D transforms",
                stack()
                    .style(mue_macroquad::style! {
                        width: 100%;
                        min-width: 0px;
                        gap: {responsive::ui(12.)};
                    })
                    .children((
                        three_d_control,
                        code("perspective(520px) rotateX(14deg) rotateY(-12deg) translate3d(0, 0, 18px) scale3d(1.05, 1.05, 1)")
                            .style(mue_macroquad::style! {
                                font-size: {responsive::font(15.)};
                                line-height: {responsive::ui(22.)};
                            }),
                        center()
                            .style(mue_macroquad::style! {
                                width: 100%;
                                height: {vh(24.)};
                                min-height: {responsive::ui(120.)};
                                border-radius: 12px;
                                background: {panel_brush()};
                            })
                            .children(text("Two-dimensional view").style(
                                mue_macroquad::style! {
                                    font-size: {responsive::font(22.)};
                                    line-height: {responsive::ui(32.)};
                                },
                            ))
                            .show_if(three_d.map(|enabled| !enabled)),
                        center()
                            .style(mue_macroquad::style! {
                                width: 100%;
                                height: {vh(24.)};
                                min-height: {responsive::ui(120.)};
                                border-radius: 12px;
                                background: {panel_brush()};
                                transform: {computed(move |_| Transform(vec![
                                    TransformOp::Perspective(1100.),
                                    TransformOp::RotateX(if three_d.get() { (animation_time.get() * TAU * 0.07).sin() * 0.2 } else { 0. }),
                                    TransformOp::RotateY(if three_d.get() { (animation_time.get() * TAU * 0.11).sin() * 0.24 } else { 0. }),
                                    TransformOp::Rotate(if three_d.get() { (animation_time.get() * TAU * 0.05).sin() * 0.12 } else { 0. }),
                                    TransformOp::TranslateZ(8.),
                                ]))};
                                transition: 60ms linear;
                            })
                            .children(text("Projected 3D view").style(
                                mue_macroquad::style! {
                                    font-size: {responsive::font(22.)};
                                    line-height: {responsive::ui(32.)};
                                },
                            ))
                            .show_if(three_d),
                    )),
            ),
        ))
        .into_node()
}
