use std::{ops::Deref, rc::Rc};

use macroquad::prelude::*;
use mue_core::prop::PropValue;

use crate::math::{vec2, Matrix, Rect, Vector};

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

#[repr(transparent)]
struct Inner(Texture2D);

#[derive(Clone)]
#[repr(transparent)]
pub struct SharedTexture(Rc<Inner>);

impl PartialEq for SharedTexture {
    fn eq(&self, other: &Self) -> bool {
        self.0 .0.raw_miniquad_id() == other.0 .0.raw_miniquad_id()
    }
}
impl Eq for SharedTexture {}

impl Deref for SharedTexture {
    type Target = Texture2D;

    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}

impl From<Texture2D> for SharedTexture {
    fn from(value: Texture2D) -> Self {
        Self(Rc::new(Inner(value)))
    }
}

impl PropValue for SharedTexture {}

pub trait Shader {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex;

    fn texture(&self) -> Option<SharedTexture>;
}

impl PropValue for TextureShader {}

#[derive(Clone)]
pub struct GradientShader {
    origin: (f32, f32),
    color: Color,
    vector: (f32, f32),
    color_end: Color,
}

impl Shader for GradientShader {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        let t = mat.transform_point2(*p);
        let mut color = {
            let (dx, dy) = (p.x - self.origin.0, p.y - self.origin.1);
            lerp_color(
                self.color,
                self.color_end,
                dx * self.vector.0 + dy * self.vector.1,
            )
        };
        color.a *= alpha;

        Vertex::new(t.x, t.y, 0., 0., 0., color)
    }

    fn texture(&self) -> Option<SharedTexture> {
        None
    }
}

#[derive(Clone)]
pub struct TextureShader {
    pub texture: SharedTexture,
    pub texture_region: Rect,
    pub draw_rect: Rect,
    pub color: Color,
}

impl TextureShader {
    pub fn new(
        texture: SharedTexture,
        texture_region: Rect,
        draw_rect: Rect,
        color: Color,
    ) -> Self {
        Self {
            texture,
            texture_region,
            draw_rect,
            color,
        }
    }
}

impl Shader for TextureShader {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        let t = mat.transform_point2(*p);
        let tr = self.texture_region;
        let dr = self.draw_rect;
        let ux = (p.x - dr.x) / dr.w;
        let uy = (p.y - dr.y) / dr.h;
        Vertex::new(
            t.x,
            t.y,
            0.,
            tr.x + tr.w * ux,
            tr.y + tr.h * uy,
            Color {
                a: self.color.a * alpha,
                ..self.color
            },
        )
    }

    fn texture(&self) -> Option<SharedTexture> {
        Some(self.texture.clone())
    }
}

pub struct RadialShader {
    origin: Vector,
    radius: f32,
    color: Color,
    color_end: Color,
}

impl Shader for RadialShader {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        let e = (p - self.origin).length() / self.radius;
        let mut color = lerp_color(self.color, self.color_end, e);
        color.a *= alpha;
        let t = mat.transform_point2(*p);
        Vertex::new(t.x, t.y, 0., 0., 0., color)
    }

    fn texture(&self) -> Option<SharedTexture> {
        None
    }
}

pub trait IntoShader {
    type Target: Shader;

    fn into_shader(self) -> Self::Target;
}

impl<T: Shader> IntoShader for T {
    type Target = T;

    fn into_shader(self) -> Self::Target {
        self
    }
}

impl IntoShader for Color {
    type Target = GradientShader;

    fn into_shader(self) -> Self::Target {
        GradientShader {
            origin: (0., 0.),
            color: self,
            vector: (1., 0.),
            color_end: self,
        }
    }
}

impl IntoShader for (Color, (f32, f32), Color, (f32, f32)) {
    type Target = GradientShader;

    fn into_shader(self) -> Self::Target {
        let (color, origin, color_end, end) = self;
        let vector = (end.0 - origin.0, end.1 - origin.1);
        let norm = vector.0.hypot(vector.1);
        let vector = (vector.0 / norm, vector.1 / norm);
        let color_end = lerp_color(color, color_end, 1. / norm);
        GradientShader {
            origin,
            color,
            vector,
            color_end,
        }
    }
}

impl IntoShader for (Color, (f32, f32), Color, f32) {
    type Target = RadialShader;

    fn into_shader(self) -> Self::Target {
        let (color, origin, color_end, radius) = self;
        RadialShader {
            origin: vec2(origin.0, origin.1),
            radius,
            color,
            color_end,
        }
    }
}
