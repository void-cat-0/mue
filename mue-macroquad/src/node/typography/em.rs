use super::{document::Document, text::render_text};
use crate::style::{Style, Styleable};
use std::rc::Rc;

/// Emphasized text using the current font's italic face.
#[mue_macros::node]
pub fn em(style: &mut Style, text: Rc<str>) {
    style.provide_defaults(Style::new().font_italic(true));
    render_text(style, text.map(|text| Document::plain(text.to_string())));
}
