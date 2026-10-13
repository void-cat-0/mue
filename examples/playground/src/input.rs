use super::controls::*;
use mue_core::prelude::*;
use mue_macroquad::{
    node::*,
    style::{Styleable, StyleableExt},
};

pub fn page() -> Node {
    let count = signal(0);
    let child = signal(0);
    let pressed = signal(false);
    let hover = signal(false);
    let held = signal(0);
    let pan = signal(String::from("Pan"));
    let swipe = signal(String::from("Swipe"));
    let pinch = signal(1.);
    let rotate = signal(0.);
    let panel = |label: &str| {
        center()
            .width(280.)
            .height(150.)
            .background(PANEL)
            .children(text(label).font_size(22.))
    };
    stack()
        .gap(20.)
        .children((
            link_overlay()
                .padding((20.,))
                .radius(12.)
                .background(PANEL)
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
                    .font_size(20.),
                    link(child.map(|n| format!("Child {n}")))
                        .font_size(20.)
                        .on_activate(move || child.update(|n| *n += 1)),
                )),
            wrap().gap(12.).children((
                panel("Pan").on_pan(move |e| {
                    pan.set(format!(
                        "{:?}: {:.0}, {:.0}",
                        e.phase, e.translation.x, e.translation.y
                    ))
                }),
                panel("Swipe").on_swipe(move |e| swipe.set(format!("{:?}", e.direction))),
                panel("Pinch").on_pinch(move |e| pinch.set(e.scale)),
                panel("Rotate").on_rotate(move |e| rotate.set(e.rotation)),
            )),
            text(computed(move |_| {
                format!(
                    "{}\n{}\n{:.2}x · {:.0}°",
                    pan.get_clone(),
                    swipe.get_clone(),
                    pinch.get(),
                    rotate.get().to_degrees()
                )
            }))
            .font_size(20.),
        ))
        .into_node()
}
