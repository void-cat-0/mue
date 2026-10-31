use super::document::Document;
use mue_macroquad::{node::NodePlan, style::Style};
use std::rc::Rc;

/// Display CommonMark as read-only rich text with reactive content and styles.
#[mue_macros::node]
#[theme]
pub fn markdown(source: Rc<str>) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new(), move |style| {
        super::text::render_text(style, source.map(|source| Document::markdown(&source)))
    })
}
