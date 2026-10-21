use super::responsive;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    style::{Styleable, StyleableExt, vh, vw},
    transition::Transition,
};
use std::rc::Rc;

pub const BG: Color = Color::from_rgba(19, 23, 32, 255);
pub const PANEL: Color = Color::from_rgba(33, 42, 57, 255);
pub const BLUE: Color = Color::from_rgba(110, 160, 245, 255);
pub const CORAL: Color = Color::from_rgba(245, 165, 112, 255);
pub const SPRING: Transition = Transition::spring(0.3, 0.1);

pub fn button(label: impl Into<Prop<Rc<str>>>, action: impl FnMut() + 'static) -> Node {
    link_overlay()
        .style(mue_macroquad::style! {
        max-width: 100%;
        min-width: 0px;
        padding: {(responsive::ui(12.), responsive::ui(7.))};
        min-height: {responsive::ui(44.)};
        border-radius: {responsive::ui(7.)};
        background: {PANEL};
        })
        .on_activate(action)
        .children(text(label).style(mue_macroquad::style! {
        font-size: {responsive::font(16.)};
        line-height: {responsive::ui(24.)};
        }))
        .into_node()
}
pub fn toggle(label: &'static str) -> (Signal<bool>, Node) {
    let value = signal(false);
    let control = button(
        value.map(move |v| format!("{label}: {}", if v { "on" } else { "off" })),
        move || value.toggle(),
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
        move || index.update(|i| *i = (*i + 1) % len),
    );
    (value, control)
}
pub fn controls(nodes: Vec<Node>) -> Node {
    wrap()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vw(1.)};
        })
        .children(nodes)
        .into_node()
}
pub fn section(title: &str, content: impl IntoChildren) -> Node {
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(1.75)};
        })
        .children((
            heading(title).style(mue_macroquad::style! {
            font-size: {responsive::font(22.)};
            line-height: {responsive::ui(32.)};
            }),
            content,
        ))
        .into_node()
}
