//! Example typography uses a 480-unit short edge, independent of window orientation.
#![allow(dead_code)]
use macroquad::window::{screen_height, screen_width};
use mue_core::{
    prop::Prop,
    scope::Scope,
    signal::{Access, ReadSignal, Signal, signal},
};
use mue_macroquad::style::{FontSize, Size};

#[derive(Clone, Copy, PartialEq)]
struct Metrics {
    scale: f32,
    compact: bool,
}

impl Metrics {
    fn for_viewport(width: f32, height: f32) -> Self {
        let short_edge = width.max(1.).min(height.max(1.));
        let scale = (short_edge / 480.).max(0.75);
        Self {
            scale,
            compact: width / scale < 600.,
        }
    }
}

thread_local! {
    // Pages can be constructed in short-lived scopes; shared viewport state must outlive them.
    static METRICS: Signal<Metrics> = Scope::null().run(|| signal(Metrics::for_viewport(480., 800.)));
    static COMPACT: ReadSignal<bool> = Scope::null().run(|| METRICS.with(|metrics| metrics.map(|m| m.compact)));
    static ZOOM: Signal<f32> = Scope::null().run(|| signal(0.75));
}

pub fn sync() {
    let mut metrics = Metrics::for_viewport(screen_width(), screen_height());
    metrics.scale *= ZOOM.with(|zoom| zoom.get());
    METRICS.with(|state| state.set(metrics));
}

pub fn zoom() -> ReadSignal<f32> {
    ZOOM.with(|zoom| zoom.map(|value| value))
}

pub fn cycle_zoom() {
    ZOOM.with(|zoom| {
        const LEVELS: [f32; 5] = [0.5, 0.75, 1., 1.25, 1.5];
        zoom.update(|value| {
            let index = LEVELS
                .iter()
                .position(|level| (*level - *value).abs() < 0.001)
                .unwrap_or(0);
            *value = LEVELS[(index + 1) % LEVELS.len()];
        })
    });
}

pub fn window_config(window: macroquad::window::Conf) -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: window,
        // Gradient fills sample a 32x32 grid and need more than the default 5000 indices.
        draw_call_vertex_capacity: 20_000,
        draw_call_index_capacity: 30_000,
        ..Default::default()
    }
}

pub fn scale() -> f32 {
    METRICS.with(|state| state.get().scale)
}

pub fn compact() -> ReadSignal<bool> {
    COMPACT.with(|state| *state)
}

/// Set on the actual App root; descendants express their type hierarchy in rem.
pub fn root_font() -> Prop<FontSize> {
    METRICS.with(|state| {
        state
            .map(|metrics| FontSize::Px(32. * metrics.scale))
            .into()
    })
}

/// A length in design units, for line heights, padding and touch targets.
pub fn ui(value: impl Into<Prop<f32>>) -> Prop<Size> {
    let value = value.into();
    METRICS.with(|state| {
        state
            .map(move |metrics| Size::Px(value.get() * metrics.scale))
            .into()
    })
}

/// A font size relative to the responsive 32-unit root, not to the immediate parent.
pub fn font(value: impl Into<Prop<f32>>) -> Prop<FontSize> {
    value.into().map(|value| FontSize::Rem(value / 32.))
}
