//! Rounded and sector clipping for immediate UI subtrees.
//!
//! A clip is a material scope: it replaces the current pipeline with an alpha
//! mask while the subtree draws. Retained geometry uses an active retained
//! material; it falls back to immediate only when that material is unavailable.

use std::{
    cell::{OnceCell, RefCell},
    ops::Range,
};

use macroquad::{
    math::{Mat4, vec2, vec4},
    retained::{RetainedMaterial, RetainedMesh},
};
use mue_core::prop::PropValue;

use crate::{
    material::UiMaterial,
    math::{Rect, Vector, model_matrix},
    paint::{current_transform, enter_material_clip},
};

const TAU: f32 = std::f32::consts::TAU;

const CLIP_FRAGMENT: &str = r#"#version 100
precision highp float;

varying highp vec2 v_pos;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;

uniform highp vec4 rect;
uniform highp float radius;
uniform highp mat4 inverse_model;
uniform sampler2D Texture;

void main() {
    vec2 local = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    vec2 half_size = (rect.zw - rect.xy) * 0.5;
    vec2 center = rect.xy + half_size;
    vec2 q = abs(local - center) - half_size + vec2(radius);
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    float mask = 1.0 - smoothstep(-0.5, 0.5, d);
    vec4 tex = texture2D(Texture, v_uv);
    gl_FragColor = tex * v_color;
    gl_FragColor.a *= mask;
}
"#;

thread_local! {
    static CLIP_MATERIAL: OnceCell<UiMaterial> = const { OnceCell::new() };
    static RETAINED_CLIP: OnceCell<Option<RetainedMaterial>> = const { OnceCell::new() };
    static RETAINED_SECTOR: OnceCell<Option<RetainedMaterial>> = const { OnceCell::new() };
    static ACTIVE_RETAINED: RefCell<Vec<RetainedMaterial>> = const { RefCell::new(Vec::new()) };
}

struct RetainedGuard;

impl Drop for RetainedGuard {
    fn drop(&mut self) {
        ACTIVE_RETAINED.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn push_retained(material: RetainedMaterial) -> RetainedGuard {
    ACTIVE_RETAINED.with(|stack| stack.borrow_mut().push(material));
    RetainedGuard
}

pub(crate) fn retained_material_active() -> bool {
    ACTIVE_RETAINED.with(|stack| !stack.borrow().is_empty())
}

fn retained_clip_material() -> Option<RetainedMaterial> {
    RETAINED_CLIP.with(|cell| {
        cell.get_or_init(|| {
            RetainedMaterial::new(
                CLIP_FRAGMENT,
                &[
                    macroquad::miniquad::UniformDesc::new(
                        "rect",
                        macroquad::miniquad::UniformType::Float4,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "radius",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "inverse_model",
                        macroquad::miniquad::UniformType::Mat4,
                    ),
                ],
                &[],
            )
        })
        .clone()
    })
}

fn retained_sector_material() -> Option<RetainedMaterial> {
    RETAINED_SECTOR.with(|cell| {
        cell.get_or_init(|| {
            RetainedMaterial::new(
                SECTOR_FRAGMENT,
                &[
                    macroquad::miniquad::UniformDesc::new(
                        "center",
                        macroquad::miniquad::UniformType::Float2,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "angle",
                        macroquad::miniquad::UniformType::Float2,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "feather",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "inverse_model",
                        macroquad::miniquad::UniformType::Mat4,
                    ),
                ],
                &[],
            )
        })
        .clone()
    })
}

/// Draw retained meshes, using the active clip material when one is set.
pub(crate) fn draw_retained_meshes_ranges(
    meshes: &[RetainedMesh],
    ranges: &[Option<Range<u32>>],
    model: Mat4,
    alpha: f32,
) {
    ACTIVE_RETAINED.with(|stack| {
        let stack = stack.borrow();
        if let Some(material) = stack.last() {
            macroquad::retained::draw_meshes_ranges_with_material(
                meshes, ranges, model, alpha, material,
            );
        } else {
            macroquad::retained::draw_meshes_ranges(meshes, ranges, model, alpha);
        }
    });
}

pub(crate) fn draw_retained_meshes(meshes: &[RetainedMesh], model: Mat4, alpha: f32) {
    draw_retained_meshes_ranges(meshes, &[], model, alpha);
}

fn clip_material() -> UiMaterial {
    CLIP_MATERIAL.with(|cell| {
        cell.get_or_init(|| {
            UiMaterial::new(
                CLIP_FRAGMENT,
                vec![
                    macroquad::miniquad::UniformDesc::new(
                        "rect",
                        macroquad::miniquad::UniformType::Float4,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "radius",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "inverse_model",
                        macroquad::miniquad::UniformType::Mat4,
                    ),
                ],
                Vec::new(),
            )
            .expect("clip material")
        })
        .clone()
    })
}

/// Draw a subtree clipped to a rounded rect.
///
/// rect is in the node's layout coordinates. The shader maps global v_pos back
/// through the inverse model, so rotation and scale are exact for the mask.
pub(crate) fn with_clip_radius<R>(rect: Rect, radius: f32, draw: impl FnOnce() -> R) -> R {
    if radius <= 0. || rect.w <= 0. || rect.h <= 0. {
        return draw();
    }

    let transform = current_transform();
    let inverse = transform.inverse();
    if !inverse.is_finite() || inverse.determinant().abs() < f32::EPSILON {
        return draw();
    }

    let radius = radius.min(rect.w.min(rect.h) * 0.5);
    if radius <= 0. {
        return draw();
    }

    let material = clip_material();
    let retained = retained_clip_material().map(|retained| {
        retained.set_vec4("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
        retained.set_f32("radius", radius);
        retained.set_mat4("inverse_model", model_matrix(inverse));
        push_retained(retained)
    });
    let _guard = enter_material_clip();
    let _retained_guard = retained;
    material.with(|| {
        material.set_uniform("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
        material.set_uniform("radius", radius);
        material.set_uniform("inverse_model", model_matrix(inverse));
        draw()
    })
}

/// An angular sector clip.
///
/// center is relative to the node's layout origin; start/end are radians in
/// local layout space (0 is +x, positive is +y/down). feather softens both
/// angular edges. The node transform applies to the sector shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    pub center: Vector,
    pub start: f32,
    pub end: f32,
    pub feather: f32,
}

impl Default for Sector {
    fn default() -> Self {
        Self {
            center: Vector::ZERO,
            start: 0.,
            end: 0.,
            feather: 0.,
        }
    }
}

impl Sector {
    pub fn new(center: Vector, start: f32, end: f32) -> Self {
        Self {
            center,
            start,
            end,
            feather: 0.,
        }
    }

    pub fn feather(mut self, feather: f32) -> Self {
        self.feather = feather.max(0.);
        self
    }

    /// Normalized clockwise span in [0, TAU).
    pub fn span(self) -> f32 {
        let mut span = (self.end - self.start) % TAU;
        if span < 0. {
            span += TAU;
        }
        span
    }

    /// A zero or full-circle span draws everything and is not a clip.
    pub fn is_visible(self) -> bool {
        let span = self.span();
        span > 0. && span < TAU - 1e-4
    }
}

impl PropValue for Sector {}

/// Whether point is inside the node's rounded/sector clip.
///
/// point is in the node's local layout coordinates. A visible sector takes
/// precedence over a rounded radius, matching render order.
pub(crate) fn contains_point(
    rect: Rect,
    radius: Option<f32>,
    sector: Option<Sector>,
    point: Vector,
) -> bool {
    if let Some(sector) = sector
        && sector.is_visible()
    {
        return contains_sector(rect, sector, point);
    }
    if let Some(radius) = radius
        && radius > 0.
    {
        return contains_rounded(rect, radius, point);
    }
    true
}

fn contains_rounded(rect: Rect, radius: f32, point: Vector) -> bool {
    if !rect.contains(&point) {
        return false;
    }
    let radius = radius.max(0.).min(rect.w.min(rect.h) * 0.5);
    if radius <= 0. {
        return true;
    }
    let cx = point.x.clamp(rect.x + radius, rect.right() - radius);
    let cy = point.y.clamp(rect.y + radius, rect.bottom() - radius);
    let dx = point.x - cx;
    let dy = point.y - cy;
    dx * dx + dy * dy <= radius * radius
}

fn contains_sector(rect: Rect, sector: Sector, point: Vector) -> bool {
    let center = rect.min() + sector.center;
    let angle = (point.y - center.y).atan2(point.x - center.x);
    let span = sector.span();
    let rel = (angle - sector.start).rem_euclid(TAU);
    if rel > span {
        return false;
    }
    let edge = rel.min(span - rel);
    edge > 0.
}

const SECTOR_FRAGMENT: &str = r#"#version 100
precision highp float;

varying highp vec2 v_pos;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;

uniform highp vec2 center;
uniform highp vec2 angle;
uniform highp float feather;
uniform highp mat4 inverse_model;
uniform sampler2D Texture;

const float TAU = 6.28318530718;

void main() {
    vec2 local = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float a = atan(local.y - center.y, local.x - center.x);
    float span = mod(angle.y - angle.x, TAU);
    float rel = mod(a - angle.x, TAU);
    float edge = min(rel, span - rel);
    float mask = 0.0;
    if (span > 0.0) {
        if (feather > 0.0) {
            mask = smoothstep(0.0, feather, edge);
        } else {
            mask = step(0.0, edge);
        }
    }
    vec4 tex = texture2D(Texture, v_uv);
    gl_FragColor = tex * v_color;
    gl_FragColor.a *= mask;
}
"#;

thread_local! {
    static SECTOR_MATERIAL: OnceCell<UiMaterial> = const { OnceCell::new() };
}

fn sector_material() -> UiMaterial {
    SECTOR_MATERIAL.with(|cell| {
        cell.get_or_init(|| {
            UiMaterial::new(
                SECTOR_FRAGMENT,
                vec![
                    macroquad::miniquad::UniformDesc::new(
                        "center",
                        macroquad::miniquad::UniformType::Float2,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "angle",
                        macroquad::miniquad::UniformType::Float2,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "feather",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "inverse_model",
                        macroquad::miniquad::UniformType::Mat4,
                    ),
                ],
                Vec::new(),
            )
            .expect("sector material")
        })
        .clone()
    })
}

/// Draw a subtree clipped to an angular sector.
///
/// rect is the node's layout rect; center is relative to its origin. The shader
/// maps global v_pos back through the inverse model, so rotation, scale, and
/// shear apply to the sector shape.
pub(crate) fn with_clip_sector<R>(rect: Rect, sector: Sector, draw: impl FnOnce() -> R) -> R {
    if !sector.is_visible() {
        return draw();
    }

    let transform = current_transform();
    let inverse = transform.inverse();
    if !inverse.is_finite() || inverse.determinant().abs() < f32::EPSILON {
        return draw();
    }

    let center = rect.min() + sector.center;
    let span = sector.span();
    let feather = sector.feather.clamp(0., span * 0.5);

    let material = sector_material();
    let retained = retained_sector_material().map(|retained| {
        retained.set_vec2("center", vec2(center.x, center.y));
        retained.set_vec2("angle", vec2(sector.start, sector.end));
        retained.set_f32("feather", feather);
        retained.set_mat4("inverse_model", model_matrix(inverse));
        push_retained(retained)
    });
    let _guard = enter_material_clip();
    let _retained_guard = retained;
    material.with(|| {
        material.set_uniform("center", vec2(center.x, center.y));
        material.set_uniform("angle", vec2(sector.start, sector.end));
        material.set_uniform("feather", feather);
        material.set_uniform("inverse_model", model_matrix(inverse));
        draw()
    })
}
