//! A compact label for keywords and filters: [`tag`], modeled on Chakra UI's
//! `Tag`, `TagLabel`, `TagLeftIcon`, and `TagCloseButton`.
//!
//! Tags come in the [`BadgeVariant`]s subtle (the default), solid, and
//! outline, in three sizes, and may end with a close button that emits
//! [`on_close`](TagBuilder::on_close). Colors follow the [`Palette`] and the
//! color mode; sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{ControlSize, Palette};
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let shown = signal(true);
//! let _rust = tag("Rust").palette(Palette::Orange).size(ControlSize::Lg);
//! let _filter = tag("Open issues")
//!     .variant(BadgeVariant::Solid)
//!     .palette(Palette::Teal)
//!     .on_close(move || shown.set(false))
//!     .into_node();
//! # });
//! ```

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    layout::use_layout,
    math::vec2,
    motion::transition::Transition,
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox, text},
    paint::{
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{Size, Style, Styleable, StyleableExt},
};

use crate::{
    controls::badge::{BadgeVariant, badge_colors},
    system::{self, ControlSize, Palette},
};

/// Builder returned by [`tag`].
pub struct TagBuilder {
    label: Prop<Rc<str>>,
    variant: BadgeVariant,
    palette: Palette,
    size: ControlSize,
    left_icon: Option<Node>,
    on_close: Option<Box<dyn FnMut()>>,
    disabled: Prop<bool>,
    style: Style,
}

/// A tag showing `label`: subtle gray, medium size.
///
/// With [`on_close`](TagBuilder::on_close) it ends with a round close button
/// (a hit target of at least 44 × 44 logical pixels) that activates on a
/// click, a tap, or Enter, Space, Backspace, or Delete while focused.
pub fn tag(label: impl Into<Prop<Rc<str>>>) -> TagBuilder {
    TagBuilder {
        label: label.into(),
        variant: BadgeVariant::Subtle,
        palette: Palette::Gray,
        size: ControlSize::Md,
        left_icon: None,
        on_close: None,
        disabled: Prop::Static(false),
        style: Style::new(),
    }
}

impl TagBuilder {
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Small, medium, or large; `Xs` draws like `Sm`.
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// An icon before the label (Chakra's `TagLeftIcon`).
    pub fn left_icon(mut self, icon: impl IntoNode) -> Self {
        self.left_icon = Some(icon.into_node());
        self
    }

    /// Show a close button that runs `action` (Chakra's `TagCloseButton`).
    pub fn on_close(mut self, action: impl FnMut() + 'static) -> Self {
        self.on_close = Some(Box::new(action));
        self
    }

    /// Dim the tag and ignore its close button.
    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }
}

impl Styleable for TagBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// A painted × filling its node, in the inherited text color.
struct Cross {
    color: Prop<Brush>,
    style: Style,
}

impl Styleable for Cross {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for Cross {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { color, style } = self;
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
                let center = rect.center();
                let r = d * 0.3;
                let stroke = StrokeStyle::round((d * 0.14).max(1.));
                let brush = color.get_clone_untracked();
                paint.draw(|p| {
                    p.stroke(
                        &PathData::line(center + vec2(-r, -r), center + vec2(r, r)),
                        &stroke,
                        brush.shader(rect),
                    );
                    p.stroke(
                        &PathData::line(center + vec2(-r, r), center + vec2(r, -r)),
                        &stroke,
                        brush.shader(rect),
                    );
                });
            });
        })
    }
}

/// A close mark for tags, alerts, and other dismissible parts: a round,
/// focusable button drawing an × in `color`, `diameter` wide.
pub(crate) fn close_button(
    diameter: Size,
    color: Prop<Brush>,
    disabled: Prop<bool>,
    action: impl FnMut() + 'static,
) -> impl IntoNode + Styleable {
    let hovered = signal(false);
    let pressed = signal(false);
    let action = Rc::new(RefCell::new(action));
    let activate = move || {
        if !disabled.get_untracked() {
            (*action.borrow_mut())();
        }
    };
    flexbox()
        .width(diameter)
        .height(diameter)
        .flex_shrink(0.)
        .radius(system::radius::FULL)
        .opacity(computed(move |_| {
            if pressed.get() {
                1.
            } else if hovered.get() {
                0.8
            } else {
                0.5
            }
        }))
        .transition(Transition::ease_out(0.12))
        .focusable(true)
        .focus_ring(2., system::focus_ring(), 1.)
        .min_hit_size(44.)
        .cursor(Cursor::Pointer)
        .use_pressed(pressed)
        .on_hover_event(move |event| match event.action() {
            PointerAction::Down => hovered.set(true),
            PointerAction::Up => hovered.set(false),
            _ => {}
        })
        .on_activate({
            let activate = activate.clone();
            move || activate()
        })
        .on_key_down(move |event| {
            if !event.repeat
                && matches!(
                    event.key,
                    KeyCode::Enter
                        | KeyCode::KpEnter
                        | KeyCode::Space
                        | KeyCode::Backspace
                        | KeyCode::Delete
                )
            {
                event.stop_propagation();
                activate();
            }
        })
        .children(
            Cross {
                color,
                style: Style::new(),
            }
            .width(Size::Percent(100.))
            .height(Size::Percent(100.)),
        )
}

impl BuildNode for TagBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            label,
            variant,
            palette,
            size,
            left_icon,
            on_close,
            disabled,
            style,
        } = self;
        // Chakra's tag sizes: min height, font size, line height, padding.
        let (height, font_size, line_height, pad) = match size {
            ControlSize::Xs | ControlSize::Sm => (1.25, system::font_size::XS, 1., 0.5),
            ControlSize::Md => (1.5, system::font_size::SM, 1.25, 0.5),
            ControlSize::Lg => (2., system::font_size::MD, 1.5, 0.75),
        };
        let dark = system::is_dark();
        let colors = dark.map(move |dark| badge_colors(variant, palette, dark));
        let foreground: Prop<Brush> = colors.map(|(_, foreground, _)| Brush::Solid(foreground)).into();
        let mut root = flexbox()
            .flex_row()
            .items_center()
            .flex_shrink(0.)
            .gap(Size::Rem(0.375))
            .min_height(Size::Rem(height))
            .min_width(Size::Rem(height))
            .padding_x(Size::Rem(pad))
            .radius(system::radius::MD)
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .background(colors.map(|(background, _, _)| Brush::Solid(background)))
            .color(foreground.clone())
            .font_size(font_size)
            .line_height(Size::Rem(line_height));
        if variant == BadgeVariant::Outline {
            root = root
                .border((Size::Px(1.),))
                .border_color(colors.map(|(_, _, border)| Brush::Solid(border)));
        }
        let mut children: Vec<Node> = Vec::new();
        children.extend(left_icon);
        children.push(
            text(label)
                .font_weight(cosmic_text::Weight::MEDIUM)
                .text_wrap(false)
                .color(foreground.clone())
                .into_node(),
        );
        if let Some(action) = on_close {
            children.push(
                close_button(Size::Rem(line_height), foreground, disabled, action)
                    .margin_right(Size::Rem(-0.25))
                    .into_node(),
            );
        }
        root.children(children).style(style).build(ctx)
    }
}
