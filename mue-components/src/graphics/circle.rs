use mue_core::signal::Access;

use mue_macroquad::{
    layout::{Layout, use_layout},
    paint::use_paint,
    style::Style,
};

/// Fill an inscribed circle using the node's background brush.
#[mue_macros::node]
pub fn circle(style: &mut Style) {
    let Layout { rect, .. } = use_layout(style);
    let paint = use_paint(style);
    let color = style
        .declared_background()
        .unwrap_or_else(|| mue_macroquad::color::Color::WHITE.into());
    let shapes = paint.build(move |p| {
        let r = rect.get();
        p.fill_circle(r.center(), r.w.min(r.h) / 2., color.get_clone().shader(r));
    });
    style.on_render.append(move |_| {
        shapes.get_clone().draw();
    });
}
