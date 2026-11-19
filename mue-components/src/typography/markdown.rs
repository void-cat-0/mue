use super::document::Document;
use mue_macroquad::node::{Node, NodeContext};
use mue_macroquad::style::Style;
use std::rc::Rc;

/// Display CommonMark as read-only rich text with reactive source and styles.
///
/// CommonMark emphasis, strong text, headings, code, quotes, and lists are
/// mapped to span styles. Links and images display their labels; HTML is literal
/// text. This is not an interactive document or syntax highlighter.
///
/// ```
/// use mue_components::prelude::*;
///
/// let source = "# Heading\n\nA **styled** paragraph.";
/// let _view = markdown(source).font_size(18.0);
/// ```
#[mue_macros::node]
pub fn markdown(ctx: NodeContext, source: Rc<str>) -> Node {
    ctx.finish(Style::new(), move |style| {
        super::text::render_text(style, source.map(|source| Document::markdown(&source)));
    })
}
