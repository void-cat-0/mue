//! Policy-controlled shadow masks. OpenGL reuses individual textures. Vulkan
//! keeps fixed backing pages so scroll/cache churn never resizes GPU storage.

use std::{
    cell::{Cell, OnceCell, RefCell},
    time::{Duration, Instant},
};

use macroquad::{
    math::vec4,
    miniquad::{
        self as mq, Backend, BufferId, BufferLayout, BufferSource, BufferType, BufferUsage,
        PassAction, Pipeline, PipelineParams, RenderPass, RenderingBackend, ShaderId, ShaderMeta,
        ShaderSource, TextureId, UniformBlockLayout, UniformDesc, UniformType,
        UniformsSource, VertexAttribute, VertexFormat,
    },
    texture::Texture2D,
    window::get_internal_gl,
};

use super::{Shadow, ShadowCache, ShadowProjection};

mod vulkan_atlas;
use crate::{
    material::UiMaterial,
    math::{Matrix, Rect, model_matrix},
    paint::Paint,
};
use vulkan_atlas::VulkanAtlas;

const MAX_SLOTS: usize = 8;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_MASK_BYTES: usize = 4 * 1024 * 1024;
const MAX_SIDE: u32 = 2048;
const STABLE_FOR: Duration = Duration::from_millis(100);
const BAKE_INTERVAL: Duration = Duration::from_millis(16);
// Auto is intentionally a cheap conservative heuristic, not GPU profiling.
const MAX_PENDING_KEYS: usize = 16;
const DEMAND_WINDOW: Duration = Duration::from_millis(500);
const MIN_HALO_PIXELS: f64 = 8192.;
const MIN_OBSERVATIONS: u32 = 8;
const SHARED_CONTRIBUTORS: usize = 4;
// Recent visible halo work must plausibly amortize a full 2x-density bake.
const REUSE_WORK_FACTOR: f64 = 1.;

const BAKE_VERTEX: &str = r#"#version 100
attribute vec2 position;
varying highp vec2 v_pos;
uniform highp vec4 bounds;
void main() {
    gl_Position = vec4(position.x * 2.0 - 1.0, 1.0 - position.y * 2.0, 0.0, 1.0);
    v_pos = bounds.xy + position * bounds.zw;
}
"#;
const BAKE_FRAGMENT: &str = concat!(
    r#"#version 100
precision highp float;
varying highp vec2 v_pos;
uniform highp vec4 rect;
uniform highp float elevation;
uniform highp float radius;
"#,
    include_str!("functions.glsl"),
    r#"
void main() {
    float alpha = roundedBoxShadowIntegral(rect.xy, rect.zw, v_pos, elevation, radius);
    gl_FragColor = vec4(1.0, 1.0, 1.0, alpha);
}
"#
);

// Keep the sharp interior cutout analytic, so bilinear filtering cannot leak
// a cached shadow into a transparent panel. Only the expensive soft integral
// is sampled. These equations match roundedBoxShadow before its y shift.
const SAMPLE_FRAGMENT_PREFIX: &str = concat!(
    r#"precision highp float;
varying highp vec2 v_pos;
uniform highp vec4 rect;
uniform highp vec4 mask_bounds;
uniform highp float radius;
uniform highp vec4 color;
uniform highp mat4 inverse_model;
uniform sampler2D Texture;
"#,
    include_str!("functions.glsl"),
    "\n",
);

const GL_SAMPLE_FRAGMENT: &str = r#"
void main() {
    vec2 point = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float factor = roundedBoxCutout(rect.xy, rect.zw, point, radius);
    vec2 uv = (point - rect.xy - mask_bounds.xy) / mask_bounds.zw;
    uv.y = 1.0 - uv.y;
    float alpha = texture2D(Texture, uv).a * factor;
    gl_FragColor = vec4(color.rgb, alpha * color.a * combined_clip_mask(v_pos));
}
"#;

const ATLAS_SAMPLE_FRAGMENT: &str = r#"
uniform highp vec4 atlas_region;
uniform highp vec2 atlas_texel;
void main() {
    float clip = combined_clip_mask(v_pos);
    vec2 point = (inverse_model * vec4(v_pos, 0.0, 1.0)).xy;
    float factor = roundedBoxCutout(rect.xy, rect.zw, point, radius);
    if (factor == 0.0 || clip == 0.0) {
        gl_FragColor = vec4(color.rgb, 0.0);
        return;
    }
    vec2 uv = (point - rect.xy - mask_bounds.xy) / mask_bounds.zw;
    uv.y = 1.0 - uv.y;
    // Reproduce Clamp/Linear on an individual texture. Never sample a neighboring mask.
    vec2 half_texel = atlas_texel * 0.5;
    uv = atlas_region.xy + clamp(uv * atlas_region.zw, half_texel, atlas_region.zw - half_texel);
    // Some invocations returned above; no implicit derivatives in this branch.
    float alpha = texture2DLodEXT(Texture, uv, 0.0).a * factor;
    gl_FragColor = vec4(color.rgb, alpha * color.a * clip);
}
"#;

/// Cumulative diagnostics for the render thread's bounded mask pool.
/// Bytes count RGBA8 backing storage, including unused OpenGL slots and free
/// space in Vulkan pages. They do not include driver overhead.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShadowMaskCacheStats {
    pub slots: usize,
    pub resident_masks: usize,
    pub allocated_bytes: usize,
    pub hits: u64,
    pub bakes: u64,
    pub evictions: u64,
    pub fallbacks: u64,
    /// Shadows with no projected coverage in the target/viewport/scissor (no draw).
    pub culled: u64,
    /// Auto misses rejected by the coverage/repeated-work heuristic.
    pub auto_rejections: u64,
    /// Bounded recent Auto candidates, not allocated GPU textures.
    pub pending_keys: usize,
}

#[derive(Clone, Copy, PartialEq)]
struct Key {
    width: f32,
    height: f32,
    elevation: f32,
    radius: f32,
    pixels: (u32, u32),
}
impl Key {
    fn bytes(self) -> usize {
        self.pixels.0 as usize * self.pixels.1 as usize * 4
    }
    fn bounds(self) -> Rect {
        let reach = self.elevation * 3.;
        let w = self.width + 2. * reach;
        let h = self.height + 2. * reach;
        let pad_x = w / (self.pixels.0 - 2) as f32;
        let pad_y = h / (self.pixels.1 - 2) as f32;
        Rect::new(
            -reach - pad_x,
            -reach - pad_y,
            w + 2. * pad_x,
            h + 2. * pad_y,
        )
    }
}

pub(crate) struct ShadowMaskState {
    id: u64,
    tracking: Option<Tracking>,
}
struct Tracking {
    key: Key,
    since: Instant,
    seen: Instant,
    resident: bool,
}
thread_local! {
    static NEXT_STATE: Cell<u64> = const { Cell::new(0) };
}
impl Default for ShadowMaskState {
    fn default() -> Self {
        let id = NEXT_STATE.with(|next| {
            let id = next
                .get()
                .checked_add(1)
                .expect("shadow state identifiers exhausted");
            next.set(id);
            id
        });
        Self { id, tracking: None }
    }
}
impl ShadowMaskState {
    pub(crate) fn reset(&mut self) {
        self.tracking = None;
    }

    pub(crate) fn cull(&mut self) {
        self.reset();
        POOL.with_borrow_mut(|pool| pool.stats.culled += 1);
    }

    pub(super) fn draw(
        &mut self,
        paint: &Paint,
        rect: Rect,
        expanded: Rect,
        shadow: Shadow,
        transform: Matrix,
        inverse: Matrix,
        policy: ShadowCache,
        projection: Option<ShadowProjection>,
    ) -> bool {
        let Some((backend, candidate)) = projection.and_then(|projection| {
            Some((
                MaskBackend::current()?,
                candidate_for(rect, expanded, shadow, transform, projection)?,
            ))
        }) else {
            self.reset();
            POOL.with_borrow_mut(|pool| pool.stats.fallbacks += 1);
            return false;
        };
        // GL retains its existing area/knockout culling. Vulkan draws unless
        // the independent conservative reject proved it wholly outside;
        // polygon area is only an admission heuristic, never a Vulkan cull.
        let gl_area = (backend == MaskBackend::Gl).then(|| {
            if clip_inside_box(
                candidate.projection,
                candidate.viewport,
                candidate.clip,
                rect,
                candidate.key.radius,
            ) {
                0.
            } else {
                candidate.visible_pixels(expanded)
            }
        });
        if gl_area.is_some_and(|area| area <= 0.) {
            self.cull();
            return true;
        }
        let key = candidate.key;
        let now = Instant::now();
        if self
            .tracking
            .as_ref()
            .is_none_or(|state| state.key != key || now.duration_since(state.seen) > DEMAND_WINDOW)
        {
            self.tracking = Some(Tracking {
                key,
                since: now,
                seen: now,
                resident: false,
            });
        }
        let state = self.tracking.as_mut().unwrap();
        state.seen = now;
        POOL.with_borrow_mut(|pool| {
            // Matching pixels are already correct; only new bakes need stability
            // or profitability. This also lets new scroll rows reuse immediately.
            let sample = if let Some(sample) = pool.find(backend, key, now) {
                sample
            } else {
                let admitted = policy == ShadowCache::Enabled || {
                    let visible = gl_area.unwrap_or_else(|| candidate.visible_pixels(expanded));
                    pool.observe(key, self.id, candidate.halo_pixels(rect, visible), now)
                };
                if state.resident {
                    // An evicted node waits again rather than rebaking every draw.
                    state.resident = false;
                    state.since = now;
                }
                if !admitted || now.duration_since(state.since) < STABLE_FOR {
                    if !admitted {
                        pool.stats.auto_rejections += 1;
                    }
                    pool.stats.fallbacks += 1;
                    return false;
                }
                let Some(sample) = pool.bake(backend, key, now) else {
                    state.since = now;
                    pool.stats.fallbacks += 1;
                    return false;
                };
                pool.forget_demand(key);
                sample
            };
            state.resident = true;
            let resources = pool.resources.get().and_then(Option::as_ref).unwrap();
            let Some(material) = resources.sampler(backend, crate::clip::active_clip_depth())
            else {
                return false;
            };
            // An unmanaged wrapper: the pool owns the physical texture.
            let texture = Texture2D::from_miniquad_texture(sample.texture);
            material.with(|| {
                crate::clip::apply_active_clip_uniforms(&material, None);
                material.set_uniform("rect", vec4(rect.x, rect.y, rect.right(), rect.bottom()));
                let bounds = key.bounds();
                material.set_uniform("mask_bounds", vec4(bounds.x, bounds.y, bounds.w, bounds.h));
                if let Some((region, texel)) = sample.atlas {
                    material.set_uniform("atlas_region", macroquad::math::Vec4::from_array(region));
                    material.set_uniform("atlas_texel", macroquad::math::Vec2::from_array(texel));
                }
                material.set_uniform("radius", key.radius);
                material.set_uniform(
                    "color",
                    vec4(
                        shadow.color.r,
                        shadow.color.g,
                        shadow.color.b,
                        shadow.color.a * paint.opacity(),
                    ),
                );
                material.set_uniform("inverse_model", model_matrix(inverse));
                paint.draw_mask_rect(expanded, &texture);
            });
            true
        })
    }
}

struct Candidate {
    key: Key,
    projection: macroquad::math::Mat4,
    viewport: Rect,
    clip: Rect,
}
impl Candidate {
    fn visible_pixels(&self, rect: Rect) -> f64 {
        projected_area(self.projection, rect, self.viewport, self.clip).unwrap_or(0.)
    }

    fn halo_pixels(&self, rect: Rect, visible: f64) -> f64 {
        (visible
            - projected_area(self.projection, rect, self.viewport, self.clip).unwrap_or(visible))
        .max(0.)
    }
}

fn candidate_for(
    rect: Rect,
    expanded: Rect,
    shadow: Shadow,
    transform: Matrix,
    raster: ShadowProjection,
) -> Option<Candidate> {
    // The shader maps pixels back using Mue's transform, not a host's extra
    // model transform. Only cache when that mapping matches the submitted quad.
    if transform.x_axis.z != 0.
        || transform.y_axis.z != 0.
        || transform.z_axis.z != 1.
        || raster.model != model_matrix(transform)
        || ![
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            shadow.elevation,
            shadow.radius,
        ]
        .into_iter()
        .all(f32::is_finite)
        || shadow.elevation < 1.
    {
        return None;
    }
    let viewport = raster.viewport;
    let clip = raster.clip?;
    let m = raster.projection * raster.model;
    // A local texture is not a perspective-adaptive cache. Keep projective
    // transformations analytic rather than undersampling their nearest edge.
    if !m.is_finite()
        || m.x_axis.w != 0.
        || m.y_axis.w != 0.
        || m.w_axis.w <= f32::EPSILON
        || m.x_axis.z != 0.
        || m.y_axis.z != 0.
        || m.w_axis.z.abs() > m.w_axis.w
    {
        return None;
    }
    let x = crate::math::vec2(m.x_axis.x * viewport.w, m.x_axis.y * viewport.h) / (2. * m.w_axis.w);
    let y = crate::math::vec2(m.y_axis.x * viewport.w, m.y_axis.y * viewport.h) / (2. * m.w_axis.w);
    let (a, b, c) = (x.length_squared(), x.dot(y), y.length_squared());
    let scale = ((a + c + ((a - c) * (a - c) + 4. * b * b).sqrt()) * 0.5).sqrt();
    if !scale.is_finite() || scale <= 0. || scale > 4. {
        return None;
    }
    // Two samples/device pixel; quantized density avoids tiny scale jitter.
    let density = (scale.max(1.) * 4. - 0.0001).ceil() * 0.5;
    let w = (expanded.w * density).ceil() + 2.;
    let h = (expanded.h * density).ceil() + 2.;
    if w < 3. || h < 3. || w > MAX_SIDE as f32 || h > MAX_SIDE as f32 {
        return None;
    }
    let key = Key {
        width: rect.w,
        height: rect.h,
        elevation: shadow.elevation,
        radius: shadow.radius.min(rect.w.min(rect.h) * 0.5),
        pixels: (w as u32, h as u32),
    };
    if key.bytes() > MAX_MASK_BYTES {
        return None;
    }
    Some(Candidate {
        key,
        projection: m,
        viewport,
        clip,
    })
}

// The rounded knockout is convex. If the entire physical clip lies strictly
// inside it, the shadow contributes zero alpha even though the quad intersects.
// Keep a margin larger than the shader eps/ein; uncertain edge/corner cases draw.
fn clip_inside_box(
    m: macroquad::math::Mat4,
    viewport: Rect,
    clip: Rect,
    rect: Rect,
    radius: f32,
) -> bool {
    // Invert the canvas plane only. A full 4x4 inverse at clip z=0 is not
    // the inverse of a tilted 2D plane under an orthographic camera.
    let plane = Matrix::from_cols_array(&[
        m.x_axis.x, m.x_axis.y, 0., m.y_axis.x, m.y_axis.y, 0., m.w_axis.x, m.w_axis.y, m.w_axis.w,
    ]);
    let inverse = plane.inverse();
    if !inverse.is_finite() {
        return false;
    }
    let center = rect.center();
    let half = rect.size() * 0.5;
    [
        clip.min(),
        crate::math::vec2(clip.right(), clip.y),
        clip.max(),
        crate::math::vec2(clip.x, clip.bottom()),
    ]
    .into_iter()
    .all(|point| {
        let ndc = crate::math::vec2(
            (point.x - viewport.x) / viewport.w * 2. - 1.,
            1. - (point.y - viewport.y) / viewport.h * 2.,
        );
        let local = inverse * ndc.extend(1.);
        let point = local.truncate() / local.z;
        let q = (point - center).abs() - half + crate::math::vec2(radius, radius);
        let distance = q.max(crate::math::vec2(0., 0.)).length() + q.x.max(q.y).min(0.) - radius;
        point.is_finite() && distance < -0.001
    })
}

// Clip an affine projected quad, not its AABB, to physical raster bounds. The
// halo estimate subtracts the full box (including rounded corners), so it is a
// conservative lower bound and never credits a clip wholly inside the box.
fn projected_area(m: macroquad::math::Mat4, rect: Rect, viewport: Rect, clip: Rect) -> Option<f64> {
    let mut points = [macroquad::math::Vec2::ZERO; 12];
    for (i, p) in [
        rect.min(),
        crate::math::vec2(rect.right(), rect.y),
        rect.max(),
        crate::math::vec2(rect.x, rect.bottom()),
    ]
    .into_iter()
    .enumerate()
    {
        let p = m * vec4(p.x, p.y, 0., 1.);
        points[i] = macroquad::math::vec2(
            viewport.x + (p.x / p.w + 1.) * viewport.w * 0.5,
            viewport.y + (1. - p.y / p.w) * viewport.h * 0.5,
        );
        if !points[i].is_finite() {
            return None;
        }
    }
    let mut len = 4;
    for (axis, edge, keep_above) in [
        (0, clip.x, true),
        (0, clip.right(), false),
        (1, clip.y, true),
        (1, clip.bottom(), false),
    ] {
        if len == 0 {
            return Some(0.);
        }
        let mut output = [macroquad::math::Vec2::ZERO; 12];
        let mut count = 0;
        let mut previous = points[len - 1];
        let mut was_inside = if keep_above {
            previous[axis] >= edge
        } else {
            previous[axis] <= edge
        };
        for &point in &points[..len] {
            let inside = if keep_above {
                point[axis] >= edge
            } else {
                point[axis] <= edge
            };
            if inside != was_inside {
                let t = (edge - previous[axis]) / (point[axis] - previous[axis]);
                output[count] = previous + (point - previous) * t;
                count += 1;
            }
            if inside {
                output[count] = point;
                count += 1;
            }
            previous = point;
            was_inside = inside;
        }
        points = output;
        len = count;
    }
    if len < 3 {
        return Some(0.);
    }
    let origin = points[0];
    let mut area = 0.;
    for i in 1..len - 1 {
        let a = points[i] - origin;
        let b = points[i + 1] - origin;
        area += a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64;
    }
    Some(area.abs() * 0.5)
}

#[derive(Clone, Copy)]
struct Contributor {
    id: u64,
    touched: Instant,
    halo_pixels: f64,
}
struct Demand {
    key: Key,
    started: Instant,
    touched: Instant,
    observations: u32,
    halo_work: f64,
    contributors: [Option<Contributor>; SHARED_CONTRIBUTORS],
}
impl Demand {
    fn new(key: Key, now: Instant) -> Self {
        Self {
            key,
            started: now,
            touched: now,
            observations: 0,
            halo_work: 0.,
            contributors: [None; SHARED_CONTRIBUTORS],
        }
    }
    fn observe(&mut self, id: u64, halo: f64, now: Instant) -> bool {
        // Fixed tumbling recent window: expire the whole work bucket, rather
        // than keeping lifetime counts that would eventually admit any shadow.
        if now.duration_since(self.started) > DEMAND_WINDOW {
            *self = Self::new(self.key, now);
        }
        self.touched = now;
        for entry in &mut self.contributors {
            if entry.is_some_and(|entry| now.duration_since(entry.touched) > STABLE_FOR) {
                *entry = None;
            }
        }
        let entry = self
            .contributors
            .iter()
            .position(|entry| entry.is_some_and(|entry| entry.id == id))
            .or_else(|| self.contributors.iter().position(Option::is_none))
            // Keep the strongest four simultaneous contributors. A few clipped
            // rows encountered first must not hide later full-size peers.
            .or_else(|| {
                self.contributors
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| entry.map(|entry| (i, entry.halo_pixels)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .filter(|(_, area)| halo > *area)
                    .map(|(i, _)| i)
            });
        if let Some(index) = entry {
            self.contributors[index] = Some(Contributor {
                id,
                touched: now,
                halo_pixels: halo,
            });
        }
        self.observations = self.observations.saturating_add(1);
        let required = REUSE_WORK_FACTOR * self.key.pixels.0 as f64 * self.key.pixels.1 as f64;
        self.halo_work = (self.halo_work + halo).min(required);
        let shared = self.contributors.iter().flatten().count() == SHARED_CONTRIBUTORS
            && self
                .contributors
                .iter()
                .flatten()
                .map(|entry| entry.halo_pixels)
                .sum::<f64>()
                >= MIN_HALO_PIXELS;
        (halo >= MIN_HALO_PIXELS || shared)
            && self.observations >= MIN_OBSERVATIONS
            && self.halo_work >= required
    }
}

struct Slot {
    key: Option<Key>,
    texture: TextureId,
    pass: RenderPass,
    pixels: (u32, u32),
    touched: Instant,
    usable: bool,
}
impl Slot {
    fn bytes(&self) -> usize {
        self.pixels.0 as usize * self.pixels.1 as usize * 4
    }

    fn reusable(&self, now: Instant) -> bool {
        self.key.is_none() || now.duration_since(self.touched) >= STABLE_FOR
    }
}

#[derive(Default)]
struct Pool {
    slots: Vec<Slot>,
    atlas: VulkanAtlas,
    demands: Vec<Demand>,
    // Some(None) remembers an initialization failure without repeating GPU work.
    resources: OnceCell<Option<Resources>>,
    last_attempt: Option<Instant>,
    stats: ShadowMaskCacheStats,
}
thread_local! {
    static POOL: RefCell<Pool> = RefCell::default();
    static BACKEND: OnceCell<Option<MaskBackend>> = const { OnceCell::new() };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskBackend {
    Gl,
    Vulkan,
}

impl MaskBackend {
    fn current() -> Option<Self> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        BACKEND.with(|backend| {
            *backend.get_or_init(|| {
                match unsafe { get_internal_gl() }.quad_context.info().backend {
                    Backend::OpenGl => Some(Self::Gl),
                    #[cfg(all(
                        feature = "vulkan",
                        any(
                            windows,
                            target_os = "android",
                            all(target_os = "linux", not(target_env = "ohos"))
                        )
                    ))]
                    Backend::Vulkan => Some(Self::Vulkan),
                    _ => None,
                }
            })
        })
    }
}

struct MaskSample {
    texture: TextureId,
    // Normalized origin and extent, plus texel size, for the Vulkan atlas.
    atlas: Option<([f32; 4], [f32; 2])>,
}

impl MaskSample {
    fn full(texture: TextureId) -> Self {
        Self {
            texture,
            atlas: None,
        }
    }
}

impl Pool {
    fn observe(&mut self, key: Key, id: u64, halo: f64, now: Instant) -> bool {
        if halo <= 0. {
            return false;
        }
        self.demands
            .retain(|entry| now.duration_since(entry.touched) <= DEMAND_WINDOW);
        let index = if let Some(index) = self.demands.iter().position(|entry| entry.key == key) {
            index
        } else {
            let index = if self.demands.len() < MAX_PENDING_KEYS {
                self.demands.push(Demand::new(key, now));
                self.demands.len() - 1
            } else {
                self.demands
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, entry)| entry.touched)
                    .map(|(index, _)| index)
                    .unwrap()
            };
            self.demands[index] = Demand::new(key, now);
            index
        };
        self.demands[index].observe(id, halo, now)
    }
    fn forget_demand(&mut self, key: Key) {
        self.demands.retain(|entry| entry.key != key);
    }

    fn find(&mut self, backend: MaskBackend, key: Key, now: Instant) -> Option<MaskSample> {
        if backend == MaskBackend::Vulkan {
            let sample = self.atlas.find(key, now)?;
            self.stats.hits += 1;
            return Some(sample);
        }
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.usable && slot.key == Some(key))
        {
            slot.touched = now;
            self.stats.hits += 1;
            return Some(MaskSample::full(slot.texture));
        }
        None
    }

    fn bake(&mut self, backend: MaskBackend, key: Key, now: Instant) -> Option<MaskSample> {
        if self.resources.get().is_some_and(|resources| {
            resources.as_ref().is_none_or(|resources| {
                key.pixels.0 > resources.max_side || key.pixels.1 > resources.max_side
            })
        }) || self
            .last_attempt
            .is_some_and(|last| now.duration_since(last) < BAKE_INTERVAL)
        {
            return None;
        }
        if backend == MaskBackend::Vulkan {
            if !self.atlas.can_bake(key, now) {
                return None;
            }
            let resources = self
                .resources
                .get_or_init(|| Resources::new(backend))
                .as_ref()?;
            let result = self.atlas.bake(
                key,
                now,
                &resources.baker,
                resources.max_side,
                &mut self.stats,
            );
            self.last_attempt = Some(Instant::now());
            return result;
        }
        // Reuse invalid slots first, including failed storage. Never allocate
        // more metadata to work around a transient failure in an existing slot.
        let planned = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.key.is_none())
            .min_by_key(|(_, slot)| slot.touched)
            .map(|(i, _)| i)
            .or_else(|| {
                if self.slots.len() < MAX_SLOTS
                    && self.slots.iter().map(Slot::bytes).sum::<usize>() + 4 <= MAX_BYTES
                {
                    None
                } else {
                    self.slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| slot.reusable(now))
                        .min_by_key(|(_, slot)| slot.touched)
                        .map(|(i, _)| i)
                }
            });
        if planned.is_none() && self.slots.len() >= MAX_SLOTS {
            return None;
        }
        // Plan before mutating keys: every other evictable slot still costs
        // four bytes after shrinking. Freeze eligibility at this draw's time,
        // so a slow flush/shader initialization cannot evict newly expired peers.
        let other_bytes: usize = self
            .slots
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != planned)
            .map(|(_, slot)| if slot.reusable(now) { 4 } else { slot.bytes() })
            .sum();
        if other_bytes + key.bytes() > MAX_BYTES {
            return None;
        }
        let texture = self.bake_planned(key, planned, now);
        // Failed GPU work is throttled too; node-local stability is not a
        // global attempt budget. Pure CPU rejections above do not spend it.
        self.last_attempt = Some(Instant::now());
        texture.map(MaskSample::full)
    }

    fn bake_planned(
        &mut self,
        key: Key,
        planned: Option<usize>,
        now: Instant,
    ) -> Option<TextureId> {
        // Submit old sampling before reusing any storage; this is an ordering
        // boundary, not a GPU fence or restoration of a host raw pass.
        unsafe { get_internal_gl() }.flush();
        // Do not attribute an earlier GL error to this texture allocation.
        if !gl_ok() {
            return None;
        }
        let resources = self
            .resources
            .get_or_init(|| Resources::new(MaskBackend::Gl))
            .as_ref()?;
        if key.pixels.0 > resources.max_side || key.pixels.1 > resources.max_side {
            return None;
        }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let index = if let Some(index) = planned {
            index
        } else {
            let texture = ctx.try_new_render_texture(mq::TextureParams {
                width: 1,
                height: 1,
                ..Default::default()
            })?;
            let pass = ctx.new_render_pass(texture, None);
            if !gl_ok() {
                ctx.delete_render_pass(pass);
                ctx.delete_texture(texture);
                return None;
            }
            self.slots.push(Slot {
                key: None,
                texture,
                pass,
                pixels: (1, 1),
                touched: now,
                usable: true,
            });
            self.slots.len() - 1
        };
        while self.slots.iter().map(Slot::bytes).sum::<usize>() - self.slots[index].bytes()
            + key.bytes()
            > MAX_BYTES
        {
            let other = self
                .slots
                .iter()
                .enumerate()
                .filter(|(i, slot)| *i != index && slot.bytes() > 4 && slot.reusable(now))
                .min_by_key(|(_, slot)| slot.touched)
                .map(|(i, _)| i)?;
            if self.slots[other].key.take().is_some() {
                self.stats.evictions += 1;
            }
            if !resize(ctx, &mut self.slots[other], (1, 1)) {
                return None;
            }
        }
        let slot = &mut self.slots[index];
        slot.touched = now;
        if slot.key.take().is_some() {
            self.stats.evictions += 1;
        }
        if !resize(ctx, slot, key.pixels) {
            return None;
        }
        if !resources.baker.bake(ctx, slot, key) {
            // A failed pass must not strand a full-sized, unusable slot forever.
            // Try to release its storage now; the fixed slot can be retried later.
            slot.usable = false;
            resize(ctx, slot, (1, 1));
            return None;
        }
        slot.key = Some(key);
        slot.touched = Instant::now();
        self.stats.bakes += 1;
        Some(slot.texture)
    }
}

fn gl_ok() -> bool {
    unsafe { mq::gl::glGetError() == mq::gl::GL_NO_ERROR }
}
fn resize(ctx: &mut dyn RenderingBackend, slot: &mut Slot, pixels: (u32, u32)) -> bool {
    if slot.pixels != pixels || !slot.usable {
        if !ctx.try_texture_resize(slot.texture, pixels.0, pixels.1, None) {
            return false;
        }
        slot.pixels = pixels;
        slot.usable = true;
    }
    true
}

struct Resources {
    sampler: UiMaterial,
    clip_samplers: RefCell<std::collections::HashMap<usize, Option<UiMaterial>>>,
    baker: Baker,
    max_side: u32,
}
impl Resources {
    fn new(backend: MaskBackend) -> Option<Self> {
        let max_side = unsafe { get_internal_gl() }
            .quad_context
            .max_texture_size()?
            .min(MAX_SIDE);
        let sampler = new_sampler(backend, 0)?;
        if backend == MaskBackend::Gl && !gl_ok() {
            return None;
        }
        let baker = Baker::new(unsafe { get_internal_gl() }.quad_context, backend)?;
        Some(Self {
            sampler,
            clip_samplers: RefCell::new(std::collections::HashMap::new()),
            baker,
            max_side,
        })
    }

    fn sampler(&self, backend: MaskBackend, depth: usize) -> Option<UiMaterial> {
        if depth == 0 {
            return Some(self.sampler.clone());
        }
        self.clip_samplers
            .borrow_mut()
            .entry(depth)
            .or_insert_with(|| new_sampler(backend, depth))
            .clone()
    }
}

fn sampler_source(backend: MaskBackend, depth: usize) -> (String, Vec<UniformDesc>) {
    let mut uniforms = vec![
        UniformDesc::new("rect", UniformType::Float4),
        UniformDesc::new("mask_bounds", UniformType::Float4),
        UniformDesc::new("radius", UniformType::Float1),
        UniformDesc::new("color", UniformType::Float4),
        UniformDesc::new("inverse_model", UniformType::Mat4),
    ];
    let fragment_main = if backend == MaskBackend::Vulkan {
        uniforms.extend([
            UniformDesc::new("atlas_region", UniformType::Float4),
            UniformDesc::new("atlas_texel", UniformType::Float2),
        ]);
        ATLAS_SAMPLE_FRAGMENT
    } else {
        GL_SAMPLE_FRAGMENT
    };
    uniforms.extend(crate::clip::clip_uniforms(depth));
    let mut fragment = String::from("#version 100\n");
    if backend == MaskBackend::Vulkan {
        // Miniquad translates the ESSL extension's texture2DLodEXT to textureLod.
        fragment.push_str("#extension GL_EXT_shader_texture_lod : require\n");
    }
    fragment.push_str(SAMPLE_FRAGMENT_PREFIX);
    fragment.push_str(&crate::clip::clip_shader_helpers(depth));
    fragment.push_str(fragment_main);
    (fragment, uniforms)
}

fn new_sampler(backend: MaskBackend, depth: usize) -> Option<UiMaterial> {
    let (fragment, uniforms) = sampler_source(backend, depth);
    UiMaterial::new(&fragment, uniforms, Vec::new()).ok()
}

#[cfg(all(
    test,
    feature = "vulkan",
    any(
        windows,
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
#[test]
fn vulkan_mask_sampling_uses_explicit_lod() {
    let (fragment, uniforms) = sampler_source(MaskBackend::Vulkan, 2);
    // Match the standard uniforms/images prepended by QuadGl.
    let mut layout = vec![
        UniformDesc::new("Projection", UniformType::Mat4),
        UniformDesc::new("Model", UniformType::Mat4),
        UniformDesc::new("_Time", UniformType::Float4),
    ];
    layout.extend(uniforms);
    let (_, spirv) = mq::graphics::vulkan::compile_glsl(
        crate::material::UI_VERTEX,
        &fragment,
        &ShaderMeta {
            uniforms: UniformBlockLayout { uniforms: layout },
            images: vec!["Texture".into(), "_ScreenTexture".into()],
        },
    )
    .unwrap();
    const OP_IMAGE_SAMPLE_IMPLICIT_LOD: u32 = 87;
    const OP_IMAGE_SAMPLE_EXPLICIT_LOD: u32 = 88;
    let mut instructions = &spirv[5..];
    let mut explicit_samples = 0;
    while let Some(&header) = instructions.first() {
        let count = (header >> 16) as usize;
        assert!(count > 0);
        let opcode = header & 0xffff;
        assert_ne!(opcode, OP_IMAGE_SAMPLE_IMPLICIT_LOD);
        if opcode == OP_IMAGE_SAMPLE_EXPLICIT_LOD {
            explicit_samples += 1;
        }
        instructions = &instructions[count..];
    }
    assert_eq!(explicit_samples, 1);
}

#[repr(C)]
struct BakeUniforms {
    bounds: [f32; 4],
    rect: [f32; 4],
    elevation: f32,
    radius: f32,
}
struct Baker {
    pipeline: Pipeline,
    vertices: BufferId,
    indices: BufferId,
    _shader: ShaderId,
}
impl Baker {
    fn new(ctx: &mut dyn RenderingBackend, backend: MaskBackend) -> Option<Self> {
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: BAKE_VERTEX,
                    fragment: BAKE_FRAGMENT,
                },
                ShaderMeta {
                    images: Vec::new(),
                    uniforms: UniformBlockLayout {
                        uniforms: vec![
                            UniformDesc::new("bounds", UniformType::Float4),
                            UniformDesc::new("rect", UniformType::Float4),
                            UniformDesc::new("elevation", UniformType::Float1),
                            UniformDesc::new("radius", UniformType::Float1),
                        ],
                    },
                },
            )
            .ok()?;
        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("position", VertexFormat::Float2)],
            shader,
            PipelineParams::default(),
        ); // No blending: do not store alpha squared.
        let vertices = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&[[0.0f32, 0.], [1., 0.], [0., 1.], [1., 1.]]),
        );
        let indices = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&[0u16, 1, 3, 0, 3, 2]),
        );
        if backend == MaskBackend::Gl && !gl_ok() {
            ctx.delete_buffer(vertices);
            ctx.delete_buffer(indices);
            ctx.delete_pipeline(pipeline);
            ctx.delete_shader(shader);
            return None;
        }
        Some(Self {
            pipeline,
            vertices,
            indices,
            _shader: shader,
        })
    }
    fn bake(&self, ctx: &mut dyn RenderingBackend, slot: &Slot, key: Key) -> bool {
        ctx.begin_pass(Some(slot.pass), PassAction::Nothing);
        let complete = unsafe {
            mq::gl::glCheckFramebufferStatus(mq::gl::GL_FRAMEBUFFER)
                == mq::gl::GL_FRAMEBUFFER_COMPLETE
        };
        if complete {
            self.draw_region(
                ctx,
                key,
                [0, 0, key.pixels.0 as i32, key.pixels.1 as i32],
                true,
            );
        }
        ctx.end_render_pass();
        let clean = gl_ok();
        complete && clean
    }

    fn bake_atlas(
        &self,
        ctx: &mut dyn RenderingBackend,
        pass: RenderPass,
        texture: TextureId,
        key: Key,
        region: [i32; 4],
        first: bool,
    ) {
        // Only the first bake clears; a later clear would wipe other masks.
        // The unblended draw fills its whole allocated region, including padding.
        let action = if first {
            PassAction::clear_color(0., 0., 0., 0.)
        } else {
            PassAction::Nothing
        };
        ctx.begin_pass(Some(pass), action);
        self.draw_region(ctx, key, region, false);
        ctx.end_render_pass();
        ctx.prepare_render_texture_for_sampling(texture);
    }

    fn draw_region(
        &self,
        ctx: &mut dyn RenderingBackend,
        key: Key,
        [x, y, w, h]: [i32; 4],
        clear_gl: bool,
    ) {
        ctx.apply_pipeline(&self.pipeline);
        ctx.apply_viewport(x, y, w, h);
        ctx.apply_scissor_rect(x, y, w, h);
        if clear_gl {
            // GL clears use the bound pipeline's color mask and the scissor.
            ctx.clear(Some((0., 0., 0., 0.)), None, None);
        }
        ctx.apply_bindings_from_slice(&[self.vertices], self.indices, &[]);
        let bounds = key.bounds();
        ctx.apply_uniforms(UniformsSource::table(&BakeUniforms {
            bounds: [bounds.x, bounds.y, bounds.w, bounds.h],
            rect: [0., 0., key.width, key.height],
            elevation: key.elevation,
            radius: key.radius,
        }));
        ctx.draw(0, 6, 1);
    }
}

/// Read this render thread's mask pool without allocating GPU resources.
pub fn shadow_mask_cache_stats() -> ShadowMaskCacheStats {
    POOL.with_borrow(|pool| ShadowMaskCacheStats {
        slots: pool.slots.len() + pool.atlas.len(),
        resident_masks: pool.slots.iter().filter(|slot| slot.key.is_some()).count()
            + pool.atlas.len(),
        allocated_bytes: pool.slots.iter().map(Slot::bytes).sum::<usize>()
            + pool.atlas.allocated_bytes(),
        pending_keys: pool
            .demands
            .iter()
            .filter(|entry| {
                entry.touched.elapsed() <= DEMAND_WINDOW && entry.started.elapsed() <= DEMAND_WINDOW
            })
            .count(),
        ..pool.stats
    })
}

/// Invalidate masks and admission history on the live render thread, outside
/// a host raw pass. OpenGL flushes and shrinks its slots to 1x1; Vulkan keeps
/// its fixed pages (later overwrites are ordered on the graphics queue).
/// Backing objects and shaders persist until context teardown.
pub fn clear_shadow_mask_cache() {
    POOL.with_borrow_mut(|pool| {
        pool.demands.clear();
        pool.atlas.clear();
        if pool.slots.is_empty() {
            return;
        }
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        for slot in &mut pool.slots {
            slot.key = None;
            resize(gl.quad_context, slot, (1, 1));
        }
    });
}
