use super::{document::Document, text::render_text};
use mue_macroquad::{
    color::Color,
    style::{Style, Styleable},
};
use std::rc::Rc;

/// Underlined action text. Supply `on_activate` to navigate or open a URL using your platform.
/// Mue does not launch browsers implicitly.
#[mue_macros::node]
pub fn link(style: &mut Style, text: Rc<str>) {
    style.provide_defaults(
        Style::new()
            .color(Color::from_rgba(96, 165, 250, 255))
            .text_underline(true),
    );
    render_text(style, text.map(|text| Document::plain(text.to_string())));
}
