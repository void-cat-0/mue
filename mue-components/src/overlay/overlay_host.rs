//! Modal overlay host.

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::focus::FocusScope,
    input::keyboard::KeyCode,
    motion::presence::{PresenceAnimation, presence, use_presence},
    motion::transition::Transition,
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox},
    overlay::layer::{Layer, teleport},
    paint::color::Color,
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

/// A full-screen modal layer with a backdrop and a centered content node.
///
/// The overlay is teleported into [`Layer::Modal`] of the nearest layer host
/// (the App root has one), so it covers the whole window wherever it is
/// declared. It fades in and out, captures pointer input while open, and keeps
/// the content mounted while hidden. The content also rises and scales up as
/// it opens, like Chakra's modal ([`OverlayHostBuilder::content_animation`]).
/// Keyboard focus moves into the content
/// when it opens, Tab stays inside it, and focus returns when it closes. Use
/// `dismissible`/`on_dismiss` for a backdrop tap, Escape, or back (the
/// Android back button, [`back_swipe`](fn@crate::back_swipe)); content taps are
/// swallowed so they do not dismiss the modal. Builder styles apply to the
/// backdrop layer.
pub struct OverlayHostBuilder {
    open: Prop<bool>,
    content: Node,
    style: Style,
    backdrop: Color,
    dismissible: bool,
    on_dismiss: Option<Box<dyn FnMut()>>,
    content_animation: PresenceAnimation,
    transition: Transition,
}

pub fn overlay_host(
    open: impl Into<Prop<bool>>,
    content: impl IntoNode + 'static,
) -> OverlayHostBuilder {
    OverlayHostBuilder {
        open: open.into(),
        content: content.into_node(),
        style: Style::new(),
        backdrop: Color::from_rgba(0, 0, 0, 170),
        dismissible: false,
        on_dismiss: None,
        content_animation: PresenceAnimation::none().scale(0.95).slide(0., 12.),
        transition: Transition::ease_out(0.22),
    }
}

impl OverlayHostBuilder {
    pub fn backdrop(mut self, color: Color) -> Self {
        self.backdrop = color;
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    pub fn on_dismiss(mut self, action: impl FnMut() + 'static) -> Self {
        self.on_dismiss = Some(Box::new(action));
        self
    }

    /// How the content moves while the overlay opens and closes; it always
    /// fades with the backdrop. The default scales from 0.95 and rises 12
    /// logical pixels. `PresenceAnimation::none()` keeps it still; `fade`
    /// fades it a second time, faster than the backdrop.
    pub fn content_animation(mut self, animation: PresenceAnimation) -> Self {
        self.content_animation = animation;
        self
    }

    /// Curve and duration of opening and closing.
    pub fn transition(mut self, transition: Transition) -> Self {
        self.transition = transition;
        self
    }
}

impl Styleable for OverlayHostBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for OverlayHostBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            open,
            content,
            style,
            backdrop,
            dismissible,
            on_dismiss,
            content_animation,
            transition,
        } = self;
        type Action = Option<Box<dyn FnMut()>>;
        let dismiss: Rc<RefCell<Action>> = Rc::new(RefCell::new(on_dismiss));
        let fire = move || {
            if dismissible && let Some(action) = dismiss.borrow_mut().as_mut() {
                action();
            }
        };
        let mut content = Some(content);
        // Back (the Android back button, an edge swipe) closes it too.
        if dismissible {
            mue_macroquad::input::back::use_back_handler(open, fire.clone());
        }
        teleport(Layer::Modal, move || {
            let on_escape = fire.clone();
            let on_backdrop = fire.clone();
            presence(open, move || {
                let content = content.take().expect("kept mounted, built once");
                let progress = use_presence().map(|state| state.progress);
                let shown = move || progress.map_or(1., |p| p.get().clamp(0., 1.));
                let animation = content_animation;
                let mut wrapper = flexbox().transform(computed(move |_| {
                    let p = shown();
                    let hidden = 1. - p;
                    let scale = animation.scale + (1. - animation.scale) * p;
                    Transform(vec![
                        TransformOp::Translate(
                            Size::Px(animation.offset.0 * hidden),
                            Size::Px(animation.offset.1 * hidden),
                        ),
                        TransformOp::Scale(scale, scale),
                    ])
                }));
                if animation.fade {
                    wrapper = wrapper.opacity(computed(move |_| shown()));
                }
                wrapper
                    .focus_scope(FocusScope::Contain)
                    .auto_focus(true)
                    .on_tap(|_| {})
                    .on_key_down({
                        let on_escape = on_escape.clone();
                        move |event| {
                            if event.key == KeyCode::Escape {
                                event.stop_propagation();
                                on_escape();
                            }
                        }
                    })
                    .children(content)
            })
            .keep_mounted(true)
            .animation(PresenceAnimation::fade())
            .transition(transition)
            .style(style)
            .absolute()
            .inset((Size::Px(0.),))
            .items_center()
            .justify_center()
            .background(backdrop)
            .on_pointer_event(|_| {})
            .on_activate(on_backdrop)
        })
        .build(ctx)
    }
}
