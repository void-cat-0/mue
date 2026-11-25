use std::{cell::Cell, rc::Rc};

use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue_components::{ButtonBuilder, button};
use mue_core::signal::Access;
use mue_macroquad::{
    input::InputBatch,
    layout::use_layout,
    math::Rect,
    node::{IntoChildren, IntoNode, Node, flexbox, text},
    style::{Style, Styleable, StyleableExt, percent},
};

use super::artwork::{self, Artwork};

pub const BACK: mue_macroquad::color::Color =
    mue_macroquad::color::Color::new(0.032, 0.043, 0.065, 1.);
pub const INK: mue_macroquad::color::Color = mue_macroquad::color::Color::new(0.94, 0.96, 1., 1.);
pub const SUB: mue_macroquad::color::Color = mue_macroquad::color::Color::new(0.67, 0.74, 0.86, 1.);
pub const PANEL: mue_macroquad::color::Color =
    mue_macroquad::color::Color::new(0.063, 0.080, 0.125, 0.96);
pub const GOLD: mue_macroquad::color::Color = mue_macroquad::color::Color::new(1., 0.86, 0.56, 1.);
pub const ACCENT: mue_macroquad::color::Color =
    mue_macroquad::color::Color::new(0.45, 0.87, 1., 1.);

#[derive(Default)]
struct Keys(Vec<KeyCode>);

impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: KeyMods, repeat: bool) {
        if !repeat {
            self.0.push(key);
        }
    }
}

pub fn keys(batch: &InputBatch) -> Vec<KeyCode> {
    let mut keys = Keys::default();
    batch.repeat(&mut keys);
    keys.0
}

pub fn action<A: Copy + 'static>(
    slot: Rc<Cell<Option<A>>>,
    label: &'static str,
    action: A,
) -> ButtonBuilder {
    button(label)
        .font_size(18.)
        .line_height(25.)
        .padding_x(16.)
        .padding_y(9.)
        .on_activate(move || slot.set(Some(action)))
}

pub fn column(children: impl IntoChildren) -> Node {
    flexbox()
        .w_full()
        .flex_column()
        .gap(12.)
        .children(children)
        .into_node()
}

pub fn card(children: impl IntoChildren) -> Node {
    flexbox()
        .w_full()
        .flex_column()
        .padding((22.,))
        .gap(10.)
        .radius(15.)
        .background(PANEL)
        .children(children)
        .into_node()
}

pub fn cover(
    art: Rc<Artwork>,
    height: f32,
    geometry: Rc<Cell<Rect>>,
    clip: Rc<Cell<Rect>>,
) -> Node {
    Node::leaf(
        Style::new()
            .width(percent(100.))
            .height(height)
            .flex_shrink(0.),
        move |style| {
            let rect = use_layout(style).rect;
            style.on_render.append(move |_| {
                let rect = rect.get();
                geometry.set(rect);
                artwork::draw(
                    &art,
                    artwork::Surface::card(rect, clip.get(), art.aspect()),
                    1.,
                );
            });
        },
    )
}

pub fn title(value: impl Into<mue_core::prop::Prop<Rc<str>>>, size: f32) -> Node {
    text(value)
        .font_size(size)
        .line_height(size * 1.2)
        .color(INK)
        .into_node()
}

pub fn caption(value: impl Into<mue_core::prop::Prop<Rc<str>>>) -> Node {
    text(value)
        .font_size(15.)
        .line_height(22.)
        .color(SUB)
        .into_node()
}

pub fn centered(viewport: Rect, width: f32, height: f32) -> Rect {
    let w = (viewport.w - 24.).clamp(1., width);
    let h = (viewport.h - 18.).clamp(1., height);
    Rect::new(
        viewport.x + (viewport.w - w) / 2.,
        viewport.y + (viewport.h - h) / 2.,
        w,
        h,
    )
}

pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
