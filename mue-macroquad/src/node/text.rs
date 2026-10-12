use std::{cell::RefCell, rc::Rc};

use cosmic_text::{
    Attrs, Buffer, CacheKey, FamilyOwned, FontSystem, Metrics, Placement, Shaping, SwashCache,
    Weight,
};
use guillotiere::{
    euclid::{Box2D, Size2D, UnknownUnit},
    point2, size2, AllocId, Allocation, AtlasAllocator,
};
use lru::LruCache;
use macroquad::{
    color::WHITE,
    texture::{Image, Texture2D},
};
use mue_core::{
    effect::{computed, watch_effect},
    signal::Access,
};
use taffy::{AvailableSpace, Size};

use crate::{
    layout::use_layout, math::Rect, paint::use_paint, runtime::Runtime, shader::SharedTexture,
    style::Style,
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
            if let Some((name, _)) = face.families.first() {
                if !families.contains(name) {
                    families.push(name.clone());
                }
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
        // trace!(
        // "Failed to allocate space of {}x{} in the atlas, evicting one item",
        // size.width, size.height
        // );
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
        // let length = length as u32;
        // println!("Creating a new atlas with size: {}x{}", length, length);
        // let texture = Texture2D::new_empty(length, length);
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

/// Notes: Vairable fonts are not well suppported
#[mue_macros::node]
pub fn text(
    style: &mut Style,
    #[default(32.0)] font_size: f32,
    #[default(44.0)] line_height: f32,
    #[default(FamilyOwned::SansSerif)] font_family: FamilyOwned,
    #[default(Weight::NORMAL)] font_weight: Weight,
    text: Rc<str>,
) {
    let layout = use_layout(style);
    let rect = layout.rect;

    let paint = use_paint(style);

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
                AvailableSpace::MinContent => None,
            });
            let height_constraint = known_dimensions.height.or(match available_space.height {
                AvailableSpace::Definite(h) => Some(h),
                AvailableSpace::MaxContent => None,
                AvailableSpace::MinContent => None,
            });

            let mut buffer = buffer.borrow_mut();
            Runtime::with_fonts_mut(|fonts| {
                buffer.set_metrics_and_size(
                    &mut fonts.font_system,
                    metrics.get_untracked(),
                    width_constraint,
                    height_constraint,
                );
            });

            let total_height = buffer
                .layout_runs()
                .last()
                .map_or(0.0, |run| run.line_top + run.line_height);

            Size {
                width: known_dimensions.width.unwrap_or_else(|| {
                    buffer
                        .layout_runs()
                        .map(|run| run.line_w)
                        .fold(0.0, f32::max)
                }),
                height: known_dimensions.height.unwrap_or(total_height),
            }
        }
    });

    watch_effect({
        let buffer = buffer.clone();
        let align = style.take_text_align();
        move || {
            let mut buffer = buffer.borrow_mut();
            Runtime::with_fonts_mut(|fonts| {
                let rect = rect.get();
                let family = font_family.get_clone();
                let attrs = Attrs::new()
                    .family(family.as_family())
                    .weight(font_weight.get());
                let mut buffer = buffer.borrow_with(&mut fonts.font_system);
                buffer.set_text(&text.get_clone(), &attrs, Shaping::Advanced, align.get());
                buffer.set_metrics_and_size(metrics.get(), Some(rect.w), Some(rect.h));
                layout.mark_dirty();
            });
        }
    });

    style.on_render.append(move |_| {
        let buffer = buffer.borrow();
        Runtime::with_fonts_mut(|fonts| {
            paint.draw(|p| {
                let origin = rect.get().min();
                for run in buffer.layout_runs() {
                    for glyph in run.glyphs.iter() {
                        let physical_glyph = glyph.physical((0., 0.), 1.0);
                        // cache if needed
                        fonts.font_atlas.cache_glyph(
                            &mut fonts.font_system,
                            &mut fonts.swash_cache,
                            physical_glyph.cache_key,
                        );
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
                        p.draw_texture(draw_rect, fonts.font_atlas.texture.clone(), uv_rect, WHITE);
                    }
                }
            });
        });
    });
}
