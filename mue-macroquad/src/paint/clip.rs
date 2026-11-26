//! Rounded and sector clipping for immediate UI subtrees.
//!
//! A clip is a material scope: it replaces the current pipeline with an alpha
//! mask while the subtree draws. Retained geometry is used only while the current
//! material has a matching retained shader; otherwise nodes draw immediate geometry.

use std::cell::RefCell;

use macroquad::{math::vec4, retained::RetainedMaterial};
use mue_core::prop::PropValue;

use crate::{
    math::{Rect, Vector, model_matrix},
    paint::current_transform,
    paint::material::UiMaterial,
};

const TAU: f32 = std::f32::consts::TAU;

const CLIP_FRAGMENT_PREFIX: &str = r#"#version 100
precision highp float;

varying highp vec2 v_pos;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;
uniform sampler2D Texture;
"#;

const CLIP_FRAGMENT_SUFFIX: &str = r#"
void main() {
    vec4 tex = texture2D(Texture, v_uv);
    gl_FragColor = tex * v_color;
    gl_FragColor.a *= combined_clip_mask(v_pos);
}
"#;

#[derive(Clone, Copy)]
struct ClipMask {
    data: [f32; 4],
    shape: [f32; 4],
    inverse: macroquad::math::Mat4,
}

#[derive(Clone)]
struct ClipMaterials {
    immediate: UiMaterial,
    retained: Option<RetainedMaterial>,
}

thread_local! {
    static CLIP_STACK: RefCell<Vec<ClipMask>> = const { RefCell::new(Vec::new()) };
    static CLIP_MATERIALS: RefCell<Vec<Option<ClipMaterials>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn active_clip_depth() -> usize {
    CLIP_STACK.with_borrow(|stack| stack.len())
}

pub(crate) fn clip_uniforms(depth: usize) -> Vec<macroquad::miniquad::UniformDesc> {
    let mut uniforms = Vec::with_capacity(depth * 3);
    for index in 0..depth {
        uniforms.push(macroquad::miniquad::UniformDesc::new(
            &format!("clip_shape_{index}"),
            macroquad::miniquad::UniformType::Float4,
        ));
        uniforms.push(macroquad::miniquad::UniformDesc::new(
            &format!("clip_data_{index}"),
            macroquad::miniquad::UniformType::Float4,
        ));
        uniforms.push(macroquad::miniquad::UniformDesc::new(
            &format!("clip_inverse_{index}"),
            macroquad::miniquad::UniformType::Mat4,
        ));
    }
    uniforms
}

pub(crate) fn clip_shader_helpers(depth: usize) -> String {
    let mut source = String::from("const float CLIP_TAU = 6.28318530718;\n");
    for index in 0..depth {
        source.push_str(&format!(
            "uniform highp vec4 clip_shape_{index};\nuniform highp vec4 clip_data_{index};\nuniform highp mat4 clip_inverse_{index};\n"
        ));
    }
    source.push_str("float combined_clip_mask(vec2 global_pos) {\n    float combined = 1.0;\n");
    for index in 0..depth {
        source.push_str(&format!(
            "    vec2 local_{index} = (clip_inverse_{index} * vec4(global_pos, 0.0, 1.0)).xy;\n    float mask_{index};\n    if (clip_shape_{index}.x < 0.5) {{\n        vec2 half_size = (clip_data_{index}.zw - clip_data_{index}.xy) * 0.5;\n        vec2 center = clip_data_{index}.xy + half_size;\n        vec2 q = abs(local_{index} - center) - half_size + vec2(clip_shape_{index}.y);\n        float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - clip_shape_{index}.y;\n        mask_{index} = 1.0 - smoothstep(-0.5, 0.5, d);\n    }} else {{\n        float a = atan(local_{index}.y - clip_data_{index}.y, local_{index}.x - clip_data_{index}.x);\n        float span = mod(clip_shape_{index}.z - clip_shape_{index}.y, CLIP_TAU);\n        float rel = mod(a - clip_shape_{index}.y, CLIP_TAU);\n        float edge = min(rel, span - rel);\n        mask_{index} = clip_shape_{index}.w > 0.0 ? smoothstep(0.0, clip_shape_{index}.w, edge) : step(0.0, edge);\n    }}\n    combined *= mask_{index};\n"
        ));
    }
    source.push_str("    return combined;\n}\n");
    source
}

pub(crate) fn apply_active_clip_uniforms(
    immediate: &UiMaterial,
    retained: Option<&RetainedMaterial>,
) {
    CLIP_STACK.with_borrow(|stack| {
        for (index, clip) in stack.iter().enumerate() {
            let shape_name = format!("clip_shape_{index}");
            let data_name = format!("clip_data_{index}");
            let inverse_name = format!("clip_inverse_{index}");
            let shape = vec4(clip.shape[0], clip.shape[1], clip.shape[2], clip.shape[3]);
            let data = vec4(clip.data[0], clip.data[1], clip.data[2], clip.data[3]);
            immediate.set_uniform(&shape_name, shape);
            immediate.set_uniform(&data_name, data);
            immediate.set_uniform(&inverse_name, clip.inverse);
            if let Some(retained) = retained {
                retained.set_vec4(&shape_name, shape);
                retained.set_vec4(&data_name, data);
                retained.set_mat4(&inverse_name, clip.inverse);
            }
        }
    });
}

fn clip_material(depth: usize) -> ClipMaterials {
    CLIP_MATERIALS.with_borrow_mut(|materials| {
        if materials.len() < depth {
            materials.resize_with(depth, || None);
        }
        materials[depth - 1]
            .get_or_insert_with(|| {
                let uniforms = clip_uniforms(depth);
                let mut fragment = String::from(CLIP_FRAGMENT_PREFIX);
                fragment.push_str(&clip_shader_helpers(depth));
                fragment.push_str(CLIP_FRAGMENT_SUFFIX);
                let retained = RetainedMaterial::new(&fragment, &uniforms, &[]);
                let immediate =
                    UiMaterial::new(&fragment, uniforms, Vec::new()).expect("clip material");
                ClipMaterials {
                    immediate,
                    retained,
                }
            })
            .clone()
    })
}

struct ClipStackGuard;

impl Drop for ClipStackGuard {
    fn drop(&mut self) {
        CLIP_STACK.with_borrow_mut(|stack| {
            stack.pop();
        });
    }
}

fn with_clip_mask<R>(clip: ClipMask, draw: impl FnOnce() -> R) -> R {
    CLIP_STACK.with_borrow_mut(|stack| stack.push(clip));
    let _stack_guard = ClipStackGuard;
    let depth = active_clip_depth();
    let materials = clip_material(depth);
    let retained = materials.retained.clone();
    apply_active_clip_uniforms(&materials.immediate, retained.as_ref());
    materials.immediate.with_retained(retained, draw)
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

    with_clip_mask(
        ClipMask {
            data: [rect.x, rect.y, rect.right(), rect.bottom()],
            shape: [0., radius, 0., 0.],
            inverse: model_matrix(inverse),
        },
        draw,
    )
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

    with_clip_mask(
        ClipMask {
            data: [center.x, center.y, 0., 0.],
            shape: [1., sector.start, sector.end, feather],
            inverse: model_matrix(inverse),
        },
        draw,
    )
}
