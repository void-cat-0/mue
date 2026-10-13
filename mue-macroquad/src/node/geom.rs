use std::rc::Rc;

use mue_core::signal::Access;

use crate::{
    layout::{Layout, use_layout},
    paint::{Shapes, use_paint},
    style::Style,
};

#[mue_macros::node]
pub fn circle(style: &mut Style) {
    let Layout { rect, .. } = use_layout(style);
    let paint = use_paint(style);
    let color = style
        .background
        .take()
        .unwrap_or_else(|| crate::color::Color::WHITE.into());
    let shapes = paint.build(move |p| {
        let r = rect.get();
        p.fill_circle(r.center(), r.w.min(r.h) / 2., color.get_clone().shader(r));
    });
    style.on_render.append(move |_| {
        shapes.get_clone().draw();
    });
}

#[mue_macros::node]
pub fn shape(style: &mut Style, shapes: Rc<Shapes>) {
    use_layout(style);
    style.on_render.append(move |_| {
        shapes.get_clone().draw();
    });
}
