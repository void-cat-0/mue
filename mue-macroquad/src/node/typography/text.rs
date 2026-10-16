use super::document::Document;
use std::{cell::RefCell, rc::Rc};

use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, Metrics, Placement, Shaping, SwashCache, Wrap,
};
use guillotiere::{
    AllocId, Allocation, AtlasAllocator,
    euclid::{Box2D, Size2D, UnknownUnit},
    point2, size2,
};
use lru::LruCache;
use macroquad::texture::{Image, Texture2D};
use mue_core::{
    effect::{computed, watch_effect},
    prop::Prop,
    signal::Access,
};
use taffy::{AvailableSpace, Size};

use crate::{
    layout::use_layout, math::Rect, paint::use_background, paint::use_paint, runtime::Runtime,
    shader::SharedTexture, style::Style,
};

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
    /// Register the faces contained in `data` with the shared font system.
    ///
    /// Returns the family name of every face that was added, de-duplicated and in file
    /// order. The result is empty when the data contains no usable font.
    pub(crate) fn load_font_data(&mut self, data: Vec<u8>) -> Vec<String> {
        let existing: Vec<_> = self.font_system.db().faces().map(|face| face.id).collect();
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

impl Default for FontState {
    fn default() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            font_atlas: None,
        }
    }
}

/// For weird rect like 1x0
enum CAllocation {
    Real(Allocation),
    Fake,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CAllocId {
    Real(AllocId),
    Fake,
}

impl CAllocation {
    pub fn rect(&self) -> Box2D<i32, UnknownUnit> {
        match self {
            CAllocation::Real(alloc) => alloc.rectangle,
            CAllocation::Fake => Box2D {
                min: point2(0, 0),
                max: point2(0, 0),
            },
        }
    }

    pub fn id(&self) -> CAllocId {
        match self {
            CAllocation::Real(alloc) => CAllocId::Real(alloc.id),
            CAllocation::Fake => CAllocId::Fake,
        }
    }
}

fn alloc_or_evict(
    allocator: &mut AtlasAllocator,
    cache: &mut LruCache<CacheKey, (CAllocId, Placement)>,
    size: Size2D<i32, UnknownUnit>,
) -> CAllocation {
    if size.width <= 0 || size.height <= 0 {
        return CAllocation::Fake;
    }
    for _ in 0..Atlas::MAX_ALLOC_ATTEMPTS {
        if let Some(alloc) = allocator.allocate(size) {
            return CAllocation::Real(alloc);
        }
        if let Some((_, (CAllocId::Real(id), _))) = cache.pop_lru() {
            allocator.deallocate(id);
        }
    }
    // TODO: handle this better
    eprintln!("Current cache size: {}", cache.len());
    panic!(
        "Failed to allocate space of {}x{} in the atlas after {} attempts, maybe the atlas is too small?",
        size.width,
        size.height,
        Atlas::MAX_ALLOC_ATTEMPTS
    );
}

struct Atlas {
    allocator: AtlasAllocator,
    pub texture: SharedTexture,
    cache: LruCache<CacheKey, (CAllocId, Placement)>,
}

impl Atlas {
    pub fn new(max_length: u32) -> Self {
        let mut length: i32 = 0;
        unsafe {
            use miniquad::gl;
            gl::glGetIntegerv(gl::GL_MAX_TEXTURE_SIZE, &mut length);
        }
        let length = length.min(max_length as i32);
        let size: Size2D<i32, UnknownUnit> = size2(length, length);
        let texture = Texture2D::from_image(&Image {
            bytes: vec![0; length as usize * length as usize * 4],
            width: length as u16,
            height: length as u16,
        });
        Self {
            allocator: AtlasAllocator::new(size),
            texture: texture.into(),
            cache: LruCache::unbounded(),
        }
    }
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new(i32::MAX as u32)
    }
}

impl Atlas {
    const MAX_ALLOC_ATTEMPTS: usize = 32;
    const ALLOC_GAP: i32 = 1;

    pub fn cache_glyph(
        &mut self,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        key: CacheKey,
    ) -> Option<CAllocId> {
        if let Some((alloc_id, _)) = self.cache.get(&key) {
            return Some(*alloc_id);
        }

        let image = cache.get_image_uncached(font_system, key)?;

        let cosmic_text::Placement {
            left,
            top,
            width,
            height,
        } = image.placement;

        let alloc = alloc_or_evict(
            &mut self.allocator,
            &mut self.cache,
            size2(
                width as i32 + 2 * Self::ALLOC_GAP,
                height as i32 + 2 * Self::ALLOC_GAP,
            ),
        );

        let data = match image.content {
            cosmic_text::SwashContent::Mask => image
                .data
                .iter()
                .flat_map(|a| [255, 255, 255, *a])
                .collect(),
            cosmic_text::SwashContent::Color => image.data,
            cosmic_text::SwashContent::SubpixelMask => {
                todo!()
            }
        };
        let quad_image = Image {
            bytes: data,
            width: width as u16,
            height: height as u16,
        };
        self.texture.update_part(
            &quad_image,
            alloc.rect().min.x + Self::ALLOC_GAP,
            alloc.rect().min.y + Self::ALLOC_GAP,
            alloc.rect().width() - 2 * Self::ALLOC_GAP,
            alloc.rect().height() - 2 * Self::ALLOC_GAP,
        );

        self.cache.push(
            key,
            (
                alloc.id(),
                Placement {
                    left,
                    top,
                    width,
                    height,
                },
            ),
        );

        Some(alloc.id())
    }

    pub fn get_glyph(&mut self, key: CacheKey) -> Option<Rect> {
        self.cache.get(&key).map(|(alloc_id, _)| {
            if let CAllocId::Real(alloc_id) = alloc_id {
                let box2d = self.allocator[*alloc_id].to_f32();
                Rect::new(
                    box2d.min.x + Self::ALLOC_GAP as f32,
                    box2d.min.y + Self::ALLOC_GAP as f32,
                    box2d.width() - 2.0 * Self::ALLOC_GAP as f32,
                    box2d.height() - 2.0 * Self::ALLOC_GAP as f32,
                )
            } else {
                Rect::default()
            }
        })
    }

    pub fn get_placement(&mut self, key: CacheKey) -> Option<Placement> {
        self.cache.get(&key).map(|(_, placement)| *placement)
    }
}

/// Display wrapping text with reactive content, font, alignment, and paint styles.
/// Load custom fonts before building the node. Height follows the shaped lines.
#[mue_macros::node]
pub fn text(style: &mut Style, text: Rc<str>) {
    render_text(style, text.map(|text| Document::plain(text.to_string())));
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

pub(super) fn render_text(style: &mut Style, content: Prop<Document>) {
    let layout = use_layout(style);
    let rect = layout.rect;
    let layout_id = layout.layout_id;

    let paint = use_paint(style);
    use_background(style, rect);
    let declared_font_size = style.font_size;
    let font_size = crate::node::use_font_size();
    crate::node::NodeInner::with_mut(|node| {
        if node.font_size.is_none() {
            node.font_size = declared_font_size;
        }
    });
    let line_height = crate::style::units::resolve_length(style.take_line_height(), "line_height");
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
    let color = style.take_color();

    let metrics =
        computed(move |_| Metrics::new(font_size.get().max(1.), line_height.get().max(1.)));

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
                        .spans
                        .iter()
                        .map(|span| (span.text.as_str(), span.style.attrs(&attrs, metrics))),
                    &attrs,
                    Shaping::Advanced,
                    align.get(),
                );
                buffer.set_metrics_and_size(metrics, None, None);
                let _ = spacing.get();
                layout.mark_dirty();
            });
        }
    });

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
            // Visible overflow must not hide lines after a size or font animation.
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
            let mut draw = || {
                paint.draw(|p| {
                    for run in buffer
                        .layout_runs()
                        .take(max_lines.get().unwrap_or(usize::MAX))
                    {
                        let origin = bounds.min()
                            + crate::math::vec2(0., y + run.line_i as f32 * spacing.get().max(0.));
                        if strike.get() {
                            let left = run
                                .glyphs
                                .iter()
                                .map(|g| g.x)
                                .reduce(f32::min)
                                .unwrap_or(0.);
                            p.fill_rect(
                                Rect::new(
                                    origin.x + left,
                                    origin.y + run.line_y - font_size.get() * 0.3,
                                    run.line_w,
                                    (font_size.get() / 16.).max(1.),
                                ),
                                color.get_clone().shader(bounds),
                            );
                        }
                        if text_underline.get() {
                            let left = run
                                .glyphs
                                .iter()
                                .map(|glyph| glyph.x)
                                .reduce(f32::min)
                                .unwrap_or(0.);
                            let underline = Rect::new(
                                origin.x + left,
                                origin.y + run.line_y + 2.,
                                run.line_w,
                                (font_size.get() / 16.).max(1.),
                            );
                            p.fill_rect(underline, color.get_clone().shader(bounds));
                        }
                        for glyph in run.glyphs.iter() {
                            let physical_glyph = glyph.physical((0., 0.), 1.0);
                            let atlas = fonts.font_atlas.get_or_insert_with(|| Atlas::new(4096));
                            if atlas
                                .cache_glyph(
                                    &mut fonts.font_system,
                                    &mut fonts.swash_cache,
                                    physical_glyph.cache_key,
                                )
                                .is_none()
                            {
                                continue;
                            }
                            let rect = atlas.get_glyph(physical_glyph.cache_key).unwrap();
                            let placement = atlas.get_placement(physical_glyph.cache_key).unwrap();
                            let draw_rect = Rect::new(
                                (physical_glyph.x + placement.left) as f32 + origin.x,
                                ((physical_glyph.y - placement.top) as f32 + run.line_y) + origin.y,
                                placement.width as f32,
                                placement.height as f32,
                            );
                            let uv_rect = {
                                let width = atlas.texture.width();
                                let height = atlas.texture.height();
                                Rect::new(
                                    rect.x / width,
                                    rect.y / height,
                                    rect.w / width,
                                    rect.h / height,
                                )
                            };
                            p.draw_texture(
                                draw_rect,
                                atlas.texture.clone(),
                                uv_rect,
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
