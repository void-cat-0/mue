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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cover_crop_preserves_aspect_and_centers() {
        let wide = cover_uv(Rect::new(0., 0., 400., 200.), 1.5);
        assert_eq!(wide, Rect::new(0., 0.125, 1., 0.75));
        let tall = cover_uv(Rect::new(0., 0., 200., 400.), 1.5);
        assert!((tall.w - 1. / 3.).abs() < 1e-6);
        assert!((tall.x - 1. / 3.).abs() < 1e-6);
    }
    #[test]
    fn shared_surface_has_exact_card_and_fullscreen_endpoints() {
        let card = Surface::card(
            Rect::new(32., 240., 320., 200.),
            Rect::new(32., 140., 900., 520.),
            1.5,
        );
        let full = Surface::fullscreen(Rect::new(0., 0., 1000., 720.), 1.5);
        assert_eq!(card.lerp(full, 0.), card);
        assert_eq!(card.lerp(full, 1.), full);
        let middle = card.lerp(full, 0.5);
        assert!(middle.radius > 0. && middle.radius < CARD_RADIUS);
        assert_eq!(middle.uv, lerp_rect(card.uv, full.uv, 0.5));
    }
    #[test]
    fn source_anchor_resizes_without_scope_handles() {
        let old = Rect::new(0., 0., 1000., 720.);
        let new = Rect::new(0., 0., 1200., 864.);
        let surface = Surface::card(Rect::new(40., 240., 320., 200.), old, 1.5);
        let resized = Anchor {
            surface,
            viewport: old,
        }
        .in_viewport(new);
        assert!((resized.rect.x - 48.).abs() < 1e-5);
        assert!((resized.rect.h - 240.).abs() < 1e-4);
        assert_eq!(resized.uv, surface.uv);
    }
    #[test]
    fn curve_has_fixed_endpoints_and_is_monotone() {
        assert_eq!(ease_out_quart(0.), 0.);
        assert_eq!(ease_out_quart(1.), 1.);
        for i in 0..100 {
            assert!(ease_out_quart(i as f32 / 100.) <= ease_out_quart((i + 1) as f32 / 100.));
        }
    }
}
