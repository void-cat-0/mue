use super::document::Document;
use mue_macroquad::node::{Node, NodeContext};
use mue_macroquad::style::Style;
use std::rc::Rc;

/// Display CommonMark as read-only rich text with reactive content and styles.
#[mue_macros::node]
pub fn markdown(ctx: NodeContext, source: Rc<str>) -> Node {
    ctx.finish(Style::new(), move |style| {
        super::text::render_text(style, source.map(|source| Document::markdown(&source)));
    })
}
