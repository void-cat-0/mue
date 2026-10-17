use super::{controls::*, responsive};
use mue_core::prelude::*;
use mue_macroquad::{
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
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
            .flex_basis(percent(40.))
            .flex_1()
            .min_width(0.)
            .height(vh(18.75))
            .min_height(responsive::ui(64.))
            .background(PANEL)
            .children(
                text(label)
                    .font_size(responsive::font(22.))
                    .line_height(responsive::ui(32.)),
            )
    };
    stack()
        .w_full()
        .min_width(0.)
        .gap(vh(2.5))
        .children((
            link_overlay()
                .w_full()
                .min_width(0.)
                .padding((vw(2.5), vh(2.5)))
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
                    .font_size(responsive::font(20.))
                    .line_height(responsive::ui(28.)),
                    link(child.map(|n| format!("Child {n}")))
                        .font_size(responsive::font(20.))
                        .line_height(responsive::ui(28.))
                        .on_activate(move || child.update(|n| *n += 1)),
                )),
            wrap().w_full().gap(vw(1.5)).children((
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
            .font_size(responsive::font(20.))
            .line_height(responsive::ui(28.)),
        ))
        .into_node()
}
