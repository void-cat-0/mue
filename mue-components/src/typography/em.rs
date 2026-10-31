use super::{document::Document, text::render_text};
use mue_macroquad::{
    node::NodePlan,
    style::{Style, Styleable},
};
use std::rc::Rc;

/// Emphasized text using the current font's italic face.
#[mue_macros::node]
#[theme]
pub fn em(text: Rc<str>) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().font_italic(true), move |style| {
        render_text(style, text.map(|text| Document::plain(text.to_string())))
    })
}
