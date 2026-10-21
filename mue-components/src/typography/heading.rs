use super::{document::Document, text::render_text};
use cosmic_text::Weight;
use mue_core::prop::PropValue;
use mue_macroquad::style::{Style, Styleable};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeadingLevel {
    H1,
    #[default]
    H2,
    H3,
    H4,
    H5,
    H6,
}
impl PropValue for HeadingLevel {}

impl HeadingLevel {
    pub(crate) fn scale(self) -> f32 {
        match self {
            Self::H1 => 1.8,
            Self::H2 => 1.45,
            Self::H3 => 1.2,
            Self::H4 => 1.,
            Self::H5 => 0.85,
            Self::H6 => 0.75,
        }
    }
}

/// A bold heading. Font size and line height remain individually styleable.
#[mue_macros::node]
pub fn heading(style: &mut Style, text: Rc<str>, #[default] level: HeadingLevel) {
    style.provide_defaults(
        Style::new()
            .font_weight(Weight::BOLD)
            .font_size(level.map(|level| 32. * level.scale()))
            .line_height(level.map(|level| 44. * level.scale())),
    );
    render_text(style, text.map(|text| Document::plain(text.to_string())));
}
