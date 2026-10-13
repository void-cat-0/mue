use mue_core::{prelude::Access, prop::PropValue};
use taffy::{AvailableSpace, Size};

use crate::{
    layout::use_layout,
    math::{Rect, Vector, vec2},
    paint::use_background,
    paint::use_paint,
    shader::SharedTexture,
    style::Style,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
/// How an image scales within the node's content area.
pub enum ObjectFit {
    #[default]
    Fill,
    Cover,
    Contain,
    ScaleDown,
    None,
}

impl PropValue for ObjectFit {}

/// Display a texture with reactive sizing, cropping, fit, and foreground tint.
/// Without an explicit size, the node uses the texture region's natural size.
#[mue_macros::node]
pub fn image(
    style: &mut Style,
    texture: SharedTexture,
    #[default] object_fit: ObjectFit,
    #[default(vec2(0.5, 0.5))] object_position: Vector,
    #[default] region: Option<Rect>,
) {
    let layout = use_layout(style);
    let rect = layout.rect;
    let paint = use_paint(style);
    use_background(style, rect);
    let color = style.take_color();

    layout.set_measure_fn({
        let texture = texture.clone();
        move |known_dimensions: Size<Option<f32>>, _available_space: Size<AvailableSpace>| {
            let texture = texture.get_clone();
            let region = region
                .get_clone()
                .unwrap_or_else(|| Rect::new(0., 0., texture.width(), texture.height()));

            let Rect { w, h, .. } = region;
            let aspect_ratio = w / h;
            match (known_dimensions.width, known_dimensions.height) {
                (Some(width), Some(height)) => Size { width, height },
                (Some(width), None) => Size {
                    width,
                    height: width / aspect_ratio,
                },
                (None, Some(height)) => Size {
                    width: height * aspect_ratio,
                    height,
                },
                (None, None) => Size {
                    width: w,
                    height: h,
                },
            }
        }
    });

    let shapes = paint.build(move |p| {
        let texture = texture.get_clone();
        let region = region
            .get_clone()
            .unwrap_or_else(|| Rect::new(0., 0., texture.width(), texture.height()));

        let uv_region = Rect::new(
            region.x / texture.width(),
            region.y / texture.height(),
            region.w / texture.width(),
            region.h / texture.height(),
        );
        let (draw_rect, adjusted_uv) = calculate_draw_rect_and_uv(
            object_fit.get(),
            object_position.get(),
            rect.get(),
            region.w,
            region.h,
            uv_region,
        );

        p.draw_texture(
            draw_rect,
            texture,
            adjusted_uv,
            color.get_clone().shader(rect.get()),
        );
    });

    style.on_render.append(move |_| {
        shapes.get_clone().draw();
    });
}

fn calculate_draw_rect_and_uv(
    object_fit: ObjectFit,
    object_position: Vector,
    container: Rect,
    image_width: f32,
    image_height: f32,
    uv_region: Rect,
) -> (Rect, Rect) {
    let container_aspect = container.w / container.h;
    let image_aspect = image_width / image_height;

    let (draw_width, draw_height) = match object_fit {
        ObjectFit::Fill => (container.w, container.h),
        ObjectFit::Cover => {
            if image_aspect > container_aspect {
                let h = container.h;
                (h * image_aspect, h)
            } else {
                let w = container.w;
                (w, w / image_aspect)
            }
        }
        ObjectFit::Contain => {
            if image_aspect > container_aspect {
                let w = container.w;
                (w, w / image_aspect)
            } else {
                let h = container.h;
                (h * image_aspect, h)
            }
        }
        ObjectFit::ScaleDown => {
            if image_aspect > container_aspect {
                let w = container.w.min(image_width);
                (w, w / image_aspect)
            } else {
                let h = container.h.min(image_height);
                (h * image_aspect, h)
            }
        }
        ObjectFit::None => (image_width, image_height),
    };

    let offset_x = (container.w - draw_width) * object_position.x;
    let offset_y = (container.h - draw_height) * object_position.y;
    let draw_rect = Rect::new(
        container.x + offset_x,
        container.y + offset_y,
        draw_width,
        draw_height,
    );
    let draw_rect_clamped = draw_rect.intersect(&container).unwrap();
    let uv_rect = Rect::new(
        uv_region.x + (draw_rect_clamped.x - draw_rect.x) / draw_rect.w * uv_region.w,
        uv_region.y + (draw_rect_clamped.y - draw_rect.y) / draw_rect.h * uv_region.h,
        draw_rect_clamped.w / draw_rect.w * uv_region.w,
        draw_rect_clamped.h / draw_rect.h * uv_region.h,
    );
    (draw_rect_clamped, uv_rect)
}

impl ImageBuilder {
    pub fn object_fill(self) -> Self {
        self.object_fit(ObjectFit::Fill)
    }

    pub fn object_cover(self) -> Self {
        self.object_fit(ObjectFit::Cover)
    }

    pub fn object_contain(self) -> Self {
        self.object_fit(ObjectFit::Contain)
    }

    pub fn object_scale_down(self) -> Self {
        self.object_fit(ObjectFit::ScaleDown)
    }

    pub fn object_none(self) -> Self {
        self.object_fit(ObjectFit::None)
    }
}
