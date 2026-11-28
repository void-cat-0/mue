//! Box shadows, as in CSS `box-shadow`.
//!
//! A node draws its shadows before its background (see `use_background`).
//! Each [`BoxShadow`] is the node's border box moved by an offset, grown by a
//! spread, and blurred; it follows the node's corner radius and transform.
//! Like CSS outer shadows, a shadow never shows through the node itself, so
//! translucent panels do not darken. The first shadow in a list draws on top.
//!
//! ```
//! use mue_macroquad::{
//!     node::flexbox,
//!     paint::{color::Color, shadow::BoxShadow},
//!     style::Styleable,
//! };
//!
//! let _card = flexbox().box_shadow([
//!     BoxShadow::new(0., 4., 12., Color::from_rgba(0, 0, 0, 80)),
//!     BoxShadow::new(0., 1., 2., Color::from_rgba(0, 0, 0, 60)),
//! ]);
//! ```
//!
//! In `style!`, the same reads
//! `box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3), 0 1px 2px rgba(0, 0, 0, 0.25);`.

mod mask;

pub(crate) use mask::ShadowMaskState;
pub use mask::{ShadowMaskCacheStats, clear_shadow_mask_cache, shadow_mask_cache_stats};

use std::rc::Rc;

use macroquad::math::vec4;
use mue_core::prop::{Prop, PropValue};

use crate::{
    math::{CornerRadii, Rect, Vector, model_matrix},
    paint::color::Color,
    paint::material::UiMaterial,
    paint::{Paint, current_transform},
};

/// One shadow: an offset, a blur radius, a spread distance, and a color.
///
/// Lengths are logical pixels, as in CSS: `blur` is the blur radius (twice
/// the Gaussian standard deviation) and `spread` grows (or, when negative,
/// shrinks) the shadow box and its corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub offset: Vector,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl BoxShadow {
    /// A shadow offset by `(x, y)` with the given blur radius.
    pub const fn new(x: f32, y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset: Vector::new(x, y),
            blur,
            spread: 0.,
            color,
        }
    }

    /// Grow the shadow box by `spread` on every side (negative shrinks it).
    pub const fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    fn is_visible(&self) -> bool {
        self.color.a > 0.
            && [self.offset.x, self.offset.y, self.blur, self.spread]
                .into_iter()
                .all(f32::is_finite)
            && self.blur >= 0.
    }
}

/// A list of shadows; the first draws on top. The default is no shadow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxShadows(Rc<[BoxShadow]>);

impl BoxShadows {
    /// No shadow.
    pub fn none() -> Self {
        Self::default()
    }

    /// A soft two-layer shadow that grows with `level` (about 1 to 24), for
    /// raised surfaces such as cards, menus, and dialogs.
    pub fn elevation(level: f32) -> Self {
        let level = level.max(0.);
        if level == 0. {
            return Self::none();
        }
        Self::from([
            BoxShadow::new(0., level * 0.5, level * 1.5, Color::from_rgba(0, 0, 0, 72)),
            BoxShadow::new(0., level * 0.15, level * 0.4, Color::from_rgba(0, 0, 0, 48)),
        ])
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, BoxShadow> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl PropValue for BoxShadows {}

impl From<BoxShadow> for BoxShadows {
    fn from(shadow: BoxShadow) -> Self {
        Self(Rc::from([shadow]))
    }
}

impl<const N: usize> From<[BoxShadow; N]> for BoxShadows {
    fn from(shadows: [BoxShadow; N]) -> Self {
        Self(Rc::from(shadows))
    }
}

impl From<Vec<BoxShadow>> for BoxShadows {
    fn from(shadows: Vec<BoxShadow>) -> Self {
        Self(Rc::from(shadows))
    }
}

/// Values accepted by `box_shadow`: one shadow, an array or `Vec` of
/// shadows, a [`BoxShadows`] list, or a reactive list.
pub trait IntoBoxShadows {
    fn into_box_shadows(self) -> Prop<BoxShadows>;
}

impl IntoBoxShadows for BoxShadow {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        Prop::Static(self.into())
    }
}

impl<const N: usize> IntoBoxShadows for [BoxShadow; N] {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        Prop::Static(self.into())
    }
}

impl IntoBoxShadows for Vec<BoxShadow> {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        Prop::Static(self.into())
    }
}

impl IntoBoxShadows for BoxShadows {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        Prop::Static(self)
    }
}

impl IntoBoxShadows for Prop<BoxShadows> {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        self
    }
}

impl IntoBoxShadows for mue_core::signal::ReadSignal<BoxShadows> {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        self.into()
    }
}

impl IntoBoxShadows for mue_core::signal::Signal<BoxShadows> {
    fn into_box_shadows(self) -> Prop<BoxShadows> {
        self.into()
    }
}

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

/// One shadow resolved against its node: the blurred shadow box and the node
/// box it is cut out of, both in local coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShadowShape {
    pub rect: Rect,
    pub radius: f32,
    /// Gaussian standard deviation (half the CSS blur radius).
    pub sigma: f32,
    pub color: Color,
    pub box_rect: Rect,
    pub box_radius: f32,
}

impl ShadowShape {
    fn resolve(shadow: &BoxShadow, rect: Rect, radii: CornerRadii) -> Option<Self> {
        if !shadow.is_visible() || rect.w <= 0. || rect.h <= 0. {
            return None;
        }
        // The analytic shader models one corner radius; per-corner radii use
        // their average.
        let box_radius =
            ((radii.top_left + radii.top_right + radii.bottom_right + radii.bottom_left) * 0.25)
                .min(rect.w.min(rect.h) * 0.5);
        let spread = shadow.spread;
        let moved = Rect::new(
            rect.x + shadow.offset.x - spread,
            rect.y + shadow.offset.y - spread,
            rect.w + 2. * spread,
            rect.h + 2. * spread,
        );
        if moved.w <= 0. || moved.h <= 0. {
            return None;
        }
        let radius = if box_radius > 0. {
            (box_radius + spread).max(0.)
        } else {
            0.
        }
        .min(moved.w.min(moved.h) * 0.5);
        Some(Self {
            rect: moved,
            radius,
            // A zero blur still needs a tiny kernel for the analytic integral.
            sigma: (shadow.blur * 0.5).max(0.35),
            color: shadow.color,
            box_rect: rect,
            box_radius,
        })
    }
}

const SHADOW_FRAGMENT_PREFIX: &str = concat!(
    r#"#version 100
precision highp float;

varying highp vec2 v_pos;
uniform highp vec4 rect;
uniform highp vec4 box;
uniform highp float sigma;
uniform highp float radius;
uniform highp float box_radius;
uniform highp vec4 color;
uniform highp mat4 inverse_model;
"#,
    include_str!("shadow/functions.glsl"),
    "\n",
);

const SHADOW_FRAGMENT_SUFFIX: &str = r#"
void main() {
    float clip = combined_clip_mask(v_pos);
    vec2 local = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float cut = roundedBoxCutout(box.xy, box.zw, local, box_radius);
    if (clip == 0.0 || cut == 0.0) {
        gl_FragColor = vec4(color.rgb, 0.0);
        return;
    }
    float alpha = roundedBoxShadowIntegral(rect.xy, rect.zw, local, sigma, radius) * cut;
    gl_FragColor = vec4(color.rgb, alpha * color.a * clip);
}
"#;

thread_local! {
    static SHADOW_MATERIALS: std::cell::RefCell<Vec<Option<UiMaterial>>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn shadow_material(depth: usize) -> UiMaterial {
    use macroquad::miniquad::{UniformDesc, UniformType};
    SHADOW_MATERIALS.with_borrow_mut(|materials| {
        if materials.len() <= depth {
            materials.resize_with(depth + 1, || None);
        }
        materials[depth]
            .get_or_insert_with(|| {
                let mut uniforms = vec![
                    UniformDesc::new("rect", UniformType::Float4),
                    UniformDesc::new("box", UniformType::Float4),
                    UniformDesc::new("sigma", UniformType::Float1),
                    UniformDesc::new("radius", UniformType::Float1),
                    UniformDesc::new("box_radius", UniformType::Float1),
                    UniformDesc::new("color", UniformType::Float4),
                    UniformDesc::new("inverse_model", UniformType::Mat4),
                ];
                uniforms.extend(crate::paint::clip::clip_uniforms(depth));
                let mut fragment = String::from(SHADOW_FRAGMENT_PREFIX);
                fragment.push_str(&crate::paint::clip::clip_shader_helpers(depth));
                fragment.push_str(SHADOW_FRAGMENT_SUFFIX);
                UiMaterial::new(&fragment, uniforms, Vec::new()).expect("shadow material")
            })
            .clone()
    })
}

/// Draw a node's shadows behind `rect`, last shadow first. Called by
/// `use_background` before the background. `masks` keeps one cache state per
/// shadow.
pub(crate) fn draw_box_shadows(
    paint: &Paint,
    rect: Rect,
    radii: CornerRadii,
    shadows: &BoxShadows,
    cache: ShadowCache,
    masks: &mut Vec<ShadowMaskState>,
    opaque: bool,
) {
    masks.resize_with(shadows.len(), ShadowMaskState::default);
    for (shadow, mask) in shadows.iter().zip(masks.iter_mut()).rev() {
        match ShadowShape::resolve(shadow, rect, radii) {
            // An opaque node hides the shadow's interior, so a small blur can
            // be drawn as feathered geometry: it batches with the rest of the
            // frame instead of costing a shader pass of its own. Larger
            // blurs and translucent nodes keep the exact analytic shader.
            Some(shape) if opaque && shadow.blur <= FAST_BLUR => {
                mask.reset();
                paint.draw(|p| {
                    p.fill_feathered_rounded_rect(shape.rect, shape.radius, shadow.blur, shape.color)
                });
            }
            Some(shape) => draw_shape(paint, &shape, cache, mask),
            None => mask.reset(),
        }
    }
}

/// Blur radius (logical pixels) up to which shadows under opaque nodes use
/// feathered geometry instead of the analytic shader.
const FAST_BLUR: f32 = 8.;

fn draw_shape(paint: &Paint, shape: &ShadowShape, cache: ShadowCache, mask: &mut ShadowMaskState) {
    let transform = current_transform();
    let inverse = transform.inverse();
    if !inverse.is_finite() || inverse.determinant().abs() < f32::EPSILON {
        mask.reset();
        return;
    }
    let alpha = shape.color.a * paint.opacity();
    if !alpha.is_finite() || alpha <= 0. {
        mask.reset();
        return;
    }
    let clip_depth = crate::paint::clip::active_clip_depth();
    if cache == ShadowCache::Disabled {
        mask.reset();
    } else if mask.draw(paint, shape, transform, inverse, cache) {
        return;
    }

    let rect = shape.rect;
    let reach = shape.sigma * 3.;
    let expanded = Rect::new(
        rect.x - reach,
        rect.y - reach,
        rect.w + 2. * reach,
        rect.h + 2. * reach,
    );
    let b = shape.box_rect;
    let material = shadow_material(clip_depth);
    material.with(|| {
        crate::paint::clip::apply_active_clip_uniforms(&material, None);
        material.set_uniform("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
        material.set_uniform("box", vec4(b.x, b.y, b.right(), b.bottom()));
        material.set_uniform("sigma", shape.sigma);
        material.set_uniform("radius", shape.radius);
        material.set_uniform("box_radius", shape.box_radius);
        material.set_uniform(
            "color",
            vec4(shape.color.r, shape.color.g, shape.color.b, alpha),
        );
        material.set_uniform("inverse_model", model_matrix(inverse));
        paint.draw_solid_rect(expanded, Color::WHITE);
    });
}
