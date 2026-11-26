//! Modal overlay host.

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    event::keyboard::KeyCode,
    focus::FocusScope,
    layer::{Layer, teleport},
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox},
    presence::{PresenceAnimation, presence},
    style::{Size, Style, Styleable, StyleableExt},
    transition::Transition,
};

/// A full-screen modal layer with a backdrop and a centered content node.
///
/// The overlay is teleported into [`Layer::Modal`] of the nearest layer host
/// (the App root has one), so it covers the whole window wherever it is
/// declared. It fades in and out, captures pointer input while open, and keeps
/// the content mounted while hidden. Keyboard focus moves into the content
/// when it opens, Tab stays inside it, and focus returns when it closes. Use
/// `dismissible`/`on_dismiss` for a backdrop tap or Escape; content taps are
/// swallowed so they do not dismiss the modal. Builder styles apply to the
/// backdrop layer.
pub struct OverlayHostBuilder {
    open: Prop<bool>,
    content: Node,
    style: Style,
    backdrop: Color,
    dismissible: bool,
    on_dismiss: Option<Box<dyn FnMut()>>,
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
        } = self;
        type Action = Option<Box<dyn FnMut()>>;
        let dismiss: Rc<RefCell<Action>> = Rc::new(RefCell::new(on_dismiss));
        let fire = move || {
            if dismissible && let Some(action) = dismiss.borrow_mut().as_mut() {
                action();
            }
        };
        let mut content = Some(content);
        teleport(Layer::Modal, move || {
            let on_escape = fire.clone();
            let on_backdrop = fire.clone();
            presence(open, move || {
                let content = content.take().expect("kept mounted, built once");
                flexbox()
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
            .transition(Transition::ease_out(0.16))
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
