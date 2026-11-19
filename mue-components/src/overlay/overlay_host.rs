//! Modal overlay host.

use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, flexbox, flexbox::use_container,
        show_if,
    },
    style::{Size, Style, Styleable, StyleableExt},
};
use taffy::Display;

/// A full-screen modal layer with a backdrop and a centered content node.
///
/// The host is hidden when closed, captures pointer input while open, and
/// keeps the content mounted while hidden. Use dismissible/on_dismiss for a
/// backdrop tap; content taps are swallowed so they do not dismiss the modal.
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
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            open,
            content,
            style,
            backdrop,
            dismissible,
            on_dismiss,
        } = self;

        let display = computed(move |_| {
            if open.try_get().unwrap_or(false) {
                Display::Flex
            } else {
                Display::None
            }
        });
        let style = style.display(display);
        ctx.push_style(style);

        let defaults = Style::new()
            .absolute()
            .inset((Size::Px(0.),))
            .items_center()
            .justify_center();

        ctx.finish(defaults, move |style| {
            let mut layer = flexbox()
                .style(
                    Style::new()
                        .absolute()
                        .inset((Size::Px(0.),))
                        .items_center()
                        .justify_center()
                        .background(backdrop),
                )
                .on_pointer_event(|_| {});

            if dismissible {
                let mut on_dismiss = on_dismiss;
                layer = layer.on_activate(move || {
                    if let Some(action) = on_dismiss.as_mut() {
                        action();
                    }
                });
            }

            let content = flexbox().on_tap(|_| {}).children(content).into_node();
            let layer = layer.children(content).into_node();
            mue_macroquad::node::set_children(show_if(open, layer).into_children());
            use_container(style);
        })
    }
}
