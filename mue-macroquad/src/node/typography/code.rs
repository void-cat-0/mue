use super::{document::Document, text::render_text};
use crate::{
    color::Color,
    style::{Style, Styleable},
};
use cosmic_text::FamilyOwned;
use std::rc::Rc;

/// A short monospace code fragment. For inline rich text use `Span::monospace`.
#[mue_macros::node]
pub fn code(style: &mut Style, text: Rc<str>) {
    style.provide_defaults(
        Style::new()
            .font_family(FamilyOwned::Monospace)
            .padding_x(6.)
            .padding_y(2.)
            .radius(4.)
            .background(Color::from_rgba(128, 128, 128, 28)),
    );
    render_text(style, text.map(|text| Document::plain(text.to_string())));
}
