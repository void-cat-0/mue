use super::{document::Document, text::render_text};
use crate::{
    math::Rect,
    node::NodeInner,
    paint::use_paint,
    style::{Style, Styleable},
};
use mue_core::signal::Access;
use std::rc::Rc;

/// An italic quotation with an inset rule using the foreground brush.
#[mue_macros::node]
pub fn blockquote(style: &mut Style, text: Rc<str>) {
    style.provide_defaults(Style::new().font_italic(true).padding_left(18.));
    let color = style
        .color
        .clone()
        .unwrap_or_else(|| crate::color::Color::WHITE.into());
    let paint = use_paint(style);
    render_text(style, text.map(|text| Document::plain(text.to_string())));
    let rect = NodeInner::with_mut(|node| *node.layout.rect);
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
