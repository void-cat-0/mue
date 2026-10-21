use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::style::{Styleable, StyleableExt, vh, vw};

pub fn page() -> Node {
    let count = signal(0);
    let child = signal(0);
    let pressed = signal(false);
    let hover = signal(false);
    let held = signal(0);
    let swipe = signal(String::from("Swipe"));
    let pinch = signal(1.);
    let rotate = signal(0.);
    let panel = |label: &str| {
        center()
            .style(mue_macroquad::style! {
            flex-basis: 40%;
            flex: 1;
            min-width: 0px;
            height: {vh(18.75)};
            min-height: {responsive::ui(64.)};
            background: {panel_brush()};
            })
            .children(text(label).style(mue_macroquad::style! {
            font-size: {responsive::font(22.)};
            line-height: {responsive::ui(32.)};
            }))
    };
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(2.5)};
        })
        .children((
            link_overlay()
                .style(mue_macroquad::style! {
                width: 100%;
                min-width: 0px;
                padding: {(vw(2.5), vh(2.5))};
                border-radius: {12.};
                background: {panel_brush()};
                })
                .use_pressed(pressed)
                .use_hovered(hover)
                .on_activate(move || count.update(|n| *n += 1))
                .on_long_press(move |_| held.update(|n| *n += 1))
                .children((
                    text(computed(move |_| {
                        format!(
                            "Tap {} · hold {} · pressed {} · hover {}",
                            count.get(),
                            held.get(),
                            pressed.get(),
                            hover.get()
                        )
                    }))
                    .style(mue_macroquad::style! {
                    font-size: {responsive::font(20.)};
                    line-height: {responsive::ui(28.)};
                    }),
                    link(child.map(|n| format!("Child {n}")))
                        .style(mue_macroquad::style! {
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(28.)};
                        })
                        .on_activate(move || child.update(|n| *n += 1)),
                )),
            wrap()
                .style(mue_macroquad::style! {
                width: 100%;
                gap: {vw(1.5)};
                })
                .children((
                    panel("Swipe").on_swipe(move |e| swipe.set(format!("{:?}", e.direction))),
                    panel("Pinch").on_pinch(move |e| pinch.set(e.scale)),
                    panel("Rotate").on_rotate(move |e| rotate.set(e.rotation)),
                )),
            text(computed(move |_| {
                format!(
                    "{}\n{}\n{:.2}x · {:.0}°",
                    "Use the Map gestures tab to pan the image",
                    swipe.get_clone(),
                    pinch.get(),
                    rotate.get().to_degrees()
                )
            }))
            .style(mue_macroquad::style! {
            font-size: {responsive::font(20.)};
            line-height: {responsive::ui(28.)};
            }),
        ))
        .into_node()
}

pub fn map_page() -> Node {
    use macroquad::texture::Texture2D;
    use mue_macroquad::{
        gesture::GesturePhase,
        math::Vector,
        shader::SharedTexture,
        style::{Size, Transform, TransformOp},
    };

    let deer: SharedTexture =
        Texture2D::from_file_with_format(include_bytes!("../../../deer.png"), None).into();
    let offset = signal(Vector::ZERO);
    let scale = signal(1.);
    let rotation = signal(0.);
    let gesture_scale = signal(1.);
    let gesture_rotation = signal(0.);
    let (animate, animate_control) = toggle("Animate transform");
    animate.set(true);
    let map_transition = animate.map(|enabled| {
        if enabled {
            mue_macroquad::transition::Transition::ease_out(0.2)
        } else {
            mue_macroquad::transition::Transition::Instant
        }
    });
    let map_transform = computed(move |_| {
        let offset = offset.get();
        let scale = scale.get();
        Transform(vec![
            TransformOp::Translate(Size::Px(offset.x), Size::Px(offset.y)),
            TransformOp::Rotate(rotation.get()),
            TransformOp::Scale(scale, scale),
        ])
    });
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(2.5)};
        })
        .children((
            text("Scroll the page with one finger. Use two fingers to pan, pinch, or rotate the deer.").style(
                mue_macroquad::style! {
                font-size: {responsive::font(18.)};
                line-height: {responsive::ui(26.)};
                },
            ),
            scroll_area()
                .style(mue_macroquad::style! {
                width: 100%;
                height: {vh(58.)};
                min-height: {responsive::ui(260.)};
                background: {panel_brush()};
                })
                .options(ScrollOptions {
                    touch: false,
                    wheel: false,
                    scrollbars: false,
                    chain: false,
                    ..Default::default()
                })
                .content_style(mue_macroquad::style! {
                    width: 100%;
                    height: 100%;
                    align-items: center;
                    justify-content: center;
                })
                .on_transform(move |event| {
                    if event.phase == GesturePhase::Start {
                        gesture_scale.set(scale.get());
                        gesture_rotation.set(rotation.get());
                    }
                    if matches!(
                        event.phase,
                        GesturePhase::Start | GesturePhase::Update | GesturePhase::End
                    ) {
                        scale.set((gesture_scale.get() * event.scale).clamp(0.5, 4.));
                        rotation.set(gesture_rotation.get() + event.rotation);
                        if event.phase != GesturePhase::Start {
                            offset.update(|value| *value += event.translation_delta);
                        }
                    }
                })
                .children(
                    image(deer)
                        .object_contain()
                        .style(mue_macroquad::style! {
                        width: 70%;
                        height: 70%;
                        flex: none;
                        transition: {map_transition};
                        })
                        .transform_list(map_transform),
                ),
            wrap()
                .style(mue_macroquad::style! {
                    width: 100%;
                    gap: {vw(1.)};
                })
                .children((
                    animate_control,
                    button("Center image", move || offset.set(Vector::ZERO)),
                    button("Reset transform", move || {
                        offset.set(Vector::ZERO);
                        scale.set(1.);
                        rotation.set(0.);
                    }),
                )),
            text(computed(move |_| {
                format!(
                    "Scale: {:.2}x  |  Rotation: {:.0}°",
                    scale.get(),
                    rotation.get().to_degrees()
                )
            }))
            .style(mue_macroquad::style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(26.)};
            }),
        ))
        .into_node()
}
