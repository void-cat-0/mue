use super::{document::Document, text::render_text};
use cosmic_text::FamilyOwned;
use mue_macroquad::{
    color::Color,
    node::NodePlan,
    style::{Style, Styleable},
};
use std::rc::Rc;

/// Preformatted monospace text without syntax highlighting.
/// Long lines remain intact; put inside a horizontal scroll area or set `text_wrap(true)`.
#[mue_macros::node]
#[theme]
pub fn code_block(text: Rc<str>) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(
        Style::new()
            .font_family(FamilyOwned::Monospace)
            .text_wrap(false)
            .padding((16.,))
            .radius(6.)
            .background(Color::from_rgba(128, 128, 128, 28)),
        move |style| render_text(style, text.map(|text| Document::plain(text.to_string()))),
    )
}
