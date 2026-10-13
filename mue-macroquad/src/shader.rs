use std::{ops::Deref, rc::Rc};

use macroquad::{models::Vertex, texture::Texture2D};
use mue_core::prop::{Prop, PropValue};

use crate::color::Color;
use crate::math::{Matrix, Rect, Vector, vec2};

pub struct BrushShader {
    brush: Brush,
    bounds: Rect,
}
impl BrushShader {
    fn new(brush: Brush, bounds: Rect) -> Self {
        Self { brush, bounds }
    }
}
impl Shader for BrushShader {
    fn subdivisions(&self, rect: Rect) -> (usize, usize) {
        match self.brush {
            Brush::Solid(_) => (1, 1),
            _ => (
                (rect.w.abs() / 6.).ceil().clamp(1., 32.) as usize,
                (rect.h.abs() / 6.).ceil().clamp(1., 32.) as usize,
            ),
        }
    }
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        self.brush
            .color_at(*p, self.bounds)
            .new_vertex(mat, p, alpha)
    }
    fn texture(&self) -> Option<SharedTexture> {
        None
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct SharedTexture(Rc<Texture2D>);

impl PartialEq for SharedTexture {
    fn eq(&self, other: &Self) -> bool {
        self.raw_miniquad_id() == other.raw_miniquad_id()
    }
}
impl Eq for SharedTexture {}

impl Deref for SharedTexture {
    type Target = Texture2D;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Texture2D> for SharedTexture {
    fn from(value: Texture2D) -> Self {
        Self(Rc::new(value))
    }
}

impl PropValue for SharedTexture {}

pub trait Shader {
    /// Extra samples for rectangular fills and texture tints with interior detail.
    fn subdivisions(&self, _rect: Rect) -> (usize, usize) {
        (1, 1)
    }

    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex;

    fn texture(&self) -> Option<SharedTexture>;
}

impl PropValue for TextureShader {}

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
        color: impl Into<Color>,
    ) -> Self {
        Self {
            texture,
            texture_region,
            draw_rect,
            color: color.into(),
        }
    }
}

impl Shader for TextureShader {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        let mut vertex = self.color.new_vertex(mat, p, alpha);
        let tr = self.texture_region;
        let dr = self.draw_rect;
        vertex.uv = macroquad::math::vec2(
            tr.x + tr.w * (p.x - dr.x) / dr.w,
            tr.y + tr.h * (p.y - dr.y) / dr.h,
        );
        vertex
    }

    fn texture(&self) -> Option<SharedTexture> {
        Some(self.texture.clone())
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

/// A fill shared by foreground `color` and `background` styles.
/// New fill kinds can be added here without changing either style property.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Brush {
    Solid(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

impl PropValue for Brush {}

impl crate::animation::Animatable for Brush {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let stops = |a: &[ColorStop], b: &[ColorStop]| {
            a.iter()
                .zip(b)
                .map(|(a, b)| {
                    ColorStop::new(
                        f32::interpolate(&a.offset, &b.offset, t),
                        a.color.lerp(b.color, t),
                    )
                })
                .collect::<Vec<_>>()
        };
        match (from, to) {
            (Self::Solid(a), Self::Solid(b)) => Self::Solid(a.lerp(*b, t)),
            (Self::LinearGradient(a), Self::LinearGradient(b))
                if a.stops.len() == b.stops.len() =>
            {
                Self::LinearGradient(LinearGradient::new(
                    a.start.lerp(b.start, t),
                    a.end.lerp(b.end, t),
                    stops(&a.stops, &b.stops),
                ))
            }
            (Self::RadialGradient(a), Self::RadialGradient(b))
                if a.stops.len() == b.stops.len() =>
            {
                Self::RadialGradient(RadialGradient::new(
                    a.center.lerp(b.center, t),
                    f32::interpolate(&a.radius, &b.radius, t),
                    stops(&a.stops, &b.stops),
                ))
            }
            _ => to.clone(),
        }
    }
}

impl Default for Brush {
    fn default() -> Self {
        Self::Solid(Color::TRANSPARENT)
    }
}

macro_rules! impl_brush_from {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for Brush {
            fn from(value: $ty) -> Self {
                Self::$variant(value)
            }
        }
        impl From<$ty> for Prop<Brush> {
            fn from(value: $ty) -> Self {
                Self::Static(value.into())
            }
        }
    };
}
impl_brush_from!(Color, Solid);
impl_brush_from!(LinearGradient, LinearGradient);
impl_brush_from!(RadialGradient, RadialGradient);

impl From<macroquad::color::Color> for Brush {
    fn from(value: macroquad::color::Color) -> Self {
        Self::Solid(value.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub offset: f32,
    pub color: Color,
}

impl ColorStop {
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}

/// Endpoints are fractions of the painted bounds, e.g. (0, 0) to (1, 0).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Vector,
    pub end: Vector,
    stops: Vec<ColorStop>,
}

impl LinearGradient {
    pub fn new(start: Vector, end: Vector, stops: impl Into<Vec<ColorStop>>) -> Self {
        Self {
            start,
            end,
            stops: sorted_stops(stops.into()),
        }
    }
}

/// Center is a fraction of the bounds; radius is a fraction of its shortest side.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub center: Vector,
    pub radius: f32,
    stops: Vec<ColorStop>,
}

impl RadialGradient {
    pub fn new(center: Vector, radius: f32, stops: impl Into<Vec<ColorStop>>) -> Self {
        Self {
            center,
            radius,
            stops: sorted_stops(stops.into()),
        }
    }
}

fn sorted_stops(mut stops: Vec<ColorStop>) -> Vec<ColorStop> {
    for stop in &mut stops {
        stop.offset = stop.offset.clamp(0., 1.);
    }
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    stops
}

fn sample(stops: &[ColorStop], t: f32) -> Color {
    let Some(first) = stops.first() else {
        return Color::TRANSPARENT;
    };
    if t < first.offset {
        return first.color;
    }
    for pair in stops.windows(2) {
        let [a, b] = pair else { unreachable!() };
        if t < b.offset {
            return a
                .color
                .lerp(b.color, (t - a.offset) / (b.offset - a.offset));
        }
    }
    stops.last().unwrap().color
}

impl Brush {
    pub fn shader(&self, bounds: Rect) -> BrushShader {
        BrushShader::new(self.clone(), bounds)
    }

    pub(crate) fn color_at(&self, point: Vector, bounds: Rect) -> Color {
        let position =
            |p: Vector| Vector::new(bounds.x + p.x * bounds.w, bounds.y + p.y * bounds.h);
        match self {
            Self::Solid(color) => *color,
            Self::LinearGradient(gradient) => {
                let start = position(gradient.start);
                let direction = position(gradient.end) - start;
                let length_squared = direction.length_squared();
                let t = if length_squared > 0. {
                    (point - start).dot(direction) / length_squared
                } else {
                    1.
                };
                sample(&gradient.stops, t)
            }
            Self::RadialGradient(gradient) => {
                let radius = gradient.radius * bounds.w.min(bounds.h);
                let t = if radius > 0. {
                    (point - position(gradient.center)).length() / radius
                } else {
                    1.
                };
                sample(&gradient.stops, t)
            }
        }
    }
}

impl Shader for Color {
    fn new_vertex(&self, mat: &Matrix, p: &Vector, alpha: f32) -> Vertex {
        let point = mat.transform_point2(*p);
        let color = Color {
            a: self.a * alpha,
            ..*self
        };
        Vertex::new(point.x, point.y, 0., 0., 0., color.into())
    }

    fn texture(&self) -> Option<SharedTexture> {
        None
    }
}

impl IntoShader for macroquad::color::Color {
    type Target = Color;
    fn into_shader(self) -> Color {
        self.into()
    }
}

// The tuple API uses absolute coordinates; Brush gradients use fractions of bounds.
// Both adapters reuse BrushShader and Color::lerp.
pub type GradientShader = BrushShader;
pub type RadialShader = BrushShader;

impl<C: Into<Color>> IntoShader for (C, (f32, f32), C, (f32, f32)) {
    type Target = BrushShader;
    fn into_shader(self) -> Self::Target {
        let (start_color, start, end_color, end) = self;
        Brush::LinearGradient(LinearGradient::new(
            vec2(start.0, start.1),
            vec2(end.0, end.1),
            vec![
                ColorStop::new(0., start_color.into()),
                ColorStop::new(1., end_color.into()),
            ],
        ))
        .shader(Rect::new(0., 0., 1., 1.))
    }
}

impl<C: Into<Color>> IntoShader for (C, (f32, f32), C, f32) {
    type Target = BrushShader;
    fn into_shader(self) -> Self::Target {
        let (start_color, center, end_color, radius) = self;
        Brush::RadialGradient(RadialGradient::new(
            vec2(center.0, center.1),
            radius,
            vec![
                ColorStop::new(0., start_color.into()),
                ColorStop::new(1., end_color.into()),
            ],
        ))
        .shader(Rect::new(0., 0., 1., 1.))
    }
}
