//! Drop shadows for immediate UI nodes.
//!
//! A shadow is drawn before the node background by use_background. It uses the
//! node's global rect and the shared shadow material; the shader is a rounded
//! box shadow adapted from Phira's prpr shadow.

use std::cell::OnceCell;

use macroquad::math::vec4;
use mue_core::prop::PropValue;

use crate::{
    color::Color,
    material::UiMaterial,
    math::{Rect, model_matrix},
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

const SHADOW_FRAGMENT: &str = r#"#version 100
precision highp float;

varying highp vec2 v_pos;

uniform highp vec4 rect;
uniform highp float elevation;
uniform highp float radius;
uniform highp vec4 color;
uniform highp mat4 inverse_model;

float gaussian(float x, float sigma) {
    const float pi = 3.141592653589793;
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * pi) * sigma);
}

vec2 erf(vec2 x) {
    vec2 s = sign(x), a = abs(x);
    x = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    x *= x;
    return s - s / (x * x);
}

float roundedBoxShadowX(float x, float y, float sigma, float corner, vec2 halfSize) {
    float delta = min(halfSize.y - corner - abs(y), 0.0);
    float curved = halfSize.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    vec2 integral = 0.5 + 0.5 * erf((x + vec2(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

float roundedBoxShadow(vec2 lower, vec2 upper, vec2 point, float sigma, float corner) {
    vec2 lowerp = lower + vec2(corner);
    vec2 upperp = upper - vec2(corner);
    float lf = step(point.x, lowerp.x);
    float tp = step(point.y, lowerp.y);
    float rt = step(upperp.x, point.x);
    float bt = step(upperp.y, point.y);
    float eps = 0.0003;
    float ein = 0.0007;
    float factor = 1.0 -
        (1.0 - step(corner - eps, distance(lowerp, point)) * lf * tp)
      * (1.0 - step(corner - eps, distance(upperp, point)) * rt * bt)
      * (1.0 - step(corner - eps, distance(vec2(lowerp.x, upperp.y), point)) * lf * bt)
      * (1.0 - step(corner - eps, distance(vec2(upperp.x, lowerp.y), point)) * rt * tp)
      * smoothstep(lower.x, lower.x + ein, point.x)
      * smoothstep(lower.y, lower.y + ein, point.y)
      * smoothstep(point.x, point.x + ein, upper.x)
      * smoothstep(point.y, point.y + ein, upper.y);

    point.y -= sigma * 0.5;
    vec2 center = (lower + upper) * 0.5;
    vec2 halfSize = (upper - lower) * 0.5;
    point -= center;

    float low = point.y - halfSize.y;
    float high = point.y + halfSize.y;
    float start = clamp(-3.0 * sigma, low, high);
    float end = clamp(3.0 * sigma, low, high);

    float s = (end - start) / 4.0;
    float y = start + s * 0.5;
    float value = 0.0;
    for (int i = 0; i < 4; i++) {
        value += roundedBoxShadowX(point.x, point.y - y, sigma, corner, halfSize) * gaussian(y, sigma) * s;
        y += s;
    }

    return value * factor;
}

void main() {
    vec2 local = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float alpha = roundedBoxShadow(rect.xy, rect.zw, local, elevation, radius);
    gl_FragColor = vec4(color.rgb, alpha * color.a);
}
"#;

thread_local! {
    static SHADOW_MATERIAL: OnceCell<UiMaterial> = const { OnceCell::new() };
}

fn shadow_material() -> UiMaterial {
    SHADOW_MATERIAL.with(|cell| {
        cell.get_or_init(|| {
            UiMaterial::new(
                SHADOW_FRAGMENT,
                vec![
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
                ],
                Vec::new(),
            )
            .expect("shadow material")
        })
        .clone()
    })
}

/// Draw a shadow behind rect. Called by use_background before the background.
pub(crate) fn draw_shadow(paint: &Paint, rect: Rect, shadow: Shadow) {
    if !shadow.is_visible() || rect.w <= 0. || rect.h <= 0. {
        return;
    }

    let transform = current_transform();
    let inverse = transform.inverse();
    if !inverse.is_finite() || inverse.determinant().abs() < f32::EPSILON {
        return;
    }

    let alpha = shadow.color.a * paint.opacity();
    if alpha <= 0. {
        return;
    }

    let elevation = shadow.elevation;
    let radius = shadow.radius.min(rect.w.min(rect.h) * 0.5);
    if elevation <= 0. || radius < 0. {
        return;
    }

    let reach = elevation * 3.;
    let expanded = Rect::new(
        rect.x - reach,
        rect.y - reach,
        rect.w + 2. * reach,
        rect.h + 2. * reach,
    );

    let material = shadow_material();
    material.with(|| {
        material.set_uniform("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
        material.set_uniform("elevation", elevation);
        material.set_uniform("radius", radius);
        material.set_uniform(
            "color",
            vec4(shadow.color.r, shadow.color.g, shadow.color.b, alpha),
        );
        material.set_uniform("inverse_model", model_matrix(inverse));
        paint.draw(|p| p.fill_rect(expanded, Color::WHITE));
    });
}
