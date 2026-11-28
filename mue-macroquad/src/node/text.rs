use super::{Node, NodeContext};
use crate::node::atlas::{Atlas, DEFAULT_PAGE_SIZE};
use std::{cell::RefCell, collections::HashSet, ops::Range, rc::Rc};

pub use super::atlas::GlyphAtlasStats;

use cosmic_text::{
    Attrs, Buffer, CacheKey, FamilyOwned, FontSystem, Metrics, Shaping, Style as FontStyle,
    SwashCache, Weight, Wrap,
};
use glam::{DVec2, Vec3};
use mue_core::{
    effect::{computed, watch_effect},
    prop::Prop,
    signal::{Access, signal},
};
use taffy::{AvailableSpace, Size};

use crate::{
    app::runtime::Runtime,
    layout::use_layout,
    math::{Matrix, Rect, vec2},
    paint::color::Color,
    paint::shader::Brush,
    paint::{
        current_clip_logical, current_transform, rect_visible, retained_enabled, transform_rect,
        use_background, use_paint,
    },
    style::Style,
};
use macroquad::retained::{self, RetainedMesh, RetainedVertex};
use miniquad::TextureId;

/// Preferred break opportunities. `text_wrap(false)` disables all wrapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    Word,
    Glyph,
    #[default]
    WordOrGlyph,
}
impl mue_core::prop::PropValue for TextWrap {}

/// Overflow at the content bounds; `text_max_lines` limits visible lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextOverflow {
    #[default]
    Visible,
    Clip,
    EllipsisStart,
    EllipsisMiddle,
    EllipsisEnd,
}
impl mue_core::prop::PropValue for TextOverflow {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextVerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}
impl mue_core::prop::PropValue for TextVerticalAlign {}

pub struct FontState {
    font_system: FontSystem,
    swash_cache: SwashCache,
    font_atlas: Option<Atlas>,
}

impl FontState {
    pub(crate) fn with_locale_and_db(locale: String, db: cosmic_text::fontdb::Database) -> Self {
        Self {
            font_system: FontSystem::new_with_locale_and_db(locale, db),
            swash_cache: SwashCache::new(),
            font_atlas: None,
        }
    }

    pub(crate) fn begin_render_pass(&mut self) {
        if let Some(atlas) = &mut self.font_atlas {
            // Submit staged pixels and queued UVs before allowing slot reuse.
            atlas.end_pass();
            unsafe { macroquad::window::get_internal_gl() }.flush();
            atlas.begin_pass();
        }
    }

    /// Flush any staged pixels left at the end of a render pass.
    pub(crate) fn end_render_pass(&mut self) {
        if let Some(atlas) = &mut self.font_atlas {
            atlas.end_pass();
        }
    }

    fn atlas_stats(&self) -> GlyphAtlasStats {
        self.glyph_atlas_stats().unwrap_or_default()
    }

    /// Counter snapshot of the glyph atlas, or `None` before any text renders.
    pub(crate) fn glyph_atlas_stats(&self) -> Option<GlyphAtlasStats> {
        self.font_atlas.as_ref().map(Atlas::stats)
    }

    /// Register the faces contained in `data` with the shared font system.
    ///
    /// Returns the family name of every face that was added, de-duplicated and in file
    /// order. The result is empty when the data contains no usable font.
    pub(crate) fn load_font_data(&mut self, data: Vec<u8>) -> Vec<String> {
        // A HashSet keeps registering a font linear instead of O(N^2) against the
        // whole system database.
        let existing: HashSet<_> = self.font_system.db().faces().map(|face| face.id).collect();
        // `db_mut` also clears cosmic-text's font match cache, so the new faces become
        // visible to the next shaping pass.
        self.font_system.db_mut().load_font_data(data);

        let mut families = Vec::new();
        for face in self.font_system.db().faces() {
            if existing.contains(&face.id) {
                continue;
            }

            if let Some((name, _)) = face.families.first()
                && !families.contains(name)
            {
                families.push(name.clone());
            }
        }
        families
    }
}

/// Largest square glyph page the driver accepts, capped at the preferred size.
///
/// Queried once, on the rendering thread, when the first glyph needs a page.
fn device_page_size() -> i32 {
    let mut size = DEFAULT_PAGE_SIZE;
    unsafe {
        let mut queried = 0;
        miniquad::gl::glGetIntegerv(miniquad::gl::GL_MAX_TEXTURE_SIZE, &mut queried);
        if queried > 0 {
            size = size.min(queried);
        }
    }
    size
}

impl Default for FontState {
    fn default() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            font_atlas: None,
        }
    }
}

fn shape_buffer(buffer: &mut Buffer, fonts: &mut FontSystem, metrics: Metrics, width: Option<f32>) {
    buffer.set_metrics_and_size(fonts, metrics, width, None);
    buffer.shape_until_scroll(fonts, false);
}

fn measured_size(buffer: &Buffer, paragraph_spacing: f32, max_lines: Option<usize>) -> Size<f32> {
    let mut size = Size::ZERO;
    for run in buffer.layout_runs().take(max_lines.unwrap_or(usize::MAX)) {
        size.width = size.width.max(run.line_w.ceil());
        size.height = run.line_top + run.line_height + run.line_i as f32 * paragraph_spacing;
    }
    size
}

const RETAINED_TEXT_MIN_GLYPHS: usize = 1_024;

#[derive(Clone, Copy)]
struct LineBaseline {
    /// Keep these as separate operands to match immediate painting's f32 rounding.
    offset: f32,
    layout_y: f32,
}

impl LineBaseline {
    fn snap(self, origin_y: f32) -> f32 {
        ((origin_y + self.offset) + self.layout_y).round()
    }
}

#[derive(Clone, Copy)]
struct LineRange {
    /// Tight bounds of the quads this line contributed, in text-local space.
    bounds: Rect,
    first: u32,
    count: u32,
    baseline: LineBaseline,
    /// Baseline baked into the mesh, relative to its original snapped node origin.
    mesh_y: f32,
}

struct RetainedTextBatch {
    texture: TextureId,
    vertices: Vec<RetainedVertex>,
    indices: Vec<u32>,
    lines: Vec<LineRange>,
}

/// Geometry of a short solid-color text, kept while nothing that affects it
/// changes. Short texts are not worth a retained mesh of their own (they
/// would break batching), but rebuilding their quads every frame costs a
/// glyph walk, atlas lookups, and vertex building per glyph. A hit only pins
/// the glyphs in the atlas and resubmits the vertices.
#[derive(Default)]
struct ImmediateTextCache {
    key: Option<ImmediateTextKey>,
    glyph_keys: Vec<CacheKey>,
    shapes: Option<Rc<crate::paint::Shapes>>,
}

#[derive(PartialEq)]
struct ImmediateTextKey {
    content: RetainedTextContentKey,
    origin: (f32, f32),
    transform: Matrix,
    alpha: f32,
    clip: Option<Rect>,
    atlas_evicted: u64,
    resident_glyphs: usize,
}

#[derive(PartialEq)]
struct RetainedTextContentKey {
    revision: u64,
    width: f32,
    style: RetainedTextStyle,
}

#[derive(PartialEq)]
struct RetainedTextKey {
    content: RetainedTextContentKey,
    atlas_evicted: u64,
    /// Counts resident keys; detects previously rejected glyphs becoming resident.
    resident_glyphs: usize,
}

#[derive(Default)]
struct RetainedTextCache {
    key: Option<RetainedTextKey>,
    /// Glyph count for `key.content`; lets cache hits skip the glyph scan.
    glyph_count: usize,
    glyph_keys: Vec<CacheKey>,
    /// A line with no mesh today may become drawable in a less crowded pass.
    retry_atlas: bool,
    meshes: Vec<RetainedMesh>,
    line_ranges: Vec<Vec<LineRange>>,
    block_bounds: Option<TextBlockBounds>,
    visible_ranges: Vec<TextDrawRange>,
}

#[derive(Debug, PartialEq)]
struct TextDrawRange {
    mesh: usize,
    indices: Range<u32>,
    translate_y: f32,
}

#[derive(Clone, Copy)]
struct TextPosition {
    node_x: f32,
    node_y: f32,
    transform: Matrix,
}

impl TextPosition {
    fn model(self, translate_y: f32) -> Matrix {
        self.transform * Matrix::from_translation(vec2(self.node_x, translate_y))
    }
}

fn text_cull_clip(transform: Matrix, clip: Option<Rect>) -> Option<Rect> {
    // Projecting the corners can miss visible text across a perspective horizon.
    // Leave clipping to the scissor in that case.
    clip.filter(|_| transform.row(2) == Vec3::Z)
}

fn text_rect_visible(clip: Option<Rect>, transform: Matrix, bounds: Rect) -> bool {
    clip.is_none_or(|clip| rect_visible(Some(clip), &transform_rect(transform, bounds)))
}

/// Bounds of the uploaded quads, plus a bound on how far re-snapping their
/// baselines can move the model away from a plain translation by `node_y`.
struct TextBlockBounds {
    min: DVec2,
    max: DVec2,
    snap_margin: f64,
}

impl TextBlockBounds {
    const BASELINE_ROUNDING: f64 = 8. * f32::EPSILON as f64;

    fn new(batches: &[Vec<LineRange>]) -> Option<Self> {
        batches
            .iter()
            .flatten()
            .map(|line| {
                let offset = f64::from(line.baseline.offset);
                let layout_y = f64::from(line.baseline.layout_y);
                let mesh_y = f64::from(line.mesh_y);
                // The residual, half-pixel snap and f32 arithmetic can all
                // affect the translation. Only the node origin varies on scroll.
                let snap_margin = (offset + layout_y - mesh_y).abs()
                    + 0.5
                    + Self::BASELINE_ROUNDING * (offset.abs() + layout_y.abs() + mesh_y.abs() + 1.);
                Self {
                    min: line.bounds.min().as_dvec2(),
                    max: line.bounds.max().as_dvec2(),
                    snap_margin,
                }
            })
            .reduce(|a, b| Self {
                min: a.min.min(b.min),
                max: a.max.max(b.max),
                snap_margin: a.snap_margin.max(b.snap_margin),
            })
    }

    /// Test an affine clip selected by `text_cull_clip`.
    fn is_outside(&self, position: TextPosition, clip: Rect) -> bool {
        let model = position.transform;
        let x = f64::from(position.node_x);
        let y = f64::from(position.node_y);
        let margin_y = self.snap_margin + Self::BASELINE_ROUNDING * y.abs();
        let a = model.x_axis.truncate().as_dvec2();
        let b = model.y_axis.truncate().as_dvec2();
        let t = model.z_axis.truncate().as_dvec2();
        let x1 = a * self.min.x;
        let x2 = a * self.max.x;
        let y1 = b * (self.min.y - margin_y);
        let y2 = b * (self.max.y + margin_y);
        let shift = a * x + b * y + t;
        let min = shift + x1.min(x2) + y1.min(y2);
        let max = shift + x1.max(x2) + y1.max(y2);
        // Bound the f32 model arithmetic by the magnitude of its terms so
        // cancellation does not hide the error. Keep a logical pixel of slack.
        let magnitude = a.abs() * x.abs()
            + b.abs() * y.abs()
            + t.abs()
            + x1.abs().max(x2.abs())
            + y1.abs().max(y2.abs());
        let slack = magnitude * (16. * f32::EPSILON as f64) + DVec2::ONE;
        (max + slack).cmplt(clip.min().as_dvec2()).any()
            || (min - slack).cmpgt(clip.max().as_dvec2()).any()
    }
}

impl RetainedTextCache {
    /// Returns false to use the immediate path for short or empty text.
    fn draw(
        &mut self,
        buffer: &Buffer,
        fonts: &mut FontState,
        content: RetainedTextContentKey,
        position: TextPosition,
        alpha: f32,
    ) -> bool {
        let same_content = self.key.as_ref().is_some_and(|key| key.content == content);
        if !same_content {
            self.glyph_count = buffer
                .layout_runs()
                .take(content.style.max_lines.unwrap_or(usize::MAX))
                .map(|run| run.glyphs.len())
                .sum();
        }
        if self.glyph_count < RETAINED_TEXT_MIN_GLYPHS {
            if !same_content {
                *self = Self {
                    key: Some(RetainedTextKey {
                        content,
                        atlas_evicted: 0,
                        resident_glyphs: 0,
                    }),
                    glyph_count: self.glyph_count,
                    ..Self::default()
                };
            }
            return false;
        }

        let clip = text_cull_clip(position.transform, current_clip_logical());
        if same_content && !self.meshes.is_empty() {
            // If glyphs were rejected under atlas pressure, previously absent
            // quads can extend beyond the cached bounds. Preserve their retry.
            if !self.retry_atlas
                && let Some(clip) = clip
                && self
                    .block_bounds
                    .as_ref()
                    .is_some_and(|bounds| bounds.is_outside(position, clip))
            {
                return true;
            }
            self.update_visible_ranges(position, clip);
            if self.visible_ranges.is_empty() && !self.retry_atlas {
                // No atlas lookups or uploads for wholly clipped cached text.
                return true;
            }
        }
        if !same_content {
            self.glyph_keys = glyph_keys(buffer, content.style.max_lines);
        }
        let before = fonts.atlas_stats();
        let resident_glyphs = pin_glyphs(&self.glyph_keys, fonts);
        let key = RetainedTextKey {
            content,
            atlas_evicted: fonts.atlas_stats().evicted,
            resident_glyphs,
        };
        if self.key.as_ref() != Some(&key) {
            let batches = build_retained_text(buffer, fonts, position.node_y, &key.content.style);
            self.meshes.clear();
            self.line_ranges.clear();
            for batch in batches {
                if let Some(mesh) =
                    RetainedMesh::new(&batch.vertices, &batch.indices, batch.texture)
                {
                    self.meshes.push(mesh);
                    self.line_ranges.push(batch.lines);
                }
            }
            self.block_bounds = TextBlockBounds::new(&self.line_ranges);
            self.key = Some(RetainedTextKey {
                atlas_evicted: fonts.atlas_stats().evicted,
                ..key
            });
            self.update_visible_ranges(position, clip);
        }
        self.retry_atlas = fonts.atlas_stats().rejected_exhausted != before.rejected_exhausted;
        if self.meshes.is_empty() {
            return false;
        }
        for range in &self.visible_ranges {
            // Each range has its own model, even when it shares a mesh with the
            // previous draw. The backend reuses the open rendering pass.
            crate::paint::material::draw_retained_meshes_ranges(
                std::slice::from_ref(&self.meshes[range.mesh]),
                &[Some(range.indices.clone())],
                crate::math::model_matrix(position.model(range.translate_y)),
                alpha,
            );
        }
        true
    }

    fn update_visible_ranges(&mut self, position: TextPosition, clip: Option<Rect>) {
        self.visible_ranges.clear();
        for (mesh, lines) in self.line_ranges.iter().enumerate() {
            visible_line_ranges(mesh, lines, position, clip, &mut self.visible_ranges);
        }
    }
}

/// Reuse the same vertices at any scroll offset. Apply the exact snapped
/// baseline as an integer translation and batch neighboring lines that agree.
fn visible_line_ranges(
    mesh: usize,
    lines: &[LineRange],
    position: TextPosition,
    clip: Option<Rect>,
    ranges: &mut Vec<TextDrawRange>,
) {
    let mut previous_y = f32::NAN;
    let mut model = Matrix::IDENTITY;
    let mut current: Option<usize> = None;
    for line in lines {
        let translate_y = line.baseline.snap(position.node_y) - line.mesh_y;
        if translate_y != previous_y {
            previous_y = translate_y;
            model = position.model(translate_y);
            // Do not bridge a different translation, even if that line is clipped.
            current = None;
        }
        if !text_rect_visible(clip, model, line.bounds) {
            continue;
        }
        let end = line.first + line.count;
        if let Some(index) = current {
            ranges[index].indices.end = end;
        } else {
            current = Some(ranges.len());
            ranges.push(TextDrawRange {
                mesh,
                indices: line.first..end,
                translate_y,
            });
        }
    }
}

fn push_quad(
    batches: &mut Vec<RetainedTextBatch>,
    texture: TextureId,
    rect: Rect,
    uv: Rect,
    color: Color,
) {
    let index = batches.iter().position(|batch| batch.texture == texture);
    let batch = match index {
        Some(index) => &mut batches[index],
        None => {
            batches.push(RetainedTextBatch {
                texture,
                vertices: Vec::new(),
                indices: Vec::new(),
                lines: Vec::new(),
            });
            let index = batches.len() - 1;
            &mut batches[index]
        }
    };
    let base = batch.vertices.len() as u32;
    let color = [color.r, color.g, color.b, color.a];
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    batch.vertices.extend_from_slice(&[
        RetainedVertex {
            position: [x, y],
            texcoord: [uv.x, uv.y],
            color,
        },
        RetainedVertex {
            position: [x + w, y],
            texcoord: [uv.right(), uv.y],
            color,
        },
        RetainedVertex {
            position: [x + w, y + h],
            texcoord: [uv.right(), uv.bottom()],
            color,
        },
        RetainedVertex {
            position: [x, y + h],
            texcoord: [uv.x, uv.bottom()],
            color,
        },
    ]);
    batch
        .indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn glyph_keys(buffer: &Buffer, max_lines: Option<usize>) -> Vec<CacheKey> {
    let mut seen = HashSet::new();
    buffer
        .layout_runs()
        .take(max_lines.unwrap_or(usize::MAX))
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.physical((0., 0.), 1.0).cache_key)
        .filter(|&key| seen.insert(key))
        .collect()
}

/// Keep every atlas entry referenced by a visible mesh resident. Each key only
/// needs to be pinned once, even if it appears throughout a long paragraph.
fn pin_glyphs(keys: &[CacheKey], fonts: &mut FontState) -> usize {
    if keys.is_empty() {
        return 0;
    }
    let atlas = fonts
        .font_atlas
        .get_or_insert_with(|| Atlas::new(device_page_size()));
    let resident = keys
        .iter()
        .filter(|&&key| {
            atlas
                .cache_glyph(&mut fonts.font_system, &mut fonts.swash_cache, key)
                .is_some()
        })
        .count();
    atlas.end_pass();
    resident
}

#[derive(PartialEq)]
struct RetainedTextStyle {
    y: f32,
    spacing: f32,
    font_size: f32,
    max_lines: Option<usize>,
    strike: bool,
    underline: bool,
    color: Color,
}

fn build_retained_text(
    buffer: &Buffer,
    fonts: &mut FontState,
    origin_y: f32,
    style: &RetainedTextStyle,
) -> Vec<RetainedTextBatch> {
    let RetainedTextStyle {
        y,
        spacing,
        font_size,
        max_lines,
        strike,
        underline,
        color,
    } = *style;
    let white = retained::white_texture();
    let white_uv = Rect::new(0., 0., 1., 1.);
    // Keep the solid batch first so underlines/strikes are painted before the
    // glyphs, matching the immediate path.
    let mut batches = vec![RetainedTextBatch {
        texture: white,
        vertices: Vec::new(),
        indices: Vec::new(),
        lines: Vec::new(),
    }];
    for run in buffer.layout_runs().take(max_lines.unwrap_or(usize::MAX)) {
        let baseline = LineBaseline {
            offset: y + run.line_i as f32 * spacing,
            layout_y: run.line_y,
        };
        let starts: Vec<usize> = batches.iter().map(|batch| batch.indices.len()).collect();
        let mesh_y = baseline.snap(origin_y) - origin_y.round();
        let pen = vec2(0., mesh_y);
        // Tight bounds of everything this line contributes. Using the quads
        // themselves instead of the line box keeps glyphs that overhang the
        // ascent/descent from being culled while still visible.
        let mut line_bounds: Option<Rect> = None;
        let mut add_bounds = |rect: Rect| {
            line_bounds = Some(match line_bounds {
                Some(bounds) => bounds.union(&rect),
                None => rect,
            });
        };
        if strike {
            let left = run
                .glyphs
                .iter()
                .map(|glyph| glyph.x)
                .reduce(f32::min)
                .unwrap_or(0.);
            let rect = Rect::new(
                pen.x + left,
                pen.y - font_size * 0.3,
                run.line_w,
                (font_size / 16.).max(1.),
            );
            push_quad(&mut batches, white, rect, white_uv, color);
            add_bounds(rect);
        }
        if underline {
            let left = run
                .glyphs
                .iter()
                .map(|glyph| glyph.x)
                .reduce(f32::min)
                .unwrap_or(0.);
            let rect = Rect::new(
                pen.x + left,
                pen.y + 2.,
                run.line_w,
                (font_size / 16.).max(1.),
            );
            push_quad(&mut batches, white, rect, white_uv, color);
            add_bounds(rect);
        }
        for glyph in run.glyphs.iter() {
            let physical_glyph = glyph.physical((0., 0.), 1.0);
            let atlas = fonts
                .font_atlas
                .get_or_insert_with(|| Atlas::new(device_page_size()));
            let Some(cached) = atlas.cache_glyph(
                &mut fonts.font_system,
                &mut fonts.swash_cache,
                physical_glyph.cache_key,
            ) else {
                continue;
            };
            let placement = cached.placement;
            let draw_rect = Rect::new(
                (physical_glyph.x + placement.left) as f32 + pen.x,
                (physical_glyph.y - placement.top) as f32 + pen.y,
                placement.width as f32,
                placement.height as f32,
            );
            let glyph_color = glyph.color_opt.map_or_else(
                || color,
                |color| Color::from_rgba(color.r(), color.g(), color.b(), color.a()),
            );
            push_quad(
                &mut batches,
                cached.texture.raw_miniquad_id(),
                draw_rect,
                cached.uv,
                glyph_color,
            );
            add_bounds(draw_rect);
        }
        let Some(line_bounds) = line_bounds else {
            continue;
        };
        for (index, batch) in batches.iter_mut().enumerate() {
            // A texture batch created during this line starts at index 0.
            let start = starts.get(index).copied().unwrap_or(0);
            if batch.indices.len() > start {
                batch.lines.push(LineRange {
                    bounds: line_bounds,
                    first: start as u32,
                    count: (batch.indices.len() - start) as u32,
                    baseline,
                    mesh_y,
                });
            }
        }
    }
    if let Some(atlas) = &mut fonts.font_atlas {
        atlas.end_pass();
    }
    batches
}

/// Draw one layout run's glyphs with their baseline at `pen` (immediate path,
/// shared with the editable text node).
pub(crate) fn draw_run_glyphs(
    p: &mut crate::paint::ShapesBuilder,
    fonts: &mut FontState,
    run: &cosmic_text::LayoutRun,
    pen: crate::math::Vector,
    brush: &Brush,
    shader_bounds: Rect,
) {
    for glyph in run.glyphs.iter() {
        let physical_glyph = glyph.physical((0., 0.), 1.0);
        let atlas = fonts
            .font_atlas
            .get_or_insert_with(|| Atlas::new(device_page_size()));
        let Some(cached) = atlas.cache_glyph(
            &mut fonts.font_system,
            &mut fonts.swash_cache,
            physical_glyph.cache_key,
        ) else {
            continue;
        };
        let placement = cached.placement;
        let draw_rect = Rect::new(
            (physical_glyph.x + placement.left) as f32 + pen.x,
            (physical_glyph.y - placement.top) as f32 + pen.y,
            placement.width as f32,
            placement.height as f32,
        );
        p.draw_texture(
            draw_rect,
            cached.texture,
            cached.uv,
            brush.shader(shader_bounds),
        );
    }
}

/// Finish a glyph pass started by [`draw_run_glyphs`].
pub(crate) fn end_glyph_pass(fonts: &mut FontState) {
    if let Some(atlas) = &mut fonts.font_atlas {
        atlas.end_pass();
    }
}

impl FontState {
    pub(crate) fn font_system(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }
}

/// Display a plain reactive string with the renderer's text node.
#[mue_macros::node]
pub fn text(ctx: NodeContext, text: Rc<str>) -> Node {
    ctx.finish_without_defaults(move |style| {
        let spans = text.map(|value| vec![Span::new(value.to_string())]);
        use_spans(style, spans);
    })
}

pub fn use_spans(style: &mut Style, content: Prop<Vec<Span>>) {
    let layout = use_layout(style);
    let rect = layout.rect;
    let layout_id = layout.layout_id;

    let paint = use_paint(style);
    use_background(style, rect);
    let declared_font_size = style.declared_font_size();
    let font_size = crate::node::use_font_size();
    crate::node::NodeInner::with_mut(|node| {
        if node.font_size.is_none() {
            node.font_size = declared_font_size;
        }
    });
    let line_height = crate::style::units::resolve_length(style.take_line_height(), "line_height");
    let line_height_multiplier = style.take_line_height_multiplier();
    let font_family = style.take_font_family();
    let font_weight = style.take_font_weight();
    let font_italic = style.take_font_italic();
    let text_wrap = style.take_text_wrap();
    let wrap_mode = style.take_text_wrap_mode();
    let overflow = style.take_text_overflow();
    let max_lines = style.take_text_max_lines();
    let spacing =
        crate::style::units::resolve_length(style.take_paragraph_spacing(), "paragraph_spacing");
    let letter_spacing =
        crate::style::units::resolve_length(style.take_letter_spacing(), "letter_spacing");
    let tab_width = style.take_tab_width();
    let vertical_align = style.take_text_vertical_align();
    let strike = style.take_text_strikethrough();
    let text_underline = style.take_text_underline();
    let color = style
        .declared_color()
        .or_else(crate::inherit::inherited_foreground)
        .unwrap_or_else(|| style.take_color());

    let metrics = computed(move |_| {
        Metrics::new(
            font_size.get().max(1.),
            line_height_multiplier
                .get()
                .map_or_else(|| line_height.get(), |factor| font_size.get() * factor)
                .max(1.),
        )
    });

    let buffer = Rc::new(RefCell::new(Runtime::with_fonts_mut(|fonts| {
        Buffer::new(&mut fonts.font_system, metrics.get())
    })));

    layout.set_measure_fn({
        let buffer = buffer.clone();
        move |known_dimensions: Size<Option<f32>>,
              available_space: Size<AvailableSpace>|
              -> Size<f32> {
            if let Size {
                width: Some(width),
                height: Some(height),
            } = known_dimensions
            {
                return Size { width, height };
            }

            let width_constraint = known_dimensions.width.or(match available_space.width {
                AvailableSpace::Definite(w) => Some(w),
                AvailableSpace::MaxContent => None,
                AvailableSpace::MinContent => Some(0.),
            });

            let mut buffer = buffer.borrow_mut();
            Runtime::with_fonts_mut(|fonts| {
                shape_buffer(
                    &mut buffer,
                    &mut fonts.font_system,
                    metrics.get_untracked(),
                    width_constraint,
                );
            });
            let measured = measured_size(&buffer, spacing.get().max(0.), max_lines.get());

            Size {
                width: known_dimensions.width.unwrap_or(measured.width),
                height: known_dimensions.height.unwrap_or(measured.height),
            }
        }
    });

    let revision = signal(0u64);
    watch_effect({
        let buffer = buffer.clone();
        let align = style.take_text_align();
        move || {
            let mut buffer = buffer.borrow_mut();
            Runtime::with_fonts_mut(|fonts| {
                let family = font_family.get_clone();
                let attrs = Attrs::new()
                    .family(family.as_family())
                    .weight(font_weight.get())
                    // cosmic-text takes letter spacing in em; the style is in pixels.
                    .letter_spacing(letter_spacing.get() / metrics.get().font_size.max(1.))
                    .style(if font_italic.get() {
                        cosmic_text::Style::Italic
                    } else {
                        cosmic_text::Style::Normal
                    });
                let mut buffer = buffer.borrow_with(&mut fonts.font_system);
                let content = content.get_clone();
                let metrics = metrics.get();
                buffer.set_wrap(if text_wrap.get() {
                    match wrap_mode.get() {
                        TextWrap::Word => Wrap::Word,
                        TextWrap::Glyph => Wrap::Glyph,
                        TextWrap::WordOrGlyph => Wrap::WordOrGlyph,
                    }
                } else {
                    Wrap::None
                });
                use cosmic_text::{Ellipsize, EllipsizeHeightLimit};
                let limit = EllipsizeHeightLimit::Lines(max_lines.get().unwrap_or(1).max(1));
                buffer.set_ellipsize(match overflow.get() {
                    TextOverflow::Visible | TextOverflow::Clip => Ellipsize::None,
                    TextOverflow::EllipsisStart => Ellipsize::Start(limit),
                    TextOverflow::EllipsisMiddle => Ellipsize::Middle(limit),
                    TextOverflow::EllipsisEnd => Ellipsize::End(limit),
                });
                buffer.set_tab_width(tab_width.get().max(1));
                buffer.set_rich_text(
                    content
                        .iter()
                        .map(|span| (span.text.as_str(), span.style.attrs(&attrs, metrics))),
                    &attrs,
                    Shaping::Advanced,
                    align.get(),
                );
                buffer.set_metrics_and_size(metrics, None, None);
                let _ = spacing.get();
                layout.mark_dirty();
                revision.update(|value| *value = value.wrapping_add(1));
            });
        }
    });

    let mut retained_cache = RetainedTextCache::default();
    let mut immediate_cache = ImmediateTextCache::default();
    style.on_render.append(move |_| {
        let mut buffer = buffer.borrow_mut();
        Runtime::with_fonts_mut(|fonts| {
            let outer_bounds = rect.get();
            let bounds = Runtime::with_taffy_mut(|taffy| {
                let layout = taffy.layout(layout_id).unwrap();
                let left = layout.padding.left + layout.border.left;
                let top = layout.padding.top + layout.border.top;
                Rect::new(
                    outer_bounds.x + left,
                    outer_bounds.y + top,
                    (outer_bounds.w - left - layout.padding.right - layout.border.right).max(0.),
                    (outer_bounds.h - top - layout.padding.bottom - layout.border.bottom).max(0.),
                )
            });
            // Visible overflow must not hide lines after a size or font transition.
            shape_buffer(
                &mut buffer,
                &mut fonts.font_system,
                metrics.get_untracked(),
                Some(bounds.w),
            );
            let spacing = spacing.get().max(0.);
            let max_lines = max_lines.get();
            let font_size = font_size.get();
            let y = match vertical_align.get() {
                TextVerticalAlign::Top => 0.,
                align => {
                    let height = measured_size(&buffer, spacing, max_lines).height;
                    let free = (bounds.h - height).max(0.);
                    if align == TextVerticalAlign::Center {
                        free / 2.
                    } else {
                        free
                    }
                }
            };
            let color_brush = color.get_clone();
            let strike = strike.get();
            let underline = text_underline.get();
            let clip_text = max_lines.is_some() || overflow.get() != TextOverflow::Visible;
            match &color_brush {
                Brush::Solid(color) if retained_enabled() => {
                    let mut draw = || {
                        retained_cache.draw(
                            &buffer,
                            fonts,
                            RetainedTextContentKey {
                                revision: revision.get_untracked(),
                                width: bounds.w,
                                style: RetainedTextStyle {
                                    y,
                                    spacing,
                                    font_size,
                                    max_lines,
                                    strike,
                                    underline,
                                    color: *color,
                                },
                            },
                            TextPosition {
                                node_x: bounds.x.round(),
                                node_y: bounds.y,
                                transform: current_transform(),
                            },
                            paint.opacity(),
                        )
                    };
                    let handled = if clip_text {
                        crate::paint::with_clip(bounds, draw)
                    } else {
                        draw()
                    };
                    if handled {
                        return;
                    }
                }
                Brush::Solid(_) => {}
                // Retained quads use a single tint. Keep gradients in the
                // immediate path for per-vertex samples in brush coordinates.
                _ => retained_cache = RetainedTextCache::default(),
            }

            let transform = current_transform();
            let clip = text_cull_clip(transform, current_clip_logical());
            let cache_key = match &color_brush {
                Brush::Solid(color) => {
                    let content = RetainedTextContentKey {
                        revision: revision.get_untracked(),
                        width: bounds.w,
                        style: RetainedTextStyle {
                            y,
                            spacing,
                            font_size,
                            max_lines,
                            strike,
                            underline,
                            color: *color,
                        },
                    };
                    let same_content = immediate_cache
                        .key
                        .as_ref()
                        .is_some_and(|key| key.content == content);
                    if !same_content {
                        immediate_cache.glyph_keys = glyph_keys(&buffer, max_lines);
                    }
                    let resident_glyphs = pin_glyphs(&immediate_cache.glyph_keys, fonts);
                    Some(ImmediateTextKey {
                        content,
                        origin: (bounds.x.round(), bounds.y),
                        transform,
                        alpha: paint.opacity(),
                        clip,
                        atlas_evicted: fonts.atlas_stats().evicted,
                        resident_glyphs,
                    })
                }
                _ => None,
            };
            if let (Some(key), Some(shapes)) = (&cache_key, &immediate_cache.shapes)
                && immediate_cache.key.as_ref() == Some(key)
            {
                let shapes = shapes.clone();
                if clip_text {
                    crate::paint::with_clip(bounds, || shapes.draw());
                } else {
                    shapes.draw();
                }
                return;
            }

            let rejected_before = fonts.atlas_stats().rejected_exhausted;
            let mut draw = || {
                let shapes = paint.capture(|p| {
                    for run in buffer.layout_runs().take(max_lines.unwrap_or(usize::MAX)) {
                        // The atlas is rasterized for a pixel grid. Snap the
                        // same baseline in immediate and retained rendering.
                        let baseline = LineBaseline {
                            offset: y + run.line_i as f32 * spacing,
                            layout_y: run.line_y,
                        };
                        let pen = vec2(bounds.x.round(), baseline.snap(bounds.y));
                        if strike {
                            let left = run
                                .glyphs
                                .iter()
                                .map(|g| g.x)
                                .reduce(f32::min)
                                .unwrap_or(0.);
                            let rule = Rect::new(
                                pen.x + left,
                                pen.y - font_size * 0.3,
                                run.line_w,
                                (font_size / 16.).max(1.),
                            );
                            if text_rect_visible(clip, transform, rule) {
                                p.fill_rect(rule, color_brush.shader(bounds));
                            }
                        }
                        if underline {
                            let left = run
                                .glyphs
                                .iter()
                                .map(|glyph| glyph.x)
                                .reduce(f32::min)
                                .unwrap_or(0.);
                            let underline = Rect::new(
                                pen.x + left,
                                pen.y + 2.,
                                run.line_w,
                                (font_size / 16.).max(1.),
                            );
                            if text_rect_visible(clip, transform, underline) {
                                p.fill_rect(underline, color_brush.shader(bounds));
                            }
                        }
                        for glyph in run.glyphs.iter() {
                            let physical_glyph = glyph.physical((0., 0.), 1.0);
                            let atlas = fonts
                                .font_atlas
                                .get_or_insert_with(|| Atlas::new(device_page_size()));
                            let Some(cached) = atlas.cache_glyph(
                                &mut fonts.font_system,
                                &mut fonts.swash_cache,
                                physical_glyph.cache_key,
                            ) else {
                                continue;
                            };
                            let placement = cached.placement;
                            let draw_rect = Rect::new(
                                (physical_glyph.x + placement.left) as f32 + pen.x,
                                (physical_glyph.y - placement.top) as f32 + pen.y,
                                placement.width as f32,
                                placement.height as f32,
                            );
                            if !text_rect_visible(clip, transform, draw_rect) {
                                continue;
                            }
                            let glyph_brush = glyph.color_opt.map(|color| {
                                Brush::Solid(Color::from_rgba(
                                    color.r(),
                                    color.g(),
                                    color.b(),
                                    color.a(),
                                ))
                            });
                            p.draw_texture(
                                draw_rect,
                                cached.texture,
                                cached.uv,
                                glyph_brush.as_ref().unwrap_or(&color_brush).shader(bounds),
                            );
                        }
                    }
                    // Upload staged glyphs before the geometry is submitted.
                    if let Some(atlas) = &mut fonts.font_atlas {
                        atlas.end_pass();
                    }
                });
                shapes.draw();
                shapes
            };
            let shapes = if clip_text {
                crate::paint::with_clip(bounds, draw)
            } else {
                draw()
            };
            // Keep the geometry only when every glyph made it into the atlas.
            let complete = fonts.atlas_stats().rejected_exhausted == rejected_before;
            match cache_key {
                Some(mut key) if complete => {
                    key.atlas_evicted = fonts.atlas_stats().evicted;
                    key.resident_glyphs = pin_glyphs(&immediate_cache.glyph_keys, fonts);
                    immediate_cache.key = Some(key);
                    immediate_cache.shapes = Some(shapes);
                }
                _ => immediate_cache = ImmediateTextCache::default(),
            }
        });
    });
}

/// Optional span attributes inherit the document node's defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct SpanStyle {
    pub family: Option<FamilyOwned>,
    pub weight: Option<Weight>,
    pub italic: bool,
    /// Relative to the node's font size and line height.
    pub scale: f32,
    pub color: Option<cosmic_text::Color>,
}

impl Default for SpanStyle {
    fn default() -> Self {
        Self {
            family: None,
            weight: None,
            italic: false,
            scale: 1.,
            color: None,
        }
    }
}

impl SpanStyle {
    pub fn attrs<'a>(&'a self, defaults: &Attrs<'a>, metrics: Metrics) -> Attrs<'a> {
        let mut attrs = defaults.clone();
        if let Some(family) = &self.family {
            attrs = attrs.family(family.as_family());
        }
        if let Some(weight) = self.weight {
            attrs = attrs.weight(weight);
        }
        if self.italic {
            attrs = attrs.style(FontStyle::Italic);
        }
        if let Some(color) = self.color {
            attrs = attrs.color(color);
        }
        if self.scale > 0. && self.scale != 1. {
            attrs = attrs.metrics(metrics.scale(self.scale));
        }
        attrs
    }
}

/// A text run with optional font and color overrides.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: SpanStyle::default(),
        }
    }
    pub fn bold(mut self) -> Self {
        self.style.weight = Some(Weight::BOLD);
        self
    }
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }
    pub fn monospace(mut self) -> Self {
        self.style.family = Some(FamilyOwned::Monospace);
        self
    }
    pub fn color(mut self, color: cosmic_text::Color) -> Self {
        self.style.color = Some(color);
        self
    }
}
