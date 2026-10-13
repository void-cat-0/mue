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

pub struct FontState {
    font_system: FontSystem,
    swash_cache: SwashCache,
    font_atlas: Atlas,
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
            font_atlas: Atlas::new(4096),
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

fn measured_size(buffer: &Buffer) -> Size<f32> {
    Size {
        // An intrinsic width must not wrap its last glyph on the draw pass.
        width: buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0., f32::max)
            .ceil(),
        height: buffer
            .layout_runs()
            .last()
            .map_or(0., |run| run.line_top + run.line_height),
    }
}

pub(super) fn render_text(style: &mut Style, content: Prop<Document>) {
    let layout = use_layout(style);
    let rect = layout.rect;
    let layout_id = layout.layout_id;

    let paint = use_paint(style);
    use_background(style, rect);
    let font_size = style.take_font_size();
    let line_height = style.take_line_height();
    let font_family = style.take_font_family();
    let font_weight = style.take_font_weight();
    let color = style.take_color();

    let metrics = computed(move |_| Metrics::new(font_size.get(), line_height.get()));

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
            let measured = measured_size(&buffer);

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
                    .weight(font_weight.get());
                let mut buffer = buffer.borrow_with(&mut fonts.font_system);
                let content = content.get_clone();
                let metrics = metrics.get();
                buffer.set_wrap(Wrap::WordOrGlyph);
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
            paint.draw(|p| {
                let origin = bounds.min();
                for run in buffer.layout_runs() {
                    for glyph in run.glyphs.iter() {
                        let physical_glyph = glyph.physical((0., 0.), 1.0);
                        if fonts
                            .font_atlas
                            .cache_glyph(
                                &mut fonts.font_system,
                                &mut fonts.swash_cache,
                                physical_glyph.cache_key,
                            )
                            .is_none()
                        {
                            continue;
                        }
                        let rect = fonts
                            .font_atlas
                            .get_glyph(physical_glyph.cache_key)
                            .unwrap();
                        let placement = fonts
                            .font_atlas
                            .get_placement(physical_glyph.cache_key)
                            .unwrap();
                        let draw_rect = Rect::new(
                            (physical_glyph.x + placement.left) as f32 + origin.x,
                            ((physical_glyph.y - placement.top) as f32 + run.line_y) + origin.y,
                            placement.width as f32,
                            placement.height as f32,
                        );
                        let uv_rect = {
                            let width = fonts.font_atlas.texture.width();
                            let height = fonts.font_atlas.texture.height();
                            Rect::new(
                                rect.x / width,
                                rect.y / height,
                                rect.w / width,
                                rect.h / height,
                            )
                        };
                        p.draw_texture(
                            draw_rect,
                            fonts.font_atlas.texture.clone(),
                            uv_rect,
                            glyph
                                .color_opt
                                .map_or_else(
                                    || color.get_clone(),
                                    |color| {
                                        crate::shader::Brush::Solid(crate::color::Color::from_rgba(
                                            color.r(),
                                            color.g(),
                                            color.b(),
                                            color.a(),
                                        ))
                                    },
                                )
                                .shader(bounds),
                        );
                    }
                }
            });
        });
    });
}