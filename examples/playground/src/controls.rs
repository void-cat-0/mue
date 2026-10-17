use super::responsive;
use mue_core::prelude::*;
use mue_macroquad::{
    animation::{Animation, with_animation},
    color::Color,
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
};
use std::rc::Rc;

pub const BG: Color = Color::from_rgba(19, 23, 32, 255);
pub const PANEL: Color = Color::from_rgba(33, 42, 57, 255);
pub const BLUE: Color = Color::from_rgba(110, 160, 245, 255);
pub const CORAL: Color = Color::from_rgba(245, 165, 112, 255);
pub const SPRING: Animation = Animation::spring(0.3, 0.1);

pub fn button(label: impl Into<Prop<Rc<str>>>, action: impl FnMut() + 'static) -> Node {
    link_overlay()
        .max_width(percent(100.))
        .min_width(0.)
        .padding((responsive::ui(12.), responsive::ui(7.)))
        .min_height(responsive::ui(44.))
        .radius(responsive::ui(7.))
        .background(PANEL)
        .on_activate(action)
        .children(
            text(label)
                .font_size(responsive::font(16.))
                .line_height(responsive::ui(24.)),
        )
        .into_node()
}
pub fn toggle(label: &'static str) -> (Signal<bool>, Node) {
    let value = signal(false);
    let control = button(
        value.map(move |v| format!("{label}: {}", if v { "on" } else { "off" })),
        move || with_animation(SPRING, || value.toggle()),
    );
    (value, control)
}
pub fn choice<T: Clone + PartialEq + 'static>(
    label: &'static str,
    values: Vec<(&'static str, T)>,
) -> (ReadSignal<T>, Node) {
    let index = signal(0usize);
    let values = Rc::new(values);
    let len = values.len();
    let value = index.map({
        let values = values.clone();
        move |i| values[i].1.clone()
    });
    let control = button(
        index.map(move |i| format!("{label}: {}", values[i].0)),
        move || with_animation(SPRING, || index.update(|i| *i = (*i + 1) % len)),
    );
    (value, control)
}
pub fn controls(nodes: Vec<Node>) -> Node {
    wrap()
        .w_full()
        .min_width(0.)
        .gap(vw(1.))
        .children(nodes)
        .into_node()
}
pub fn section(title: &str, content: impl IntoChildren) -> Node {
    stack()
        .w_full()
        .min_width(0.)
        .gap(vh(1.75))
        .children((
            heading(title)
                .font_size(responsive::font(22.))
                .line_height(responsive::ui(32.)),
            content,
        ))
        .into_node()
}
