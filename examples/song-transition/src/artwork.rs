//! One image renderer shared by cards, details and the transition overlay.
use anyhow::Result;
use macroquad::prelude::{Color, FilterMode, Image, Texture2D};
use mue_macroquad::{
    SharedTexture,
    math::Rect,
    paint::{Shapes, with_clip},
    shader::TextureShader,
};
use std::rc::Rc;

pub const CARD_RADIUS: f32 = 18.;
pub const CARD_DIM: f32 = 0.08;
pub const DETAIL_DIM: f32 = 0.42;

pub struct Artwork {
    pub texture: SharedTexture,
}
impl Artwork {
    pub fn load(bytes: &[u8]) -> Result<Rc<Self>> {
        let image = Image::from_file_with_format(bytes, None)?;
        let texture = Texture2D::from_image(&image);
        texture.set_filter(FilterMode::Linear);
        Ok(Rc::new(Self {
            texture: texture.into(),
        }))
    }
    pub fn aspect(&self) -> f32 {
        self.texture.width() / self.texture.height()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub rect: Rect,
    /// Normalized UVs; interpolate the crop, not just the screen-space rectangle.
    pub uv: Rect,
    pub clip: Rect,
    pub radius: f32,
    pub dim: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub surface: Surface,
    pub viewport: Rect,
}
impl Surface {
    pub fn card(rect: Rect, clip: Rect, aspect: f32) -> Self {
        Self {
            rect,
            uv: cover_uv(rect, aspect),
            clip,
            radius: CARD_RADIUS,
            dim: CARD_DIM,
        }
    }
    pub fn fullscreen(viewport: Rect, aspect: f32) -> Self {
        Self {
            rect: viewport,
            uv: cover_uv(viewport, aspect),
            clip: viewport,
            radius: 0.,
            dim: DETAIL_DIM,
        }
    }
    pub fn lerp(self, to: Self, p: f32) -> Self {
        if p <= 0. {
            return self;
        }
        if p >= 1. {
            return to;
        }
        Self {
            rect: lerp_rect(self.rect, to.rect, p),
            uv: lerp_rect(self.uv, to.uv, p),
            clip: lerp_rect(self.clip, to.clip, p),
            radius: self.radius + (to.radius - self.radius) * p,
            dim: self.dim + (to.dim - self.dim) * p,
        }
    }
}
impl Anchor {
    /// Rebase plain display geometry, never read a covered Scene's reactive handles.
    pub fn in_viewport(self, viewport: Rect) -> Surface {
        let sx = viewport.w / self.viewport.w;
        let sy = viewport.h / self.viewport.h;
        let map = |r: Rect| {
            Rect::new(
                viewport.x + (r.x - self.viewport.x) * sx,
                viewport.y + (r.y - self.viewport.y) * sy,
                r.w * sx,
                r.h * sy,
            )
        };
        Surface {
            rect: map(self.surface.rect),
            clip: map(self.surface.clip),
            radius: self.surface.radius * sx.min(sy),
            ..self.surface
        }
    }
}
pub fn lerp_rect(a: Rect, b: Rect, p: f32) -> Rect {
    Rect::new(
        a.x + (b.x - a.x) * p,
        a.y + (b.y - a.y) * p,
        a.w + (b.w - a.w) * p,
        a.h + (b.h - a.h) * p,
    )
}
pub fn ease_out_quart(u: f32) -> f32 {
    1. - (1. - u.clamp(0., 1.)).powi(4)
}
pub fn cover_uv(rect: Rect, aspect: f32) -> Rect {
    let target = rect.w / rect.h;
    if aspect > target {
        let w = target / aspect;
        Rect::new((1. - w) * 0.5, 0., w, 1.)
    } else {
        let h = aspect / target;
        Rect::new(0., (1. - h) * 0.5, 1., h)
    }
}
pub fn draw(artwork: &Artwork, surface: Surface, alpha: f32) {
    if alpha <= 0. || surface.rect.w <= 0. || surface.rect.h <= 0. {
        return;
    }
    let tint = Color::new(
        1. - surface.dim,
        1. - surface.dim,
        1. - surface.dim,
        alpha.clamp(0., 1.),
    );
    with_clip(surface.clip, || {
        Shapes::build(|shapes| {
            shapes.fill_rounded_rect(
                surface.rect,
                surface.radius,
                TextureShader::new(artwork.texture.clone(), surface.uv, surface.rect, tint),
            )
        })
        .draw();
    });
}
