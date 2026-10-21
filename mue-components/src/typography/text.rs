use super::document::Document;
use mue_core::prop::Prop;
pub use mue_macroquad::node::text::{TextOverflow, TextVerticalAlign, TextWrap, text};
use mue_macroquad::style::Style;

pub(super) fn render_text(style: &mut Style, content: Prop<Document>) {
    mue_macroquad::node::text::render_spans(style, content.map(|document| document.spans));
}
