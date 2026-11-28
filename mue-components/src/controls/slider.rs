//! A horizontal slider: [`slider`], modeled on Chakra UI's `Slider`.
//!
//! The slider picks a number between `min` and `max` in `step`s. Drag the
//! thumb with a mouse or a finger, press anywhere on the track to jump
//! there, or use the arrow keys, Page Up and Page Down (a tenth of the
//! range), Home, and End while it is focused. The filled part of the track
//! shows the [`Palette`] color and the thumb grows while pressed. Sizes are
//! in `rem`; colors follow the color mode.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::Palette;
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let volume = signal(40_f32);
//! let _volume = slider().value(volume).palette(Palette::Pink).into_node();
//! let _opacity = slider()
//!     .range(0., 1.)
//!     .step(0.05)
//!     .default_value(0.8)
//!     .on_change_end(|value: &f32| println!("opacity {value}"))
//!     .into_node();
//! # });
//! ```

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    motion::transition::Transition,
    node::{BuildNode, Callback, IntoNode, Node, NodeContext, NodeHandle, flexbox},
    paint::{color::Color, shader::Brush},
    style::{BoxShadow, Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

use crate::{
    state::Controllable,
    system::{self, ControlSize, Palette, black_alpha, white_alpha},
};

/// Builder returned by [`slider`].
pub struct SliderBuilder {
    value: Option<Signal<f32>>,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    disabled: Prop<bool>,
    palette: Palette,
    size: ControlSize,
    on_change: Callback<f32>,
    on_change_end: Callback<f32>,
    style: Style,
}

/// A horizontal slider from 0 to 100 in steps of 1, as wide as its
/// container.
///
/// Bind the value with [`value`](SliderBuilder::value) (controlled) or start
/// it with [`default_value`](SliderBuilder::default_value) (the middle of the
/// range otherwise). [`on_change`](SliderBuilder::on_change) runs on every
/// change while dragging or pressing keys, and
/// [`on_change_end`](SliderBuilder::on_change_end) once a drag or key press
/// ends. The control is at least 44 logical pixels tall for touch.
pub fn slider() -> SliderBuilder {
    SliderBuilder {
        value: None,
        default_value: None,
        min: 0.,
        max: 100.,
        step: 1.,
        disabled: Prop::Static(false),
        palette: Palette::Blue,
        size: ControlSize::Md,
        on_change: Callback::new(),
        on_change_end: Callback::new(),
        style: Style::new(),
    }
}

impl SliderBuilder {
    /// Two-way bind the value (controlled).
    pub fn value(mut self, value: Signal<f32>) -> Self {
        self.value = Some(value);
        self
    }

    /// The starting value when no [`value`](Self::value) signal is bound.
    pub fn default_value(mut self, value: f32) -> Self {
        self.default_value = Some(value);
        self
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// Set `min` and `max` together.
    pub fn range(self, min: f32, max: f32) -> Self {
        self.min(min).max(max)
    }

    /// The value moves in multiples of `step` from `min`; 0 or less means
    /// any value.
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// The filled track color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Called with the value after each change by the user.
    pub fn on_change(self, handler: impl FnMut(&f32) + 'static) -> Self {
        self.on_change.add(handler);
        self
    }

    /// Called with the value when a drag or a key press ends.
    pub fn on_change_end(self, handler: impl FnMut(&f32) + 'static) -> Self {
        self.on_change_end.add(handler);
        self
    }
}

impl Styleable for SliderBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Thumb diameter and track thickness in `rem` (Chakra's sizes).
fn metrics(size: ControlSize) -> (f32, f32) {
    match size {
        ControlSize::Xs | ControlSize::Sm => (0.625, 0.125),
        ControlSize::Md => (0.875, 0.25),
        ControlSize::Lg => (1., 0.375),
    }
}

/// Snap `value` to the step grid and clamp it into the range.
fn snap(value: f32, min: f32, max: f32, step: f32) -> f32 {
    let value = if step > 0. {
        min + ((value - min) / step).round() * step
    } else {
        value
    };
    value.clamp(min, max)
}

impl BuildNode for SliderBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            value,
            default_value,
            min,
            max,
            step,
            disabled,
            palette,
            size,
            on_change,
            on_change_end,
            style,
        } = self;
        let (min, max) = if max < min { (max, min) } else { (min, max) };
        let span = (max - min).max(f32::EPSILON);
        let initial = snap(default_value.unwrap_or(min + span / 2.), min, max, step);
        let state = Controllable::new(value.unwrap_or_else(|| signal(initial)), on_change);
        let current = state.signal();
        let fraction = computed(move |_| ((current.get() - min) / span).clamp(0., 1.));
        let dragging = signal(false);
        let hovered = signal(false);
        let dark = system::is_dark();
        let track = NodeHandle::new();

        let set_from_x = {
            let state = state.clone();
            move |x: f32| {
                let Some(rect) = track.rect() else {
                    return;
                };
                if rect.w <= 0. {
                    return;
                }
                let t = ((x - rect.x) / rect.w).clamp(0., 1.);
                state.set(snap(min + t * span, min, max, step));
            }
        };
        let end = {
            let on_change_end = on_change_end.clone();
            move || on_change_end.emit(&current.get_untracked())
        };

        let (thumb, thickness) = metrics(size);
        let filled = computed(move |_| {
            let color = match (dark.get(), disabled.get()) {
                (false, false) => palette.shade(500),
                (true, false) => palette.shade(200),
                (false, true) => Palette::Gray.shade(400),
                (true, true) => white_alpha(0.3),
            };
            Brush::Solid(color)
        });
        // Follow the value instantly while dragging; glide after a jump.
        let glide = dragging.map(|dragging| {
            if dragging { Transition::Instant } else { Transition::ease_out(0.15) }
        });
        let knob_scale = computed(move |_| {
            let s = if dragging.get() {
                1.2
            } else if hovered.get() && !disabled.get() {
                1.08
            } else {
                1.
            };
            Transform(vec![TransformOp::Scale(s, s)])
        });

        let mut root = flexbox()
            .flex_row()
            .items_center()
            .width(Size::Percent(100.))
            .height(Size::Rem(thumb))
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .focusable(true)
            .min_hit_size(44.)
            .cursor(disabled.map(|disabled| {
                if disabled { Cursor::NotAllowed } else { Cursor::Pointer }
            }))
            .on_hover_event(move |event| match event.action() {
                PointerAction::Down => hovered.set(true),
                PointerAction::Up => hovered.set(false),
                _ => {}
            })
            .on_pointer_event({
                let set_from_x = set_from_x.clone();
                let end = end.clone();
                let mut active = None;
                move |event| {
                    match event.action() {
                        PointerAction::Down => {
                            if active.is_some() || disabled.get_untracked() {
                                return;
                            }
                            active = Some(event.pointer_id());
                            dragging.set(true);
                            set_from_x(event.position().x);
                        }
                        PointerAction::Move if active == Some(event.pointer_id()) => {
                            set_from_x(event.position().x);
                        }
                        PointerAction::Up | PointerAction::Cancel
                            if active == Some(event.pointer_id()) =>
                        {
                            active = None;
                            dragging.set(false);
                            end();
                        }
                        _ => {}
                    }
                }
            })
            // Claim horizontal drags so a scroll area around the slider does
            // not take them; the pointer hook above moves the thumb.
            .on_pan(|_| {})
            .on_key_down({
                let state = state.clone();
                move |event| {
                    if disabled.get_untracked() {
                        return;
                    }
                    let unit = if step > 0. { step } else { span / 100. };
                    let page = (span / 10.).max(unit);
                    let value = current.get_untracked();
                    let next = match event.key {
                        KeyCode::Right | KeyCode::Up => value + unit,
                        KeyCode::Left | KeyCode::Down => value - unit,
                        KeyCode::PageUp => value + page,
                        KeyCode::PageDown => value - page,
                        KeyCode::Home => min,
                        KeyCode::End => max,
                        _ => return,
                    };
                    event.stop_propagation();
                    state.set(snap(next, min, max, step));
                    end();
                }
            });
        let mut focus = None;
        root = root.with_focus(|root, state| {
            focus = Some(state);
            root
        });

        let mut knob = flexbox()
            .width(Size::Percent(100.))
            .height(Size::Percent(100.))
            .radius(system::radius::FULL)
            .background(Color::WHITE)
            .border((Size::Px(1.),))
            .border_color(system::mode(Palette::Gray.shade(200), Color::TRANSPARENT))
            .box_shadow(BoxShadow::new(0., 1., 3., black_alpha(0.3)))
            .transform(knob_scale)
            .transition(Transition::ease_out(0.12));
        if let Some(focus) = focus {
            knob = knob
                .outline_width(focus.visible.map(|visible| if visible { 3. } else { 0. }))
                .outline_color(system::mode(palette.alpha(500, 0.6), palette.alpha(200, 0.6)))
                .outline_offset(0.);
        }
        // The thumb's center sits on the value; the knob inside it scales.
        let handle = flexbox()
            .absolute()
            .inset_left(fraction.map(|t| Size::Percent(t * 100.)))
            .inset_top(Size::Percent(50.))
            .width(Size::Rem(thumb))
            .height(Size::Rem(thumb))
            .transform(Transform(vec![TransformOp::Translate(
                Size::Percent(-50.),
                Size::Percent(-50.),
            )]))
            .transition(glide)
            .children(knob);
        let track = flexbox()
            .node_handle(track)
            .relative()
            .flex_grow(1.)
            .height(Size::Rem(thickness))
            .radius(system::radius::SM)
            .background(system::mode(Palette::Gray.shade(200), white_alpha(0.16)))
            .children((
                flexbox()
                    .absolute()
                    .inset_left(Size::Px(0.))
                    .inset_top(Size::Px(0.))
                    .height(Size::Percent(100.))
                    .width(fraction.map(|t| Size::Percent(t * 100.)))
                    .radius(system::radius::SM)
                    .background(filled)
                    .transition(glide),
                handle,
            ));
        root.children(track.into_node()).style(style).build(ctx)
    }
}
