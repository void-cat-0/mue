use super::document::Document;
use crate::style::Style;
use std::rc::Rc;

/// Display CommonMark as read-only rich text with reactive content and styles.
#[mue_macros::node]
pub fn markdown(style: &mut Style, source: Rc<str>) {
    super::text::render_text(style, source.map(|source| Document::markdown(&source)));
}
