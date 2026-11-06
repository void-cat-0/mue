use super::{document::Document, text::render_text};
use mue_macroquad::node::{Node, NodeContext};
use mue_macroquad::style::{Style, Styleable};
use std::rc::Rc;

/// Emphasized text using the current font's italic face.
#[mue_macros::node]
pub fn em(ctx: NodeContext, text: Rc<str>) -> Node {
    ctx.finish(Style::new().font_italic(true), move |style| {
        render_text(style, text.map(|text| Document::plain(text.to_string())));
    })
}
