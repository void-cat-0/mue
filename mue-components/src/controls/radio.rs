//! Radio buttons: [`radio_group`] and [`radio`], modeled on Chakra UI's
//! `RadioGroup` and `Radio`.
//!
//! A group holds one selected value (an `Rc<str>`; empty while nothing is
//! selected) and lays out its radios in a column or a row. Only the selected
//! radio is reached by Tab; the arrow keys move the selection to the next or
//! previous enabled radio and focus it, wrapping at the ends, like the radio
//! groups of a browser. The selected dot scales in, colors follow the
//! [`Palette`] and the color mode, and sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::Palette;
//! use mue_core::prelude::*;
//! use std::rc::Rc;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let plan = signal(Rc::<str>::from("pro"));
//! let _plans = radio_group()
//!     .value(plan)
//!     .palette(Palette::Teal)
//!     .children((
//!         radio("free").label("Free"),
//!         radio("pro").label("Pro"),
//!         radio("team").label("Team").disabled(true),
//!     ))
//!     .into_node();
//! # });
//! ```

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    motion::transition::Transition,
    node::{BuildNode, Callback, IntoNode, Node, NodeContext, NodeHandle, flexbox, text},
    paint::shader::Brush,
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp, values::FlexWrap},
};

use crate::{
    controls::checkbox::{control_colors, mark_color},
    state::{Controllable, provide_part, use_part},
    system::{self, ControlSize, Palette},
};

struct Entry {
    id: u64,
    value: Rc<str>,
    disabled: Prop<bool>,
    handle: NodeHandle,
}

#[derive(Clone)]
struct RadioContext {
    state: Controllable<Rc<str>>,
    palette: Palette,
    size: ControlSize,
    disabled: Prop<bool>,
    entries: Rc<RefCell<Vec<Entry>>>,
    /// Bumped when a radio joins or leaves, so tab stops recompute.
    version: Signal<u64>,
    next_id: Rc<std::cell::Cell<u64>>,
}

impl RadioContext {
    /// Select the next (`step` 1) or previous (-1) enabled radio after
    /// `from` and focus it.
    fn step(&self, from: &str, step: isize) {
        let target = {
            let entries = self.entries.borrow();
            let count = entries.len() as isize;
            let Some(start) = entries.iter().position(|entry| &*entry.value == from) else {
                return;
            };
            (1..count)
                .map(|offset| &entries[(start as isize + step * offset).rem_euclid(count) as usize])
                .find(|entry| !entry.disabled.get_untracked())
                .map(|entry| (entry.value.clone(), entry.handle))
        };
        if let Some((value, handle)) = target {
            self.state.set(value);
            handle.focus();
        }
    }

    /// Whether `value` is the radio Tab reaches: the selected one, or the
    /// first enabled one while nothing (enabled) is selected.
    fn is_tab_stop(&self, value: &str) -> bool {
        self.version.get();
        let selected = self.state.get();
        let entries = self.entries.borrow();
        let enabled = |entry: &&Entry| !entry.disabled.get();
        if entries
            .iter()
            .filter(enabled)
            .any(|entry| entry.value == selected)
        {
            return *selected == *value;
        }
        entries
            .iter()
            .find(enabled)
            .is_some_and(|entry| &*entry.value == value)
    }
}

/// Builder returned by [`radio_group`].
pub struct RadioGroupBuilder {
    value: Option<Signal<Rc<str>>>,
    default_value: Rc<str>,
    on_change: Callback<Rc<str>>,
    palette: Palette,
    size: ControlSize,
    disabled: Prop<bool>,
    horizontal: bool,
    style: Style,
}

/// A group of [`radio`]s sharing one selected value. Pass the radios with
/// `.children(..)`; they may be nested in other layout nodes.
///
/// Bind the value with [`value`](RadioGroupBuilder::value) (controlled) or
/// start it with [`default_value`](RadioGroupBuilder::default_value);
/// [`on_change`](RadioGroupBuilder::on_change) runs after each selection by
/// the user. Radios stack in a column with a small gap unless
/// [`horizontal`](RadioGroupBuilder::horizontal) is set.
pub fn radio_group() -> RadioGroupBuilder {
    RadioGroupBuilder {
        value: None,
        default_value: Rc::from(""),
        on_change: Callback::new(),
        palette: Palette::Blue,
        size: ControlSize::Md,
        disabled: Prop::Static(false),
        horizontal: false,
        style: Style::new(),
    }
}

impl RadioGroupBuilder {
    /// Two-way bind the selected value (controlled); empty selects nothing.
    pub fn value(mut self, value: Signal<Rc<str>>) -> Self {
        self.value = Some(value);
        self
    }

    /// The starting value when no [`value`](Self::value) signal is bound.
    pub fn default_value(mut self, value: impl Into<Rc<str>>) -> Self {
        self.default_value = value.into();
        self
    }

    /// Called with the new value after each selection by the user.
    pub fn on_change(self, handler: impl FnMut(&Rc<str>) + 'static) -> Self {
        self.on_change.add(handler);
        self
    }

    /// The selected color of every radio (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// The size of every radio.
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Disable every radio in the group.
    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// Lay the radios out in a wrapping row instead of a column.
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }
}

impl Styleable for RadioGroupBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for RadioGroupBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            value,
            default_value,
            on_change,
            palette,
            size,
            disabled,
            horizontal,
            style,
        } = self;
        let value = value.unwrap_or_else(|| signal(default_value));
        provide_part(RadioContext {
            state: Controllable::new(value, on_change),
            palette,
            size,
            disabled,
            entries: Rc::default(),
            version: signal(0),
            next_id: Rc::default(),
        });
        let root = if horizontal {
            flexbox()
                .flex_row()
                .flex_wrap(FlexWrap::Wrap)
                .items_center()
                .gap(Size::Rem(1.))
        } else {
            flexbox().flex_column().items_start().gap(Size::Rem(0.5))
        };
        root.style(style).build(ctx)
    }
}

/// Builder returned by [`radio`].
pub struct RadioBuilder {
    value: Rc<str>,
    label: Option<Prop<Rc<str>>>,
    disabled: Prop<bool>,
    style: Style,
}

/// One option of a [`radio_group`], selecting `value`. It must be built
/// inside a group.
///
/// The circle and label form one hit target of at least 44 × 44 logical
/// pixels; a click, a tap, or Space selects it.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _sizes = radio_group()
///     .default_value("m")
///     .horizontal(true)
///     .children((radio("s").label("Small"), radio("m").label("Medium"), radio("l").label("Large")));
/// ```
///
/// # Panics
///
/// When built outside a [`radio_group`].
pub fn radio(value: impl Into<Rc<str>>) -> RadioBuilder {
    RadioBuilder {
        value: value.into(),
        label: None,
        disabled: Prop::Static(false),
        style: Style::new(),
    }
}

impl RadioBuilder {
    /// Text after the circle.
    pub fn label(mut self, label: impl Into<Prop<Rc<str>>>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }
}

impl Styleable for RadioBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for RadioBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            value,
            label,
            disabled,
            style,
        } = self;
        let group: RadioContext = use_part("radio", "radio_group");
        let RadioContext { palette, size, .. } = group;
        let group_disabled = group.disabled;
        let disabled: Prop<bool> = computed(move |_| disabled.get() || group_disabled.get()).into();
        let handle = NodeHandle::new();
        let id = group.next_id.get();
        group.next_id.set(id + 1);
        group.entries.borrow_mut().push(Entry {
            id,
            value: value.clone(),
            disabled,
            handle,
        });
        group.version.update(|version| *version += 1);
        {
            let entries = group.entries.clone();
            let version = group.version;
            watch_effect(move || {
                let entries = entries.clone();
                on_cleanup(move || {
                    entries.borrow_mut().retain(|entry| entry.id != id);
                    if let Some(current) = version.try_get_untracked() {
                        let _ = version.try_set(current + 1);
                    }
                });
            });
        }

        let selected = {
            let state = group.state.clone();
            let value = value.clone();
            computed(move |_| *state.get() == *value)
        };
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let colors = computed(move |_| {
            let active = hovered.get() && !disabled.get();
            control_colors(palette, dark.get(), selected.get(), active, false)
        });
        let select = {
            let state = group.state.clone();
            let value = value.clone();
            move || {
                if !disabled.get_untracked() {
                    state.set(value.clone());
                }
            }
        };
        let tab_index = {
            let group = group.clone();
            let value = value.clone();
            computed(move |_| if group.is_tab_stop(&value) { 0 } else { -1 })
        };

        let mut root = flexbox()
            .node_handle(handle)
            .flex_row()
            .items_center()
            .flex_shrink(0.)
            .gap(size.gap())
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .focusable(disabled.map(|disabled| !disabled))
            .tab_index(tab_index)
            .min_hit_size(44.)
            .cursor(disabled.map(|disabled| {
                if disabled { Cursor::NotAllowed } else { Cursor::Pointer }
            }))
            .use_pressed(pressed)
            .on_hover_event(move |event| match event.action() {
                PointerAction::Down => hovered.set(true),
                PointerAction::Up => hovered.set(false),
                _ => {}
            })
            .on_activate({
                let select = select.clone();
                move || select()
            })
            .on_key_down(move |event| {
                let step = match event.key {
                    KeyCode::Space | KeyCode::Enter | KeyCode::KpEnter if !event.repeat => {
                        event.stop_propagation();
                        select();
                        return;
                    }
                    KeyCode::Down | KeyCode::Right => 1,
                    KeyCode::Up | KeyCode::Left => -1,
                    _ => return,
                };
                event.stop_propagation();
                group.step(&value, step);
            });
        let mut focus = None;
        root = root.with_focus(|root, state| {
            focus = Some(state);
            root
        });

        let circle = Size::Rem(size.indicator());
        let dot_scale = computed(move |_| {
            let s = if selected.get() { 1. } else { 0. };
            Transform(vec![TransformOp::Scale(s, s)])
        });
        let press_scale = computed(move |_| {
            let s = if pressed.get() && !disabled.get() { 0.9 } else { 1. };
            Transform(vec![TransformOp::Scale(s, s)])
        });
        let mut control = flexbox()
            .width(circle)
            .height(circle)
            .flex_shrink(0.)
            .items_center()
            .justify_center()
            .border((Size::Px(2.),))
            .radius(system::radius::FULL)
            .background(colors.map(|(fill, _)| Brush::Solid(fill)))
            .border_color(colors.map(|(_, border)| Brush::Solid(border)))
            .transform(press_scale)
            .transition(Transition::ease_out(0.15))
            .children(
                flexbox()
                    .width(Size::Percent(50.))
                    .height(Size::Percent(50.))
                    .radius(system::radius::FULL)
                    .background(dark.map(|dark| Brush::Solid(mark_color(dark))))
                    .opacity(selected.map(|on| if on { 1. } else { 0. }))
                    .transform(dot_scale)
                    .transition(Transition::spring(0.25, 0.25)),
            );
        if let Some(focus) = focus {
            control = control
                .outline_width(focus.visible.map(|visible| if visible { 2. } else { 0. }))
                .outline_color(system::focus_ring())
                .outline_offset(2.);
        }
        let mut children: Vec<Node> = vec![control.into_node()];
        if let Some(label) = label {
            children.push(
                text(label)
                    .font_size(size.font_size())
                    .line_height(size.line_height())
                    .color(system::fg())
                    .into_node(),
            );
        }
        root.children(children).style(style).build(ctx)
    }
}
