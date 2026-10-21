use super::{document::Document, text::render_text};
use mue_core::signal::Access;
use mue_macroquad::{
    math::Rect,
    paint::use_paint,
    style::{Style, Styleable},
};
use std::rc::Rc;

/// An italic quotation with an inset rule using the foreground brush.
#[mue_macros::node]
pub fn blockquote(style: &mut Style, text: Rc<str>) {
    style.provide_defaults(Style::new().font_italic(true).padding_left(18.));
    let color = style
        .declared_color()
        .or_else(mue_macroquad::theme::inherited_foreground)
        .unwrap_or_else(|| mue_macroquad::color::Color::WHITE.into());
    let paint = use_paint(style);
    render_text(style, text.map(|text| Document::plain(text.to_string())));
    let rect = mue_macroquad::node::use_rect();
    style.on_render.append(move |_| {
        let bounds = rect.get();
        paint.draw(|p| {
            p.fill_rect(
                Rect::new(bounds.x, bounds.y, 3., bounds.h),
                color.get_clone().shader(bounds),
            )
        });
    });
}
