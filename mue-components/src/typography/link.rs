use super::{document::Document, text::render_text};
use mue_macroquad::{
    color::Color,
    node::NodePlan,
    style::{Style, Styleable},
};
use std::rc::Rc;

/// Underlined action text. Supply `on_activate` to navigate or open a URL using your platform.
/// Mue does not launch browsers implicitly.
#[mue_macros::node]
#[theme]
pub fn link(text: Rc<str>) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(
        Style::new()
            .color(Color::from_rgba(96, 165, 250, 255))
            .text_underline(true),
        move |style| render_text(style, text.map(|text| Document::plain(text.to_string()))),
    )
}
