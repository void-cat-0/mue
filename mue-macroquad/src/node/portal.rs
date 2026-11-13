use macroquad::color::WHITE;
use mue_core::prelude::*;

use crate::{
    layout::use_layout,
    math::Rect,
    node::{Node, NodeContext},
    paint::use_paint,
    shader::SharedTexture,
    style::Style,
};

/// The normalized UV rect used to draw a render-target texture.
///
/// Render targets are upside down relative to macroquad's screen coordinates,
/// so the default portal flips V. A negative height is valid here because
/// `ShapesBuilder::draw_texture` interpolates `uv.y + uv.h * v`.
fn portal_uv(flip_y: bool) -> Rect {
    if flip_y {
        Rect::new(0., 1., 1., -1.)
    } else {
        Rect::new(0., 0., 1., 1.)
    }
}

/// Draw an externally rendered texture into this node's layout rect.
///
/// This is a pure texture consumer: the channel renders to its target before
/// `App::frame`, publishes a `SharedTexture`, and the portal composites it with
/// mue's normal paint transform, clip, and opacity. The portal does not own a
/// channel or change the render pass.
///
/// `flip_y` defaults to `true` because a macroquad render target is stored
/// upside down relative to the screen. Pass `false` for a texture that is
/// already in screen orientation.
///
/// The portal fills its layout rect and stretches the texture. Use an
/// `aspect_ratio` container or an explicit rect to preserve the chart aspect
/// ratio.
#[mue_macros::node]
pub fn portal(ctx: NodeContext, texture: SharedTexture, #[default(true)] flip_y: bool) -> Node {
    ctx.finish(Style::new(), move |style| {
        let rect = use_layout(style).rect;
        let paint = use_paint(style);
        let shapes = paint.build(move |p| {
            let Some(texture) = texture.try_get_clone() else {
                return;
            };
            p.draw_texture(rect.get(), texture, portal_uv(flip_y.get()), WHITE);
        });
        style.on_render.append(move |_| shapes.get_clone().draw());
    })
}

#[cfg(test)]
mod tests {
    use super::portal_uv;
    use crate::math::Rect;

    #[test]
    fn uv_is_screen_oriented_by_default() {
        assert_eq!(portal_uv(false), Rect::new(0., 0., 1., 1.));
    }

    #[test]
    fn uv_flips_vertically_for_render_targets() {
        assert_eq!(portal_uv(true), Rect::new(0., 1., 1., -1.));
    }
}
