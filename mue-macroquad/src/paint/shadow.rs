//! Drop shadows for immediate UI nodes.
//!
//! A shadow is drawn before the node background by use_background. It uses the
//! node's global rect and the shared shadow material; the shader is a rounded
//! box shadow adapted from Phira's prpr shadow.

mod mask;

pub(crate) use mask::ShadowMaskState;
pub use mask::{ShadowMaskCacheStats, clear_shadow_mask_cache, shadow_mask_cache_stats};

use macroquad::math::vec4;
use mue_core::prop::PropValue;

use crate::{
    math::{Rect, model_matrix},
    paint::color::Color,
    paint::material::UiMaterial,
    paint::{Paint, current_transform},
};

/// Drop-shadow parameters attached to a style.
///
/// elevation is the blur radius in local pixels; radius is the corner radius
/// of the shadowed box. The default has elevation 0 and draws nothing. The
/// shadow follows the node transform via an inverse model, so rotation and
/// scale apply to the shadow shape. Shadows are drawn by use_background;
/// custom nodes that skip it do not get shadows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub elevation: f32,
    pub radius: f32,
    pub color: Color,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            elevation: 0.,
            radius: 0.,
            color: Color::TRANSPARENT,
        }
    }
}

impl Shadow {
    /// A black shadow with the given blur and corner radius.
    pub fn new(elevation: f32, radius: f32) -> Self {
        Self {
            elevation,
            radius,
            color: Color::from_rgba(0, 0, 0, 128),
        }
    }

    /// Override the shadow color, including its alpha.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Whether this shadow contributes any pixels.
    pub fn is_visible(&self) -> bool {
        self.elevation > 0. && self.color.a > 0.
    }
}

impl PropValue for Shadow {}

/// Admission policy for the shared, bounded native-OpenGL shadow-mask cache.
/// All policies preserve the analytic fallback and the same quality/resource limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadowCache {
    /// Reuse exact resident masks; bake only stable, repeatedly drawn large shadows.
    #[default]
    Auto,
    /// Allow stable shadows to bake without the Auto profitability heuristic.
    Enabled,
    /// Always use the analytic shader and do not participate in the mask pool.
    Disabled,
}
impl PropValue for ShadowCache {}
impl From<bool> for ShadowCache {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

const SHADOW_FRAGMENT_PREFIX: &str = concat!(
    r#"#version 100
precision highp float;

varying highp vec2 v_pos;
uniform highp vec4 rect;
uniform highp float elevation;
uniform highp float radius;
uniform highp vec4 color;
uniform highp mat4 inverse_model;
"#,
    include_str!("shadow/functions.glsl"),
    "\n",
);

const SHADOW_FRAGMENT_SUFFIX: &str = r#"
void main() {
    float clip = combined_clip_mask(v_pos);
    if (clip == 0.0) {
        gl_FragColor = vec4(color.rgb, 0.0);
        return;
    }
    vec2 local = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float alpha = roundedBoxShadow(rect.xy, rect.zw, local, elevation, radius);
    gl_FragColor = vec4(color.rgb, alpha * color.a * clip);
}
"#;

thread_local! {
    static SHADOW_MATERIALS: std::cell::RefCell<Vec<Option<UiMaterial>>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn shadow_material(depth: usize) -> UiMaterial {
    SHADOW_MATERIALS.with_borrow_mut(|materials| {
        if materials.len() <= depth {
            materials.resize_with(depth + 1, || None);
        }
        materials[depth]
            .get_or_insert_with(|| {
                let uniforms = [
                    macroquad::miniquad::UniformDesc::new(
                        "rect",
                        macroquad::miniquad::UniformType::Float4,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "elevation",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "radius",
                        macroquad::miniquad::UniformType::Float1,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "color",
                        macroquad::miniquad::UniformType::Float4,
                    ),
                    macroquad::miniquad::UniformDesc::new(
                        "inverse_model",
                        macroquad::miniquad::UniformType::Mat4,
                    ),
                ];
                let mut uniforms = uniforms.to_vec();
                uniforms.extend(crate::paint::clip::clip_uniforms(depth));
                let mut fragment = String::from(SHADOW_FRAGMENT_PREFIX);
                fragment.push_str(&crate::paint::clip::clip_shader_helpers(depth));
                fragment.push_str(SHADOW_FRAGMENT_SUFFIX);
                UiMaterial::new(&fragment, uniforms, Vec::new()).expect("shadow material")
            })
            .clone()
    })
}

/// Draw a shadow behind rect. Called by use_background before the background.
pub(crate) fn draw_shadow(
    paint: &Paint,
    rect: Rect,
    shadow: Shadow,
    cache: ShadowCache,
    mask: &mut ShadowMaskState,
) {
    if !shadow.is_visible() || rect.w <= 0. || rect.h <= 0. {
        mask.reset();
        return;
    }

    let transform = current_transform();
    let inverse = transform.inverse();
    if !inverse.is_finite() || inverse.determinant().abs() < f32::EPSILON {
        mask.reset();
        return;
    }

    let alpha = shadow.color.a * paint.opacity();
    if !alpha.is_finite() || alpha <= 0. {
        mask.reset();
        return;
    }

    let elevation = shadow.elevation;
    let radius = shadow.radius.min(rect.w.min(rect.h) * 0.5);
    if !elevation.is_finite() || !radius.is_finite() || elevation <= 0. || radius < 0. {
        mask.reset();
        return;
    }

    let clip_depth = crate::paint::clip::active_clip_depth();
    if cache == ShadowCache::Disabled {
        mask.reset();
    } else if mask.draw(paint, rect, shadow, transform, inverse, cache) {
        return;
    }

    let reach = elevation * 3.;
    let expanded = Rect::new(
        rect.x - reach,
        rect.y - reach,
        rect.w + 2. * reach,
        rect.h + 2. * reach,
    );

    let material = shadow_material(clip_depth);
    material.with(|| {
        crate::paint::clip::apply_active_clip_uniforms(&material, None);
        material.set_uniform("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
        material.set_uniform("elevation", elevation);
        material.set_uniform("radius", radius);
        material.set_uniform(
            "color",
            vec4(shadow.color.r, shadow.color.g, shadow.color.b, alpha),
        );
        material.set_uniform("inverse_model", model_matrix(inverse));
        paint.draw_solid_rect(expanded, Color::WHITE);
    });
}
