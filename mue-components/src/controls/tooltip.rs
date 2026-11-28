//! A hint bubble for a control: [`tooltip`], modeled on Chakra UI's
//! `Tooltip`.
//!
//! The tooltip wraps a child. It opens after a short delay while a mouse
//! hovers the child, and on a long press for touch (closing a moment after
//! the finger lifts); a press on the child closes it. The bubble is
//! teleported into [`Layer::Tooltip`], so scroll areas never clip it, and
//! anchored next to the child with a [`Placement`] that flips to stay in the
//! window. It fades and scales in and out. Colors follow the color mode;
//! sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let _save = tooltip("Save the document (Ctrl+S)", button("Save"))
//!     .placement(Placement::Bottom)
//!     .has_arrow(true)
//!     .into_node();
//! let _help = tooltip("More about plans", icon_button(text("?"))).open_delay(0.);
//! # });
//! ```

use crate::overlay::anchor::{Placement, anchored, use_anchor};
use crate::motion::presence::{PresenceAnimation, presence};
use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    app::runtime::{TimeoutHandle, set_timeout},
    input::pointer::{PointerAction, PointerType},
    motion::transition::Transition,
    node::{BuildNode, IntoNode, Node, NodeContext, NodeHandle, flexbox, text},
    overlay::layer::{Layer, teleport},
    paint::shader::Brush,
    style::{BoxShadow, BoxShadows, Size, Style, Styleable, StyleableExt, Transform, TransformOp, root_font_size},
};

use crate::system::{self, Palette, black_alpha, white_alpha};

/// Builder returned by [`tooltip`].
pub struct TooltipBuilder {
    label: Prop<Rc<str>>,
    child: Node,
    placement: Prop<Placement>,
    open_delay: f32,
    close_delay: f32,
    has_arrow: bool,
    disabled: Prop<bool>,
    style: Style,
}

/// Show `label` in a bubble above `child` while it is hovered (after 0.4 s)
/// or long-pressed.
///
/// Builder styles apply to the bubble.
pub fn tooltip(label: impl Into<Prop<Rc<str>>>, child: impl IntoNode) -> TooltipBuilder {
    TooltipBuilder {
        label: label.into(),
        child: child.into_node(),
        placement: Prop::Static(Placement::Top),
        open_delay: 0.4,
        close_delay: 0.,
        has_arrow: false,
        disabled: Prop::Static(false),
        style: Style::new(),
    }
}

impl TooltipBuilder {
    /// The preferred side; the bubble flips when it would leave the window.
    pub fn placement(mut self, placement: impl Into<Prop<Placement>>) -> Self {
        self.placement = placement.into();
        self
    }

    /// Seconds of hover before the bubble opens (0.4 by default).
    pub fn open_delay(mut self, seconds: f32) -> Self {
        self.open_delay = seconds.max(0.);
        self
    }

    /// Seconds before the bubble closes once the pointer leaves (0 by default).
    pub fn close_delay(mut self, seconds: f32) -> Self {
        self.close_delay = seconds.max(0.);
        self
    }

    /// Draw a small arrow pointing at the child.
    pub fn has_arrow(mut self, arrow: bool) -> Self {
        self.has_arrow = arrow;
        self
    }

    /// Never open while `disabled`.
    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }
}

impl Styleable for TooltipBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Opens and closes the bubble after delays; a newer request replaces an
/// older one.
#[derive(Clone)]
struct Timer {
    open: Signal<bool>,
    pending: Rc<RefCell<Option<TimeoutHandle>>>,
}

impl Timer {
    fn cancel(&self) {
        if let Some(handle) = self.pending.borrow_mut().take() {
            handle.cancel();
        }
    }

    fn set_after(&self, open: bool, delay: f32) {
        self.cancel();
        if delay <= 0. {
            self.open.set(open);
            return;
        }
        let target = self.open;
        let pending = self.pending.clone();
        let handle = set_timeout(delay, move || {
            pending.borrow_mut().take();
            // The tooltip may be gone by now.
            let _ = target.try_set(open);
        });
        *self.pending.borrow_mut() = Some(handle);
    }
}

/// How long a tooltip opened by a long press stays after the finger lifts.
const TOUCH_LINGER: f32 = 1.5;

impl BuildNode for TooltipBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            label,
            child,
            placement,
            open_delay,
            close_delay,
            has_arrow,
            disabled,
            style,
        } = self;
        let trigger = NodeHandle::new();
        let open = signal(false);
        let timer = Timer {
            open,
            pending: Rc::default(),
        };
        {
            let timer = timer.clone();
            watch_effect(move || {
                let timer = timer.clone();
                on_cleanup(move || timer.cancel());
            });
        }
        // Close at once when disabled.
        watch_effect(move || {
            if disabled.get() && open.get_untracked() {
                open.set(false);
            }
        });
        let shown = computed(move |_| open.get() && !disabled.get());
        let dark = system::is_dark();
        let background = system::mode(Palette::Gray.shade(700), Palette::Gray.shade(300));
        let foreground = system::mode(white_alpha(0.92), Palette::Gray.shade(900));
        let rem = root_font_size().get_untracked();
        let offset = rem * 0.5;
        let arrow_size = rem * 0.5;

        let mut parts = Some((style, label));
        let layer = teleport(Layer::Tooltip, move || {
            // Kept mounted, so the bubble is built once.
            let mut parts = parts.take();
            presence(shown, move || {
                let (style, label) = parts.take().unwrap_or_default();
                let mut children: Vec<Node> = vec![
                    text(label.clone())
                        .font_size(system::font_size::SM)
                        .line_height(Size::Rem(1.25))
                        .font_weight(cosmic_text::Weight::MEDIUM)
                        .color(foreground.clone())
                        .into_node(),
                ];
                if has_arrow {
                    children.push(
                        Arrow {
                            size: arrow_size,
                            color: background.clone(),
                        }
                        .into_node(),
                    );
                }
                anchored(trigger)
                    .placement(placement)
                    .offset(offset + if has_arrow { arrow_size / 2. } else { 0. })
                    .children(
                        flexbox()
                            .relative()
                            .padding_x(Size::Rem(0.5))
                            .padding_y(Size::Rem(0.125))
                            .max_width(Size::Rem(20.))
                            .radius(system::radius::SM)
                            .background(background.clone())
                            .box_shadow(dark.map(|dark| {
                                BoxShadows::from(BoxShadow::new(
                                    0.,
                                    4.,
                                    6.,
                                    black_alpha(if dark { 0.4 } else { 0.15 }),
                                ))
                            }))
                            .children(children)
                            .style(style),
                    )
            })
            .keep_mounted(true)
            .animation(PresenceAnimation::fade().scale(0.9))
            .transition(Transition::ease_out(0.15))
        });

        let hover_timer = timer.clone();
        let press_timer = timer.clone();
        let long_timer = timer;
        flexbox()
            .node_handle(trigger)
            .flex_row()
            .flex_shrink(0.)
            .on_hover_event(move |event| {
                if event.pointer_type() == PointerType::Touch {
                    return;
                }
                match event.action() {
                    PointerAction::Down if !disabled.get_untracked() => {
                        hover_timer.set_after(true, open_delay)
                    }
                    PointerAction::Up => hover_timer.set_after(false, close_delay),
                    _ => {}
                }
            })
            .on_pointer_event(move |event| match (event.pointer_type(), event.action()) {
                // A press closes the tooltip; a long press reopens it.
                (_, PointerAction::Down) => press_timer.set_after(false, 0.),
                (PointerType::Touch, PointerAction::Up | PointerAction::Cancel)
                    if press_timer.open.get_untracked() =>
                {
                    press_timer.set_after(false, TOUCH_LINGER);
                }
                _ => {}
            })
            .on_long_press(move |_| {
                if !disabled.get_untracked() {
                    long_timer.set_after(true, 0.);
                }
            })
            .children((child, layer))
            .build(ctx)
    }
}

/// A small square rotated 45° on the side of the bubble facing the anchor.
struct Arrow {
    size: f32,
    color: Prop<Brush>,
}

impl BuildNode for Arrow {
    fn build(self, ctx: NodeContext) -> Node {
        let Self { size, color } = self;
        // Built inside `anchored`, so its placement is available here.
        let state = use_anchor();
        let half = size / 2.;
        let position = computed(move |_| {
            let Some(state) = state else {
                return (Size::Px(0.), Size::Px(0.), Size::Auto, Size::Auto);
            };
            let along = Size::Px(state.arrow.get() - half);
            let edge = Size::Px(-half);
            match state.placement.get() {
                // Above the anchor: the arrow hangs from the bottom edge.
                Placement::Top | Placement::TopStart | Placement::TopEnd => {
                    (along, Size::Auto, Size::Auto, edge)
                }
                Placement::Bottom | Placement::BottomStart | Placement::BottomEnd => {
                    (along, edge, Size::Auto, Size::Auto)
                }
                Placement::Left | Placement::LeftStart | Placement::LeftEnd => {
                    (Size::Auto, along, edge, Size::Auto)
                }
                Placement::Right | Placement::RightStart | Placement::RightEnd => {
                    (edge, along, Size::Auto, Size::Auto)
                }
            }
        });
        flexbox()
            .absolute()
            .width(Size::Px(size))
            .height(Size::Px(size))
            .inset_left(position.map(|p| p.0))
            .inset_top(position.map(|p| p.1))
            .inset_right(position.map(|p| p.2))
            .inset_bottom(position.map(|p| p.3))
            .background(color)
            .transform(Transform(vec![TransformOp::Rotate(std::f32::consts::FRAC_PI_4)]))
            .build(ctx)
    }
}
