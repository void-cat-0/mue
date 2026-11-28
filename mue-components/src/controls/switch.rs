//! A toggle switch: [`switch`], modeled on Chakra UI's `Switch`.
//!
//! The thumb slides along a rounded track with a soft spring and the track
//! fills with the [`Palette`] color while the switch is on. Sizes (small,
//! medium, large) are in `rem`; colors follow the color mode.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{ControlSize, Palette};
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let wifi = signal(true);
//! let wifi_switch = switch().checked(wifi).label("Wi-Fi");
//! let airplane = switch()
//!     .palette(Palette::Teal)
//!     .size(ControlSize::Lg)
//!     .label("Airplane mode")
//!     .on_change(|on: &bool| println!("airplane mode: {on}"));
//! let _settings = stack().children((wifi_switch, airplane)).into_node();
//! # });
//! ```

use std::rc::Rc;

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    motion::transition::Transition,
    node::{BuildNode, Callback, IntoNode, Node, NodeContext, flexbox, text},
    paint::{color::Color, shader::Brush},
    style::{BoxShadow, Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

use crate::{
    state::Controllable,
    system::{self, ControlSize, Palette, black_alpha, white_alpha},
};

/// Builder returned by [`switch`].
pub struct SwitchBuilder {
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: Prop<bool>,
    label: Option<Prop<Rc<str>>>,
    palette: Palette,
    size: ControlSize,
    on_change: Callback<bool>,
    style: Style,
}

/// A switch with an optional label after it: blue, medium size, off.
///
/// Bind the state with [`checked`](SwitchBuilder::checked) (controlled) or
/// start it with [`default_checked`](SwitchBuilder::default_checked);
/// [`on_change`](SwitchBuilder::on_change) runs after each toggle by the
/// user. A click, a tap, or Space or Enter while focused toggles it; the
/// whole row is a hit target of at least 44 × 44 logical pixels, and a focus
/// ring surrounds the track for keyboard focus. `ControlSize::Xs` draws like
/// `Sm`.
pub fn switch() -> SwitchBuilder {
    SwitchBuilder {
        checked: None,
        default_checked: false,
        disabled: Prop::Static(false),
        label: None,
        palette: Palette::Blue,
        size: ControlSize::Md,
        on_change: Callback::new(),
        style: Style::new(),
    }
}

impl SwitchBuilder {
    /// Two-way bind the on state (controlled).
    pub fn checked(mut self, checked: Signal<bool>) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The starting state when no [`checked`](Self::checked) signal is bound.
    pub fn default_checked(mut self, checked: bool) -> Self {
        self.default_checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// Text after the track.
    pub fn label(mut self, label: impl Into<Prop<Rc<str>>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The track color while on (Chakra's `colorScheme`).
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

impl Styleable for SwitchBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Track width and height (without padding) in `rem`, Chakra's values.
fn track(size: ControlSize) -> (f32, f32) {
    match size {
        ControlSize::Xs | ControlSize::Sm => (1.375, 0.75),
        ControlSize::Md => (1.875, 1.),
        ControlSize::Lg => (2.875, 1.5),
    }
}

/// The track's padding around the thumb.
const PAD: f32 = 0.125;

impl BuildNode for SwitchBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            checked,
            default_checked,
            disabled,
            label,
            palette,
            size,
            on_change,
            style,
        } = self;
        let state = Controllable::new(checked.unwrap_or_else(|| signal(default_checked)), on_change);
        let on = state.signal();
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let toggle = {
            let state = state.clone();
            move || {
                if !disabled.get_untracked() {
                    state.set(!state.get_untracked());
                }
            }
        };
        let track_color = computed(move |_| {
            let hover = hovered.get() && !disabled.get();
            let color = match (on.get(), dark.get()) {
                (true, false) => palette.shade(if hover { 600 } else { 500 }),
                (true, true) => palette.shade(if hover { 300 } else { 200 }),
                (false, false) => Palette::Gray.shade(if hover { 400 } else { 300 }),
                (false, true) => white_alpha(if hover { 0.5 } else { 0.4 }),
            };
            Brush::Solid(color)
        });

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

        let (width, height) = track(size);
        let travel = width - height;
        // The thumb stretches a little while pressed, like iOS.
        let stretch = height * 0.2;
        let thumb_width = computed(move |_| {
            let grow = if pressed.get() && !disabled.get() { stretch } else { 0. };
            Size::Rem(height + grow)
        });
        let thumb_offset = computed(move |_| {
            let grow = if pressed.get() && !disabled.get() { stretch } else { 0. };
            let x = if on.get() { travel - grow } else { 0. };
            Transform(vec![TransformOp::Translate(Size::Rem(x), Size::Px(0.))])
        });
        let thumb = flexbox()
            .width(thumb_width)
            .height(Size::Rem(height))
            .flex_shrink(0.)
            .radius(system::radius::FULL)
            .background(Color::WHITE)
            .box_shadow(BoxShadow::new(0., 1., 3., black_alpha(0.25)))
            .transform(thumb_offset)
            .transition(Transition::spring(0.3, 0.15));
        let mut track = flexbox()
            .flex_row()
            .items_center()
            .flex_shrink(0.)
            .width(Size::Rem(width + 2. * PAD))
            .height(Size::Rem(height + 2. * PAD))
            .padding((Size::Rem(PAD),))
            .radius(system::radius::FULL)
            .background(track_color)
            .transition(Transition::ease_out(0.15))
            .children(thumb);
        if let Some(focus) = focus {
            track = track
                .outline_width(focus.visible.map(|visible| if visible { 2. } else { 0. }))
                .outline_color(system::focus_ring())
                .outline_offset(2.);
        }
        let mut children: Vec<Node> = vec![track.into_node()];
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
