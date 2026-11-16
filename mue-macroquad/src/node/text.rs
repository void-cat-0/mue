use super::{Node, NodeContext};
use crate::node::atlas::{Atlas, DEFAULT_PAGE_SIZE};
use std::{cell::RefCell, collections::HashSet, ops::Range, rc::Rc};

pub use super::atlas::GlyphAtlasStats;

use cosmic_text::{
    Attrs, Buffer, FamilyOwned, FontSystem, Metrics, Shaping, Style as FontStyle, SwashCache,
    Weight, Wrap,
};
use mue_core::{
    effect::{computed, watch_effect},
    prop::Prop,
    signal::{Access, signal},
};
use taffy::{AvailableSpace, Size};

use crate::{
    color::Color,
    layout::use_layout,
    math::{Matrix, Rect, vec2},
    paint::{
        current_clip_logical, current_transform, rect_visible, retained_enabled, transform_rect,
        use_background, use_paint,
    },
    runtime::Runtime,
    shader::Brush,
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
struct LineRange {
    /// Tight bounds of the quads this line contributed, in text-local space.
    bounds: Rect,
    first: u32,
    count: u32,
}

struct RetainedTextBatch {
    texture: TextureId,
    vertices: Vec<RetainedVertex>,
    indices: Vec<u32>,
    lines: Vec<LineRange>,
}

#[derive(Clone, PartialEq)]
struct RetainedTextContentKey {
    revision: u64,
    width: f32,
    height: f32,
    y: f32,
    max_lines: Option<usize>,
    strike: bool,
    underline: bool,
    color: Brush,
}

#[derive(PartialEq)]
struct RetainedTextKey {
    content: RetainedTextContentKey,
    atlas_rasterized: u64,
    atlas_evicted: u64,
    atlas_pages: usize,
}

#[derive(Default)]
struct RetainedTextCache {
    key: Option<RetainedTextKey>,
    /// Glyph count for `key.content`; lets cache hits skip the glyph scan.
    glyph_count: usize,
    meshes: Vec<RetainedMesh>,
    line_ranges: Vec<Vec<LineRange>>,
    /// Union of every line's quad bounds, in text-local space.
    content_bounds: Option<Rect>,
    disabled: bool,
}

/// Contiguous index range covering the lines that intersect `clip`.
///
/// Lines are recorded in layout order, so the range runs from the first
/// visible line to the last one. Lines between them are kept even when they
/// are clipped, which keeps the draw to one index range per mesh; the scissor
/// still removes their pixels.
fn visible_line_range(
    lines: &[LineRange],
    clip: Option<Rect>,
    model: Matrix,
) -> Option<Range<u32>> {
    let first = lines.first()?;
    let last = lines.last()?;
    let Some(clip) = clip else {
        return Some(first.first..last.first + last.count);
    };

    let mut visible_first = None;
    let mut visible_last = None;
    for line in lines {
        let screen = transform_rect(model, line.bounds);
        if rect_visible(Some(clip), &screen) {
            visible_first.get_or_insert(line.first);
            visible_last = Some(line.first + line.count);
        }
    }

    match (visible_first, visible_last) {
        (Some(first), Some(last)) if last > first => Some(first..last),
        _ => None,
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

/// Keep every glyph used by a retained mesh resident in the atlas.
fn pin_glyphs(buffer: &Buffer, fonts: &mut FontState, max_lines: Option<usize>) {
    for run in buffer.layout_runs().take(max_lines.unwrap_or(usize::MAX)) {
        for glyph in run.glyphs.iter() {
            let physical_glyph = glyph.physical((0., 0.), 1.0);
            let atlas = fonts
                .font_atlas
                .get_or_insert_with(|| Atlas::new(device_page_size()));
            let _ = atlas.cache_glyph(
                &mut fonts.font_system,
                &mut fonts.swash_cache,
                physical_glyph.cache_key,
            );
        }
    }
    if let Some(atlas) = &mut fonts.font_atlas {
        atlas.end_pass();
    }
}

struct RetainedTextStyle<'a> {
    bounds: Rect,
    y: f32,
    spacing: f32,
    font_size: f32,
    max_lines: Option<usize>,
    strike: bool,
    underline: bool,
    color: &'a Brush,
}

fn build_retained_text(
    buffer: &Buffer,
    fonts: &mut FontState,
    style: RetainedTextStyle<'_>,
) -> Vec<RetainedTextBatch> {
    let RetainedTextStyle {
        bounds,
        y,
        spacing,
        font_size,
        max_lines,
        strike,
        underline,
        color,
    } = style;
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
        let local_origin = vec2(0., y + run.line_i as f32 * spacing);
        let starts: Vec<usize> = batches.iter().map(|batch| batch.indices.len()).collect();
        let pen = vec2(0., (local_origin.y + run.line_y).round());
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
            let color = color.color_at(rect.center() + bounds.min(), bounds);
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
            let color = color.color_at(rect.center() + bounds.min(), bounds);
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
                || color.color_at(draw_rect.center() + bounds.min(), bounds),
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
                });
            }
        }
    }
    if let Some(atlas) = &mut fonts.font_atlas {
        atlas.end_pass();
    }
    batches
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
                    .letter_spacing(letter_spacing.get())
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

    let retained_cache = Rc::new(RefCell::new(RetainedTextCache::default()));
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
            let height = measured_size(&buffer, spacing.get().max(0.), max_lines.get()).height;
            let free = (bounds.h - height).max(0.);
            let y = match vertical_align.get() {
                TextVerticalAlign::Top => 0.,
                TextVerticalAlign::Center => free / 2.,
                TextVerticalAlign::Bottom => free,
            };
            let color_brush = color.get_clone();
            // Check the cheap policy gates before walking the glyph runs; on
            // backends without retained support this avoids the scan entirely.
            let retained_requested = retained_enabled() && !retained_cache.borrow().disabled;
            let content_key = retained_requested.then(|| RetainedTextContentKey {
                revision: revision.get_untracked(),
                width: bounds.w,
                height: bounds.h,
                y,
                max_lines: max_lines.get(),
                strike: strike.get(),
                underline: text_underline.get(),
                color: color_brush.clone(),
            });
            let retained_model = retained_requested.then(|| {
                current_transform()
                    * Matrix::from_translation(vec2(bounds.min().x.round(), bounds.min().y.round()))
            });
            let cache_matches_content = content_key.as_ref().is_some_and(|content_key| {
                retained_cache
                    .borrow()
                    .key
                    .as_ref()
                    .is_some_and(|key| &key.content == content_key)
            });
            // Block-level cull before pinning: off-screen retained blocks do not
            // touch the atlas or scan their glyphs at all. The content key stays
            // valid when only the atlas stats changed, so the cached bounds are
            // still usable until the block becomes visible again.
            if cache_matches_content
                && let Some(model) = retained_model
                && let Some(content) = retained_cache.borrow().content_bounds
            {
                let screen = transform_rect(model, content);
                if !rect_visible(current_clip_logical(), &screen) {
                    return;
                }
            }
            let glyph_count = if !retained_requested {
                0
            } else if cache_matches_content {
                retained_cache.borrow().glyph_count
            } else {
                buffer
                    .layout_runs()
                    .take(max_lines.get().unwrap_or(usize::MAX))
                    .map(|run| run.glyphs.len())
                    .sum::<usize>()
            };
            let retained_eligible = retained_requested && glyph_count >= RETAINED_TEXT_MIN_GLYPHS;
            if retained_eligible {
                let content_key = content_key.expect("retained text has a content key");
                let mut cache = retained_cache.borrow_mut();
                // Keep the atlas entries referenced by the retained buffers alive
                // even on cache hits. Off-screen blocks were culled above, so this
                // only pins the blocks that can actually be seen.
                pin_glyphs(&buffer, fonts, max_lines.get());
                let stats = fonts
                    .font_atlas
                    .as_ref()
                    .map(Atlas::stats)
                    .unwrap_or_default();
                let key = RetainedTextKey {
                    content: content_key,
                    atlas_rasterized: stats.rasterized,
                    atlas_evicted: stats.evicted,
                    atlas_pages: stats.pages,
                };
                if cache.key.as_ref() != Some(&key) {
                    let batches = build_retained_text(
                        &buffer,
                        fonts,
                        RetainedTextStyle {
                            bounds,
                            y,
                            spacing: spacing.get().max(0.),
                            font_size: font_size.get(),
                            max_lines: max_lines.get(),
                            strike: strike.get(),
                            underline: text_underline.get(),
                            color: &color_brush,
                        },
                    );
                    let mut meshes = Vec::new();
                    let mut line_ranges = Vec::new();
                    for batch in batches {
                        if let Some(mesh) =
                            RetainedMesh::new(&batch.vertices, &batch.indices, batch.texture)
                        {
                            meshes.push(mesh);
                            line_ranges.push(batch.lines);
                        }
                    }
                    let stats = fonts
                        .font_atlas
                        .as_ref()
                        .map(Atlas::stats)
                        .unwrap_or_default();
                    let key = RetainedTextKey {
                        content: key.content.clone(),
                        atlas_rasterized: stats.rasterized,
                        atlas_evicted: stats.evicted,
                        atlas_pages: stats.pages,
                    };
                    if meshes.is_empty() {
                        cache.disabled = true;
                        cache.meshes.clear();
                        cache.line_ranges.clear();
                        cache.content_bounds = None;
                        cache.key = None;
                        cache.glyph_count = 0;
                    } else {
                        cache.content_bounds = line_ranges
                            .iter()
                            .flatten()
                            .map(|line| line.bounds)
                            .reduce(|bounds, line| bounds.union(&line));
                        cache.meshes = meshes;
                        cache.line_ranges = line_ranges;
                        cache.key = Some(key);
                        cache.glyph_count = glyph_count;
                    }
                }
                if !cache.meshes.is_empty() {
                    let model = retained_model.unwrap_or_else(|| {
                        current_transform()
                            * Matrix::from_translation(vec2(
                                bounds.min().x.round(),
                                bounds.min().y.round(),
                            ))
                    });
                    let draw_retained = || {
                        let clip = current_clip_logical();
                        let ranges: Vec<Option<Range<u32>>> = cache
                            .line_ranges
                            .iter()
                            .map(|lines| visible_line_range(lines, clip, model))
                            .collect();
                        if ranges.iter().all(Option::is_none) {
                            return;
                        }
                        crate::clip::draw_retained_meshes_ranges(
                            &cache.meshes,
                            &ranges,
                            crate::math::model_matrix(model),
                            paint.opacity(),
                        );
                    };
                    if max_lines.get().is_some() || overflow.get() != TextOverflow::Visible {
                        crate::paint::with_clip(bounds, draw_retained);
                    } else {
                        draw_retained();
                    }
                    return;
                }
                drop(cache);
            }

            let mut draw = || {
                let clip = current_clip_logical();
                let transform = current_transform();
                paint.draw(|p| {
                    for run in buffer
                        .layout_runs()
                        .take(max_lines.get().unwrap_or(usize::MAX))
                    {
                        let origin = bounds.min()
                            + crate::math::vec2(0., y + run.line_i as f32 * spacing.get().max(0.));
                        // Snap the glyph pen to whole pixels. Fractional layout
                        // positions would otherwise resample the 1:1 raster through
                        // the linear filter and soften every stem; cosmic-text keeps
                        // the subpixel offset inside the cache key, so nothing is lost.
                        let pen =
                            crate::math::vec2(origin.x.round(), (origin.y + run.line_y).round());
                        if strike.get() {
                            let left = run
                                .glyphs
                                .iter()
                                .map(|g| g.x)
                                .reduce(f32::min)
                                .unwrap_or(0.);
                            let rule = Rect::new(
                                pen.x + left,
                                pen.y - font_size.get() * 0.3,
                                run.line_w,
                                (font_size.get() / 16.).max(1.),
                            );
                            if rect_visible(clip, &transform_rect(transform, rule)) {
                                p.fill_rect(rule, color.get_clone().shader(bounds));
                            }
                        }
                        if text_underline.get() {
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
                                (font_size.get() / 16.).max(1.),
                            );
                            if rect_visible(clip, &transform_rect(transform, underline)) {
                                p.fill_rect(underline, color.get_clone().shader(bounds));
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
                            if !rect_visible(clip, &transform_rect(transform, draw_rect)) {
                                continue;
                            }
                            p.draw_texture(
                                draw_rect,
                                cached.texture,
                                cached.uv,
                                glyph
                                    .color_opt
                                    .map_or_else(
                                        || color.get_clone(),
                                        |color| {
                                            crate::shader::Brush::Solid(
                                                crate::color::Color::from_rgba(
                                                    color.r(),
                                                    color.g(),
                                                    color.b(),
                                                    color.a(),
                                                ),
                                            )
                                        },
                                    )
                                    .shader(bounds),
                            );
                        }
                    }
                    // Paint::draw submits geometry after this closure; later render hooks may flush it.
                    if let Some(atlas) = &mut fonts.font_atlas {
                        atlas.end_pass();
                    }
                })
            };
            if max_lines.get().is_some() || overflow.get() != TextOverflow::Visible {
                crate::paint::with_clip(bounds, draw);
            } else {
                draw();
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
