//! Checkboxes: [`checkbox`], modeled on Chakra UI's `Checkbox`.
//!
//! A checkbox is a box with an optional label. It toggles on a click, a tap,
//! or Space or Enter while focused, and it can show an indeterminate dash
//! (for a "select all" box whose children are partly checked). The check
//! mark fades and scales in; colors follow the [`Palette`] and the color
//! mode, and sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{ControlSize, Palette};
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let agreed = signal(false);
//! let terms = checkbox().checked(agreed).label("I agree to the terms");
//! let newsletter = checkbox()
//!     .default_checked(true)
//!     .palette(Palette::Green)
//!     .size(ControlSize::Lg)
//!     .label("Newsletter")
//!     .on_change(|checked: &bool| println!("newsletter: {checked}"));
//! let _row = hstack().children((terms, newsletter)).into_node();
//! # });
//! ```

use std::rc::Rc;

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    layout::use_layout,
    math::vec2,
    motion::transition::Transition,
    node::{BuildNode, Callback, IntoNode, Node, NodeContext, flexbox, text},
    paint::{
        color::Color,
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

use crate::{
    state::Controllable,
    system::{self, ControlSize, Palette, white_alpha},
};

/// Builder returned by [`checkbox`].
pub struct CheckboxBuilder {
    checked: Option<Signal<bool>>,
    default_checked: bool,
    indeterminate: Prop<bool>,
    disabled: Prop<bool>,
    invalid: Prop<bool>,
    label: Option<Prop<Rc<str>>>,
    palette: Palette,
    size: ControlSize,
    on_change: Callback<bool>,
    style: Style,
}

/// A checkbox with an optional label: blue, medium size, unchecked.
///
/// Bind the value with [`checked`](CheckboxBuilder::checked) (controlled) or
/// start it with [`default_checked`](CheckboxBuilder::default_checked)
/// (uncontrolled); [`on_change`](CheckboxBuilder::on_change) runs after each
/// toggle either way. The whole row (box and label) is the hit target, at
/// least 44 × 44 logical pixels; it is focusable and shows a focus ring
/// around the box for keyboard focus.
pub fn checkbox() -> CheckboxBuilder {
    CheckboxBuilder {
        checked: None,
        default_checked: false,
        indeterminate: Prop::Static(false),
        disabled: Prop::Static(false),
        invalid: Prop::Static(false),
        label: None,
        palette: Palette::Blue,
        size: ControlSize::Md,
        on_change: Callback::new(),
        style: Style::new(),
    }
}

impl CheckboxBuilder {
    /// Two-way bind the checked state (controlled).
    pub fn checked(mut self, checked: Signal<bool>) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The starting state when no [`checked`](Self::checked) signal is bound.
    pub fn default_checked(mut self, checked: bool) -> Self {
        self.default_checked = checked;
        self
    }

    /// Show a dash instead of the check mark (Chakra's `isIndeterminate`).
    /// The box looks checked; activating it still toggles `checked`.
    pub fn indeterminate(mut self, indeterminate: impl Into<Prop<bool>>) -> Self {
        self.indeterminate = indeterminate.into();
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// Show the error border.
    pub fn invalid(mut self, invalid: impl Into<Prop<bool>>) -> Self {
        self.invalid = invalid.into();
        self
    }

    /// Text after the box (Chakra's checkbox label).
    pub fn label(mut self, label: impl Into<Prop<Rc<str>>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The checked color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Called with the new state after each toggle by the user.
    pub fn on_change(self, handler: impl FnMut(&bool) + 'static) -> Self {
        self.on_change.add(handler);
        self
    }
}

impl Styleable for CheckboxBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Fill and border of a checkbox or radio control (Chakra's `control` part).
pub(crate) fn control_colors(
    palette: Palette,
    dark: bool,
    on: bool,
    hover: bool,
    invalid: bool,
) -> (Color, Color) {
    if on {
        let fill = if dark {
            palette.shade(if hover { 300 } else { 200 })
        } else {
            palette.shade(if hover { 600 } else { 500 })
        };
        return (fill, fill);
    }
    let border = if invalid {
        if dark { Palette::Red.shade(300) } else { Palette::Red.shade(500) }
    } else {
        match (dark, hover) {
            (false, false) => Palette::Gray.shade(200),
            (false, true) => Palette::Gray.shade(300),
            (true, false) => white_alpha(0.3),
            (true, true) => white_alpha(0.4),
        }
    };
    (Color::TRANSPARENT, border)
}

/// The mark drawn on a checked control.
pub(crate) fn mark_color(dark: bool) -> Color {
    if dark { Palette::Gray.shade(900) } else { Color::WHITE }
}

/// A painted check mark, or a dash while `dash` is true, filling the node.
fn check_mark(color: ReadSignal<Color>, dash: Prop<bool>) -> impl IntoNode + Styleable {
    struct Mark {
        color: ReadSignal<Color>,
        dash: Prop<bool>,
        style: Style,
    }
    impl Styleable for Mark {
        fn style_mut(&mut self) -> &mut Style {
            &mut self.style
        }
    }
    impl BuildNode for Mark {
        fn build(self, mut ctx: NodeContext) -> Node {
            let Self { color, dash, style } = self;
            ctx.push_style(style);
            ctx.finish(Style::new(), move |style| {
                let rect = use_layout(style).rect;
                let paint = use_paint(style);
                style.on_render.append(move |_| {
                    let rect = rect.get_untracked();
                    let d = rect.w.min(rect.h);
                    if d <= 0. {
                        return;
                    }
                    let at = |x: f32, y: f32| vec2(rect.x + rect.w * x, rect.y + rect.h * y);
                    let path = if dash.get_untracked() {
                        PathData::line(at(0.22, 0.5), at(0.78, 0.5))
                    } else {
                        PathData::polyline([at(0.2, 0.52), at(0.42, 0.72), at(0.8, 0.3)])
                    };
                    let stroke = StrokeStyle::round((d * 0.16).max(1.5));
                    let brush = Brush::Solid(color.get_untracked());
                    paint.draw(|p| p.stroke(&path, &stroke, brush.shader(rect)));
                });
            })
        }
    }
    Mark {
        color,
        dash,
        style: Style::new(),
    }
}

impl BuildNode for CheckboxBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            checked,
            default_checked,
            indeterminate,
            disabled,
            invalid,
            label,
            palette,
            size,
            on_change,
            style,
        } = self;
        let state = Controllable::new(checked.unwrap_or_else(|| signal(default_checked)), on_change);
        let value = state.signal();
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let on = computed(move |_| value.get() || indeterminate.get());
        let colors = computed(move |_| {
            let active = hovered.get() && !disabled.get();
            control_colors(palette, dark.get(), on.get(), active, invalid.get())
        });
        let toggle = {
            let state = state.clone();
            move || {
                if !disabled.get_untracked() {
                    state.set(!state.get_untracked());
                }
            }
        };

        let mut root = flexbox()
            .flex_row()
            .items_center()
            .flex_shrink(0.)
            .gap(size.gap())
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .focusable(true)
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
                let toggle = toggle.clone();
                move || toggle()
            })
            .on_key_down(move |event| {
                if !event.repeat
                    && matches!(event.key, KeyCode::Space | KeyCode::Enter | KeyCode::KpEnter)
                {
                    event.stop_propagation();
                    toggle();
                }
            });
        let mut focus = None;
        root = root.with_focus(|root, state| {
            focus = Some(state);
            root
        });
        let ring = focus.map(|focus| focus.visible);

        let box_size = Size::Rem(size.indicator());
        let mark_color = dark.map(mark_color);
        let scale = computed(move |_| {
            let s = if on.get() { 1. } else { 0.5 };
            Transform(vec![TransformOp::Scale(s, s)])
        });
        let press_scale = computed(move |_| {
            let s = if pressed.get() && !disabled.get() { 0.92 } else { 1. };
            Transform(vec![TransformOp::Scale(s, s)])
        });
        let mut control = flexbox()
            .width(box_size)
            .height(box_size)
            .flex_shrink(0.)
            .border((Size::Px(2.),))
            .radius(system::radius::SM)
            .background(colors.map(|(fill, _)| Brush::Solid(fill)))
            .border_color(colors.map(|(_, border)| Brush::Solid(border)))
            .transform(press_scale)
            .transition(Transition::ease_out(0.15))
            .children(
                check_mark(mark_color, indeterminate)
                    .width(Size::Percent(100.))
                    .height(Size::Percent(100.))
                    .opacity(on.map(|on| if on { 1. } else { 0. }))
                    .transform(scale)
                    .transition(Transition::spring(0.25, 0.2)),
            );
        if let Some(ring) = ring {
            control = control
                .outline_width(ring.map(|visible| if visible { 2. } else { 0. }))
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
