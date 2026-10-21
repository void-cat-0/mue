use super::responsive;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    shader::Brush,
    style::{Styleable, StyleableExt, vh, vw},
    transition::Transition,
};
use std::rc::Rc;

thread_local! {
    static DARK_THEME: Signal<bool> = Scope::null().run(|| signal(true));
}

pub const BLUE: Color = Color::from_rgba(110, 160, 245, 255);
pub const CORAL: Color = Color::from_rgba(245, 165, 112, 255);
pub const SPRING: Transition = Transition::spring(0.3, 0.1);

pub fn dark_theme() -> ReadSignal<bool> {
    DARK_THEME.with(|dark| dark.map(|value| value))
}

pub fn toggle_theme() {
    DARK_THEME.with(|dark| dark.toggle());
}

fn themed_brush(dark: impl Fn(bool) -> Color + 'static) -> Prop<Brush> {
    DARK_THEME
        .with(|theme| theme.map(move |is_dark| Brush::Solid(dark(is_dark))))
        .into()
}

pub fn background() -> Prop<Brush> {
    themed_brush(|dark| {
        if dark {
            Color::from_rgba(19, 23, 32, 255)
        } else {
            Color::from_rgba(242, 245, 250, 255)
        }
    })
}

pub fn panel_brush() -> Prop<Brush> {
    themed_brush(|dark| {
        if dark {
            Color::from_rgba(33, 42, 57, 255)
        } else {
            Color::from_rgba(255, 255, 255, 255)
        }
    })
}

pub fn foreground() -> Prop<Brush> {
    themed_brush(|dark| {
        if dark {
            Color::from_rgba(240, 244, 250, 255)
        } else {
            Color::from_rgba(31, 41, 55, 255)
        }
    })
}

pub fn background_color() -> Color {
    if DARK_THEME.with(|theme| theme.get()) {
        Color::from_rgba(19, 23, 32, 255)
    } else {
        Color::from_rgba(242, 245, 250, 255)
    }
}

pub fn button(label: impl Into<Prop<Rc<str>>>, action: impl FnMut() + 'static) -> Node {
    sized_button(label, action, 16.)
}

pub fn tab_button(title: &'static str, selected: Signal<usize>, index: usize) -> Node {
    let active = selected.map(move |page| page == index);
    let panel = panel_brush();
    let foreground = foreground();
    link_overlay()
        .style(mue_macroquad::style! {
            flex: none;
            padding: {(responsive::ui(12.), responsive::ui(7.))};
            min-height: {responsive::ui(44.)};
            border-radius: {responsive::ui(7.)};
            background: {computed(move |_| if active.get() { Brush::Solid(BLUE) } else { panel.get_clone() })};
            align-items: center;
            justify-content: center;
        })
        .on_activate(move || selected.set(index))
        .children(text(title).style(mue_macroquad::style! {
            font-size: {responsive::font(14.)};
            line-height: {responsive::ui(24.)};
            color: {computed(move |_| if active.get() { Brush::Solid(Color::from_rgba(19, 23, 32, 255)) } else { foreground.get_clone() })};
            text-wrap: {false};
        }))
        .into_node()
}

fn sized_button(
    label: impl Into<Prop<Rc<str>>>,
    action: impl FnMut() + 'static,
    font_size: f32,
) -> Node {
    link_overlay()
        .style(mue_macroquad::style! {
        max-width: 100%;
        min-width: 0px;
        flex: none;
        padding: {(responsive::ui(12.), responsive::ui(7.))};
        min-height: {responsive::ui(44.)};
        border-radius: {responsive::ui(7.)};
        background: {panel_brush()};
        align-items: center;
        justify-content: center;
        })
        .on_activate(action)
        .children(text(label).style(mue_macroquad::style! {
        font-size: {responsive::font(font_size)};
        line-height: {responsive::ui(24.)};
        text-wrap: {false};
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
