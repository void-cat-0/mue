use std::collections::HashMap;

use cosmic_text::{CacheKey, FontSystem, Placement, SwashCache, SwashContent};
use guillotiere::euclid::size2;
use guillotiere::{AllocId, AtlasAllocator};
use lru::LruCache;
use macroquad::texture::{FilterMode, Image, Texture2D};

use crate::{math::Rect, shader::SharedTexture};

const PAGE_SIZE: i32 = 1024;
const MAX_PAGES: usize = 8;
const PADDING: i32 = 1;

type Key = CacheKey;

/// The result of rasterizing and placing one glyph in an atlas page.
///
/// `uv` is normalized to the returned `texture`; the one-pixel border is not
/// included in the UV rectangle. `placement` remains the original Swash
/// placement, so this type does not change glyph metrics or raster size.
pub(super) struct CachedGlyph {
    pub(super) texture: SharedTexture,
    pub(super) uv: Rect,
    pub(super) placement: Placement,
}

struct Page {
    texture: Option<SharedTexture>,
    size: i32,
}

struct Entry {
    uv: Rect,
    placement: Placement,
}

/// Allocation returned by the shared pure atlas book.
#[derive(Clone, Copy)]
struct BookSlot {
    page: usize,
    allocation: AllocId,
}

struct BookPage {
    allocator: AtlasAllocator,
}

struct BookEntry {
    slot: BookSlot,
    pinned: bool,
    last_used: u64,
}

struct AllocationResult<K> {
    slot: Option<BookSlot>,
    evicted: Vec<K>,
}

/// Pure allocation/cache policy shared by the GPU atlas and its CPU tests.
/// Unpinned entries are kept in an LRU queue for O(1) victim selection.
struct AllocationBook<K> {
    side: i32,
    max_pages: usize,
    pages: Vec<BookPage>,
    entries: HashMap<K, BookEntry>,
    unpinned: LruCache<K, ()>,
    tick: u64,
}

impl<K> AllocationBook<K>
where
    K: Copy + Eq + std::hash::Hash,
{
    fn new(side: i32, max_pages: usize) -> Self {
        Self {
            side,
            max_pages,
            pages: Vec::new(),
            entries: HashMap::new(),
            unpinned: LruCache::unbounded(),
            tick: 0,
        }
    }

    fn begin_pass(&mut self) {
        // Preserve the previous access order when making all currently pinned
        // entries evictable. Sorting is once per pass; eviction itself remains
        // O(1) per entry during a churn-heavy pass.
        let mut pinned: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(key, entry)| entry.pinned.then_some((*key, entry.last_used)))
            .collect();
        pinned.sort_unstable_by_key(|(_, last_used)| *last_used);
        for (key, _) in pinned {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.pinned = false;
                self.unpinned.push(key, ());
            }
        }
    }

    fn touch(&mut self, key: &K) -> Option<BookSlot> {
        let slot = self.entries.get(key)?.slot;
        let was_pinned = self.entries.get(key).is_some_and(|entry| entry.pinned);
        if !was_pinned {
            self.unpinned.pop(key);
        }
        self.tick = self.tick.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.pinned = true;
            entry.last_used = self.tick;
        }
        Some(slot)
    }

    fn allocate(&mut self, key: K, width: i32, height: i32) -> AllocationResult<K> {
        if width <= 0
            || height <= 0
            || width > self.side
            || height > self.side
            || self.entries.contains_key(&key)
        {
            return AllocationResult {
                slot: None,
                evicted: Vec::new(),
            };
        }
        self.tick = self.tick.wrapping_add(1);
        let requested = size2(width, height);
        let mut evicted = Vec::new();

        loop {
            if let Some(allocation) =
                self.pages
                    .iter_mut()
                    .enumerate()
                    .find_map(|(page, page_data)| {
                        page_data
                            .allocator
                            .allocate(requested)
                            .map(|allocation| BookSlot {
                                page,
                                allocation: allocation.id,
                            })
                    })
            {
                self.insert(key, allocation);
                return AllocationResult {
                    slot: Some(allocation),
                    evicted,
                };
            }

            // Evict progressively, retrying after every individual deallocation
            // so fragmented free space is reclaimed without a retry limit.
            if let Some((victim, _)) = self.unpinned.pop_lru() {
                if let Some(entry) = self.entries.remove(&victim) {
                    self.pages[entry.slot.page]
                        .allocator
                        .deallocate(entry.slot.allocation);
                    evicted.push(victim);
                }
                continue;
            }

            // All existing entries are pinned. Grow lazily only at this point.
            if self.pages.len() < self.max_pages {
                let page = self.pages.len();
                self.pages.push(BookPage {
                    allocator: AtlasAllocator::new(size2(self.side, self.side)),
                });
                if let Some(allocation) = self.pages[page].allocator.allocate(requested) {
                    let slot = BookSlot {
                        page,
                        allocation: allocation.id,
                    };
                    self.insert(key, slot);
                    return AllocationResult {
                        slot: Some(slot),
                        evicted,
                    };
                }
            }
            return AllocationResult {
                slot: None,
                evicted,
            };
        }
    }

    fn insert(&mut self, key: K, slot: BookSlot) {
        self.entries.insert(
            key,
            BookEntry {
                slot,
                pinned: true,
                last_used: self.tick,
            },
        );
    }

    fn page_rect(
        &self,
        slot: BookSlot,
    ) -> guillotiere::euclid::Box2D<i32, guillotiere::euclid::UnknownUnit> {
        self.pages[slot.page].allocator[slot.allocation]
    }
}

/// A bounded, paged glyph atlas.
///
/// Pages and their GPU textures are created only when the first glyph needs
/// them. Allocations are never moved. A glyph is pinned when returned, and is
/// eligible for eviction only after `begin_pass` has cleared all pins.
pub(super) struct Atlas {
    book: Option<AllocationBook<Key>>,
    pages: Vec<Page>,
    entries: HashMap<Key, Entry>,
    page_size: Option<i32>,
    warned_oversized: bool,
    warned_exhausted: bool,
}

impl Atlas {
    pub(super) fn new() -> Self {
        Self {
            book: None,
            pages: Vec::new(),
            entries: HashMap::new(),
            page_size: None,
            warned_oversized: false,
            warned_exhausted: false,
        }
    }

    /// Starts a new text rendering pass. The caller must have flushed queued
    /// geometry before calling this, because unpinned slots may be reused.
    pub(super) fn begin_pass(&mut self) {
        if let Some(book) = &mut self.book {
            book.begin_pass();
        }
    }

    pub(super) fn cache_glyph(
        &mut self,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        key: CacheKey,
    ) -> Option<CachedGlyph> {
        if let Some(book) = &mut self.book
            && let Some(slot) = book.touch(&key)
        {
            let entry = self.entries.get(&key)?;
            let texture = self.pages[slot.page].texture.as_ref()?.clone();
            return Some(CachedGlyph {
                texture,
                uv: entry.uv,
                placement: entry.placement,
            });
        }

        // Deliberately bypass SwashCache's image cache: the atlas owns the
        // lifetime of this raster and cache churn must not grow unboundedly.
        let image = cache.get_image_uncached(font_system, key)?;
        let placement = image.placement;
        let width = i32::try_from(placement.width).ok()?;
        let height = i32::try_from(placement.height).ok()?;
        if width <= 0 || height <= 0 {
            return None;
        }
        let padded_width = width.checked_add(PADDING * 2)?;
        let padded_height = height.checked_add(PADDING * 2)?;
        let page_size = self.device_page_size();
        if padded_width > page_size || padded_height > page_size {
            self.warn_oversized(padded_width, padded_height, page_size);
            return None;
        }

        let rgba = raster_to_rgba(image.content, &image.data, width, height)?;
        if self.book.is_none() {
            self.book = Some(AllocationBook::new(page_size, MAX_PAGES));
        }
        let result = self
            .book
            .as_mut()
            .expect("allocation book was initialized")
            .allocate(key, padded_width, padded_height);
        for evicted in result.evicted {
            self.entries.remove(&evicted);
        }
        let slot = match result.slot {
            Some(slot) => slot,
            None => {
                self.warn_exhausted((padded_width, padded_height));
                return None;
            }
        };
        while self.pages.len() < self.book.as_ref().expect("book exists").pages.len() {
            self.pages.push(Page {
                texture: None,
                size: page_size,
            });
        }

        let rect = self.book.as_ref().expect("book exists").page_rect(slot);
        let x = rect.min.x + PADDING;
        let y = rect.min.y + PADDING;
        let page_ref = &mut self.pages[slot.page];
        let texture = page_ref.texture.get_or_insert_with(|| {
            let side = page_ref.size as usize;
            let texture: SharedTexture = Texture2D::from_image(&Image {
                bytes: vec![0; side.saturating_mul(side).saturating_mul(4)],
                width: page_ref.size as u16,
                height: page_ref.size as u16,
            })
            .into();
            texture.set_filter(FilterMode::Linear);
            texture
        });

        // Upload the complete padded rectangle, not merely the glyph pixels.
        // This clears stale texels whenever a freed slot is reused.
        let upload = padded_rgba(&rgba, width as usize, height as usize);
        texture.update_part(
            &Image {
                bytes: upload,
                width: padded_width as u16,
                height: padded_height as u16,
            },
            rect.min.x,
            rect.min.y,
            padded_width,
            padded_height,
        );

        let uv = Rect::new(
            x as f32 / page_size as f32,
            y as f32 / page_size as f32,
            width as f32 / page_size as f32,
            height as f32 / page_size as f32,
        );
        self.entries.insert(key, Entry { uv, placement });
        Some(CachedGlyph {
            texture: texture.clone(),
            uv,
            placement,
        })
    }

    fn device_page_size(&mut self) -> i32 {
        if let Some(size) = self.page_size {
            return size;
        }
        let mut max_size = PAGE_SIZE;
        unsafe {
            let mut queried = 0;
            miniquad::gl::glGetIntegerv(miniquad::gl::GL_MAX_TEXTURE_SIZE, &mut queried);
            if queried > 0 {
                max_size = max_size.min(queried);
            }
        }
        self.page_size = Some(max_size);
        max_size
    }

    fn warn_oversized(&mut self, width: i32, height: i32, page: i32) {
        if !self.warned_oversized {
            self.warned_oversized = true;
            eprintln!(
                "mue glyph atlas: rejected non-empty glyph {}x{} (page size {})",
                width, height, page
            );
        }
    }

    fn warn_exhausted(&mut self, requested: (i32, i32)) {
        if !self.warned_exhausted {
            self.warned_exhausted = true;
            eprintln!(
                "mue glyph atlas: rejected non-empty glyph {}x{} (all atlas slots pinned)",
                requested.0, requested.1
            );
        }
    }
}

fn raster_to_rgba(content: SwashContent, data: &[u8], width: i32, height: i32) -> Option<Vec<u8>> {
    let pixels = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    match content {
        SwashContent::Mask => {
            if data.len() != pixels {
                return None;
            }
            let mut rgba = Vec::with_capacity(pixels * 4);
            for &alpha in data {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            Some(rgba)
        }
        SwashContent::Color => {
            if data.len() != pixels.checked_mul(4)? {
                return None;
            }
            Some(data.to_vec())
        }
        // Swash describes this as four bytes containing RGB coverage. Treat
        // the strongest channel as coverage and render it as white alpha;
        // this is deterministic and avoids panicking on an uncommon format.
        SwashContent::SubpixelMask => {
            if data.len() != pixels.checked_mul(4)? {
                return None;
            }
            let mut rgba = Vec::with_capacity(pixels * 4);
            for pixel in data.as_chunks::<4>().0 {
                let alpha = pixel[0].max(pixel[1]).max(pixel[2]);
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            Some(rgba)
        }
    }
}

fn padded_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let padded_width = width + PADDING as usize * 2;
    let padded_height = height + PADDING as usize * 2;
    let mut output = vec![0; padded_width * padded_height * 4];
    for row in 0..height {
        let source_start = row * width * 4;
        let destination_start = ((row + PADDING as usize) * padded_width + PADDING as usize) * 4;
        output[destination_start..destination_start + width * 4]
            .copy_from_slice(&rgba[source_start..source_start + width * 4]);
    }
    output
}
