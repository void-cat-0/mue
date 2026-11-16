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

/// Accent colors that stay the same in every theme.
pub const BLUE: Color = Color::from_rgba(110, 160, 245, 255);
pub const CORAL: Color = Color::from_rgba(245, 165, 112, 255);
pub const SPRING: Transition = Transition::spring(0.3, 0.1);

const CANVAS_LIGHT: Color = Color::from_rgba(242, 245, 250, 255);
const CANVAS_DARK: Color = Color::from_rgba(19, 23, 32, 255);
const PANEL_DARK: Color = Color::from_rgba(33, 42, 57, 255);
const FOREGROUND_LIGHT: Color = Color::from_rgba(31, 41, 55, 255);
const FOREGROUND_DARK: Color = Color::from_rgba(240, 244, 250, 255);

/// One theme: the same token names, different values.
struct Palette {
    brushes: TokenSet<Brush>,
    colors: TokenSet<Color>,
    numbers: TokenSet<f32>,
}

impl Palette {
    fn register(&self, theme: &Theme) {
        theme.register(&self.brushes);
        theme.register(&self.colors);
        theme.register(&self.numbers);
    }

    fn apply(&self, theme: &Theme) {
        theme.apply(&self.brushes);
        theme.apply(&self.colors);
        theme.apply(&self.numbers);
    }
}

fn light_palette() -> Palette {
    Palette {
        brushes: token_set! {
            canvas: Brush::Solid(CANVAS_LIGHT),
            panel: Brush::Solid(Color::WHITE),
            foreground: Brush::Solid(FOREGROUND_LIGHT),
            "tile.panel": Brush::Solid(BLUE),
            "tile.accent": Brush::Solid(CORAL),
        },
        colors: token_set! { canvas: CANVAS_LIGHT },
        numbers: token_set! {
            "tile.scale.panel": 1.02_f32,
            "tile.scale.accent": 1.04_f32,
        },
    }
}

fn dark_palette() -> Palette {
    Palette {
        brushes: token_set! {
            canvas: Brush::Solid(CANVAS_DARK),
            panel: Brush::Solid(PANEL_DARK),
            foreground: Brush::Solid(FOREGROUND_DARK),
            "tile.panel": Brush::Solid(PANEL_DARK),
            "tile.accent": Brush::Solid(BLUE),
        },
        colors: token_set! { canvas: CANVAS_DARK },
        numbers: token_set! {
            "tile.scale.panel": 1_f32,
            "tile.scale.accent": 1_f32,
        },
    }
}

thread_local! {
    static DARK: Signal<bool> = Scope::null().run(|| signal(true));
    // Every token is registered once, at startup; reads only hand out handles.
    static THEME: Rc<Theme> = Scope::null().run(|| {
        let theme = Theme::new();
        if DARK.with(|dark| dark.get()) {
            dark_palette().register(&theme);
        } else {
            light_palette().register(&theme);
        }
        Rc::new(theme)
    });
}

/// The application theme, shared by every page.
pub fn theme() -> Rc<Theme> {
    THEME.with(Rc::clone)
}

pub fn dark_theme() -> ReadSignal<bool> {
    DARK.with(|dark| **dark)
}

/// Switch every token in place; existing bindings update on their own.
pub fn toggle_theme() {
    DARK.with(|dark| dark.toggle());
    let palette = if DARK.with(|dark| dark.get()) {
        dark_palette()
    } else {
        light_palette()
    };
    THEME.with(|theme| palette.apply(theme));
}

pub fn background() -> Prop<Brush> {
    THEME.with(|theme| theme.get("canvas"))
}

pub fn panel_brush() -> Prop<Brush> {
    THEME.with(|theme| theme.get("panel"))
}

pub fn foreground() -> Prop<Brush> {
    THEME.with(|theme| theme.get("foreground"))
}

pub fn background_color() -> Color {
    THEME.with(|theme| theme.get::<Color>("canvas").get())
}

pub fn demo_button(label: impl Into<Prop<Rc<str>>>, action: impl FnMut() + 'static) -> Node {
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
            background: {active.pick(Brush::Solid(BLUE), panel)};
            align-items: center;
            justify-content: center;
        })
        .on_activate(move || selected.set(index))
        .children(text(title).style(mue_macroquad::style! {
            font-size: {responsive::font(14.)};
            line-height: {responsive::ui(24.)};
            color: {active.pick(Brush::Solid(Color::from_rgba(19, 23, 32, 255)), foreground)};
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
    let control = demo_button(
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
    let control = demo_button(
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
