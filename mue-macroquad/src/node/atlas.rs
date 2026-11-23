use std::collections::HashMap;

use cosmic_text::{CacheKey, FontSystem, Placement, SwashCache, SwashContent, SwashImage};
use guillotiere::euclid::{Size2D, UnknownUnit, size2};
use guillotiere::{AllocId, AtlasAllocator};
use macroquad::texture::{FilterMode, Texture2D};
use miniquad::TextureUpdateRegion;
use smallvec::SmallVec;

use crate::{math::Rect, shader::SharedTexture};

/// Preferred page side. FontState clamps this to the driver limit.
pub(crate) const DEFAULT_PAGE_SIZE: i32 = 1024;
const MAX_PAGES: usize = 8;
const PADDING: i32 = 1;

type Key = CacheKey;

/// Counters for atlas pressure, read through crate::font::glyph_atlas_stats.
///
/// Rejections used to be reported by a one-shot warning with no way to tell
/// how often they happened; every counter here is cumulative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphAtlasStats {
    /// Glyphs rasterized into a page since startup.
    pub rasterized: u64,
    /// Glyphs dropped to make room for a newer allocation.
    pub evicted: u64,
    /// Glyphs rejected because their padded raster cannot fit one page.
    pub rejected_oversized: u64,
    /// Glyphs rejected because every slot was already in use this pass.
    pub rejected_exhausted: u64,
    /// Glyphs rejected because swash returned unexpected raster data.
    pub rejected_raster: u64,
    /// Rectangles uploaded after coalescing. One backend batch may upload several.
    pub uploads: u64,
    /// Pixels sent to the GPU by those uploads.
    pub uploaded_pixels: u64,
    /// Atlas pages created so far.
    pub pages: usize,
}

/// The result of rasterizing and placing one glyph in an atlas page.
///
/// The uv rectangle is normalized to the returned texture and excludes the
/// one-pixel border. The placement stays the original Swash placement, so this
/// type does not change glyph metrics or raster size.
pub(super) struct CachedGlyph {
    pub(super) texture: SharedTexture,
    pub(super) uv: Rect,
    pub(super) placement: Placement,
}

#[derive(Clone, Copy)]
struct DirtyRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl DirtyRect {
    fn area(self) -> usize {
        self.w as usize * self.h as usize
    }

    fn append_pixels(self, page: &[u8], side: usize, bytes: &mut Vec<u8>) {
        let row_bytes = self.w as usize * 4;
        for row in self.y as usize..(self.y + self.h) as usize {
            let start = (row * side + self.x as usize) * 4;
            bytes.extend_from_slice(&page[start..start + row_bytes]);
        }
    }

    fn union(self, x: i32, y: i32, w: i32, h: i32) -> Self {
        let x1 = self.x.min(x);
        let y1 = self.y.min(y);
        let x2 = (self.x + self.w).max(x + w);
        let y2 = (self.y + self.h).max(y + h);
        Self {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }
}

struct Page {
    side: usize,
    allocator: AtlasAllocator,
    texture: SharedTexture,
    /// CPU mirror flushed at the end of a text batch. It keeps untouched
    /// texels correct inside a coalesced upload.
    staging: Vec<u8>,
    /// Rectangles written since the last flush.
    pending: Vec<DirtyRect>,
    /// Any allocation or cache hit pins the entire page against resets.
    last_used: u64,
    /// Payloads on this page when the pass's eviction queue was built.
    candidates: Vec<Key>,
}

impl Page {
    fn new(allocator: AtlasAllocator, last_used: u64) -> Self {
        let side = allocator.size().width as usize;
        let staging = vec![0; side * side * 4];
        let texture = Texture2D::from_rgba8(side as u16, side as u16, &staging);
        texture.set_filter(FilterMode::Linear);
        Self {
            side,
            allocator,
            texture: texture.into(),
            staging,
            pending: Vec::new(),
            last_used,
            candidates: Vec::new(),
        }
    }

    fn mark_dirty(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.pending.push(DirtyRect { x, y, w, h });
    }

    /// Uploads the dirty rectangles together. Nearby writes can share a
    /// bounding box, while sparse writes keep their exact rectangles.
    fn upload(&mut self, stats: &mut GlyphAtlasStats) {
        if self.pending.is_empty() {
            return;
        }
        let mut union = self.pending[0];
        let mut changed = 0;
        for rect in &self.pending {
            union = union.union(rect.x, rect.y, rect.w, rect.h);
            changed += rect.area();
        }
        // Dirty slots remain pinned and disjoint until this upload.
        let (rects, pixels) = if union.area() <= changed * 4 {
            (std::slice::from_ref(&union), union.area())
        } else {
            (self.pending.as_slice(), changed)
        };
        let mut bytes = Vec::with_capacity(pixels * 4);
        for rect in rects {
            rect.append_pixels(&self.staging, self.side, &mut bytes);
        }
        let mut remaining = bytes.as_slice();
        let regions: SmallVec<[_; 1]> = rects
            .iter()
            .map(|rect| {
                let (bytes, tail) = remaining.split_at(rect.area() * 4);
                remaining = tail;
                TextureUpdateRegion {
                    x: rect.x,
                    y: rect.y,
                    width: rect.w,
                    height: rect.h,
                    bytes,
                }
            })
            .collect();
        self.texture.update_regions(&regions);
        stats.uploads += regions.len() as u64;
        stats.uploaded_pixels += pixels as u64;
        self.pending.clear();
    }
}

/// A packed glyph and its location in the atlas.
struct Glyph {
    page: usize,
    allocation: AllocId,
    uv: Rect,
    placement: Placement,
    /// Render pass in which the glyph was last requested; older entries are
    /// eligible for eviction.
    last_used: u64,
}

impl Glyph {
    fn cached(&self, page: &Page) -> CachedGlyph {
        CachedGlyph {
            texture: page.texture.clone(),
            uv: self.uv,
            placement: self.placement,
        }
    }
}

/// A bounded, paged glyph atlas.
///
/// Pages and their GPU textures are created only when the first glyph needs
/// them, and allocations never move. A glyph is owned by the pass that
/// requested it and becomes evictable once a later pass begins; eviction is
/// lazy, so an atlas that never fills does no bookkeeping per frame.
pub(super) struct Atlas {
    page_size: i32,
    max_pages: usize,
    /// Monotonic pass counter. Entries whose last_used is older are evictable.
    pass: u64,
    pages: Vec<Page>,
    glyphs: HashMap<Key, Glyph>,
    /// Candidates are built lazily on the first allocation miss in a pass.
    /// The queue is intentionally left allocated between passes: invalidating
    /// it is just a generation change, not an O(n) clear/drop operation.
    candidates: Vec<(u64, Key)>,
    candidates_pass: Option<u64>,
    stats: GlyphAtlasStats,
}

impl Atlas {
    pub(super) fn new(page_size: i32) -> Self {
        Self {
            page_size: page_size.max(1),
            max_pages: MAX_PAGES,
            pass: 1,
            pages: Vec::new(),
            glyphs: HashMap::new(),
            candidates: Vec::new(),
            candidates_pass: None,
            stats: GlyphAtlasStats::default(),
        }
    }

    pub(super) fn stats(&self) -> GlyphAtlasStats {
        let mut stats = self.stats;
        stats.pages = self.pages.len();
        stats
    }

    /// Opens a new pass. Glyphs not requested in this pass become evictable.
    ///
    /// The caller must have flushed queued geometry before calling this,
    /// because a later allocation may overwrite pixels a queued draw reads.
    pub(super) fn begin_pass(&mut self) {
        self.pass = self.pass.wrapping_add(1);
        // Candidate keys are only meaningful for the pass in which they were
        // built. Leave the allocation in place and invalidate by generation;
        // the next miss rebuilds it once, without scanning on every frame.
        self.candidates_pass = None;
    }

    /// Uploads staged glyphs before their geometry is submitted. A later call
    /// is a no-op when nothing is staged.
    pub(super) fn end_pass(&mut self) {
        let (pages, stats) = (&mut self.pages, &mut self.stats);
        for page in pages {
            page.upload(stats);
        }
    }

    /// Pins the batch's glyphs for this pass and uploads staged pixels. The
    /// keys are distinct, so the return value counts resident glyphs.
    pub(super) fn pin_glyphs(
        &mut self,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        keys: &[CacheKey],
    ) -> usize {
        let resident = keys
            .iter()
            .filter(|&&key| self.cache_glyph(font_system, cache, key).is_some())
            .count();
        self.end_pass();
        resident
    }

    /// Looks up a glyph without pinning it or modifying the atlas.
    pub(super) fn get_glyph(&self, key: CacheKey) -> Option<CachedGlyph> {
        let glyph = self.glyphs.get(&key)?;
        Some(glyph.cached(&self.pages[glyph.page]))
    }

    pub(super) fn cache_glyph(
        &mut self,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        key: CacheKey,
    ) -> Option<CachedGlyph> {
        if let Some(glyph) = self.glyphs.get_mut(&key) {
            glyph.last_used = self.pass;
            let page = &mut self.pages[glyph.page];
            page.last_used = self.pass;
            return Some(glyph.cached(page));
        }

        // Deliberately bypass SwashCache's image cache: the atlas owns the
        // lifetime of this raster and cache churn must not grow unboundedly.
        let image = cache.get_image_uncached(font_system, key)?;
        let placement = image.placement;
        let (width, height) = (placement.width, placement.height);
        if width == 0 || height == 0 {
            // Whitespace and other empty glyphs have nothing to upload.
            return None;
        }
        let max_glyph_side = (self.page_size - 2 * PADDING).max(0) as u32;
        if width > max_glyph_side || height > max_glyph_side {
            self.stats.rejected_oversized += 1;
            if self.stats.rejected_oversized == 1 {
                eprintln!(
                    "mue glyph atlas: rejected glyph {width}x{height} plus padding (page size {})",
                    self.page_size
                );
            }
            return None;
        }
        // Dimensions now fit on a page, so the signed sizes and byte count fit too.
        let bytes_per_pixel = match image.content {
            SwashContent::Mask => 1,
            SwashContent::Color | SwashContent::SubpixelMask => 4,
        };
        if image.data.len() != width as usize * height as usize * bytes_per_pixel {
            self.stats.rejected_raster += 1;
            return None;
        }
        let padded_width = width as i32 + 2 * PADDING;
        let padded_height = height as i32 + 2 * PADDING;

        let Some((page, allocation)) = self.allocate(size2(padded_width, padded_height)) else {
            self.stats.rejected_exhausted += 1;
            if self.stats.rejected_exhausted == 1 {
                eprintln!(
                    "mue glyph atlas: rejected non-empty glyph {}x{} (all slots in use)",
                    padded_width, padded_height
                );
            }
            return None;
        };

        let page_data = &mut self.pages[page];
        let origin = page_data.allocator[allocation].min;
        blit_glyph(
            &mut page_data.staging,
            page_data.side,
            origin.x as usize,
            origin.y as usize,
            &image,
        );
        page_data.mark_dirty(origin.x, origin.y, padded_width, padded_height);

        let page_size = self.page_size as f32;
        let uv = Rect::new(
            (origin.x + PADDING) as f32 / page_size,
            (origin.y + PADDING) as f32 / page_size,
            width as f32 / page_size,
            height as f32 / page_size,
        );
        let glyph = Glyph {
            page,
            allocation,
            uv,
            placement,
            last_used: self.pass,
        };
        let cached = glyph.cached(page_data);
        self.stats.rasterized += 1;
        self.glyphs.insert(key, glyph);
        Some(cached)
    }

    fn allocate(&mut self, requested: Size2D<i32, UnknownUnit>) -> Option<(usize, AllocId)> {
        if let Some(slot) = self.allocate_in_existing_pages(requested) {
            return Some(slot);
        }

        self.build_candidates();
        // Do not seed a new pass into old fragmented holes on every page.
        // Repack into one wholly unpinned page before falling back to partial
        // eviction on pages whose current-pass survivors cannot move.
        if let Some(page) = self
            .pages
            .iter()
            .position(|page| page.last_used != self.pass)
        {
            for key in self.pages[page].candidates.drain(..) {
                if self.glyphs.remove(&key).is_some() {
                    self.stats.evicted += 1;
                }
            }
            let data = &mut self.pages[page];
            data.allocator.clear();
            // No current-pass allocations/hits means no current staged writes.
            data.pending.clear();
            let allocation = data.allocator.allocate(requested)?;
            data.last_used = self.pass;
            return Some((page, allocation.id));
        }
        while let Some((_, key)) = self.candidates.pop() {
            // A candidate can have been requested after the queue was built.
            // Re-check the live entry so a touched glyph is never evicted.
            let evictable = self
                .glyphs
                .get(&key)
                .is_some_and(|glyph| glyph.last_used < self.pass);
            if !evictable {
                continue;
            }
            if self.evict(&key) {
                self.stats.evicted += 1;
                // Retry after every individual deallocation so fragmented free
                // space is reclaimed without a retry limit.
                if let Some(slot) = self.allocate_in_existing_pages(requested) {
                    return Some(slot);
                }
            }
        }

        // All existing entries are in use this pass. Grow lazily only now.
        if self.pages.len() == self.max_pages {
            return None;
        }
        let mut allocator = AtlasAllocator::new(size2(self.page_size, self.page_size));
        let allocation = allocator.allocate(requested)?;
        let page = self.pages.len();
        self.pages.push(Page::new(allocator, self.pass));
        Some((page, allocation.id))
    }

    fn build_candidates(&mut self) {
        if self.candidates_pass == Some(self.pass) {
            return;
        }
        self.candidates.clear();
        for page in &mut self.pages {
            page.candidates.clear();
        }
        for (key, glyph) in &self.glyphs {
            if glyph.last_used == self.pass {
                self.pages[glyph.page].last_used = self.pass;
            } else if glyph.last_used < self.pass {
                self.candidates.push((glyph.last_used, *key));
                self.pages[glyph.page].candidates.push(*key);
            }
        }
        // Descending order makes Vec::pop return the oldest entry first.
        self.candidates.sort_unstable_by(|(a, _), (b, _)| b.cmp(a));
        self.candidates_pass = Some(self.pass);
    }

    fn allocate_in_existing_pages(
        &mut self,
        requested: Size2D<i32, UnknownUnit>,
    ) -> Option<(usize, AllocId)> {
        self.pages.iter_mut().enumerate().find_map(|(page, data)| {
            // Untouched pages are reclaimed wholesale on pressure instead of
            // pinning scattered survivors into last pass's fragmented layout.
            if data.last_used != self.pass {
                return None;
            }
            data.allocator
                .allocate(requested)
                .map(|allocation| (page, allocation.id))
        })
    }

    fn evict(&mut self, key: &Key) -> bool {
        let Some(glyph) = self.glyphs.remove(key) else {
            return false;
        };
        self.pages[glyph.page]
            .allocator
            .deallocate(glyph.allocation);
        true
    }
}

/// Writes one validated glyph raster into its padded slot in the CPU page mirror.
///
/// The one-pixel border is cleared first: a reused slot still holds the
/// previous glyph's texels, and the border must not inherit them.
fn blit_glyph(staging: &mut [u8], side: usize, left: usize, top: usize, raster: &SwashImage) {
    let w = raster.placement.width as usize;
    let h = raster.placement.height as usize;
    let stride = side * 4;
    let pad = PADDING as usize;
    let padded_width = w + 2 * pad;

    for row in 0..h + 2 * pad {
        let start = (top + row) * stride + left * 4;
        staging[start..start + padded_width * 4].fill(0);
    }

    let glyph_start = |row: usize| (top + pad + row) * stride + (left + pad) * 4;
    match raster.content {
        SwashContent::Mask => {
            for row in 0..h {
                let start = glyph_start(row);
                let source = &raster.data[row * w..(row + 1) * w];
                for (dest, &alpha) in staging[start..start + w * 4]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(source)
                {
                    *dest = [255, 255, 255, alpha];
                }
            }
        }
        SwashContent::Color => {
            for row in 0..h {
                let start = glyph_start(row);
                let source = row * w * 4;
                staging[start..start + w * 4].copy_from_slice(&raster.data[source..source + w * 4]);
            }
        }
        // Swash describes this as four bytes containing RGB coverage. Treat
        // the strongest channel as coverage and render it as white alpha;
        // this is deterministic and avoids panicking on an uncommon format.
        SwashContent::SubpixelMask => {
            for row in 0..h {
                let start = glyph_start(row);
                let row_data = &raster.data[row * w * 4..(row + 1) * w * 4];
                for (dest, pixel) in staging[start..start + w * 4]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(row_data.as_chunks::<4>().0)
                {
                    let alpha = pixel[0].max(pixel[1]).max(pixel[2]);
                    *dest = [255, 255, 255, alpha];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blit_places_the_glyph_inside_its_padded_slot() {
        let mut page = [42; 5 * 5 * 4];
        let glyph = SwashImage {
            content: SwashContent::Mask,
            placement: Placement {
                width: 2,
                height: 1,
                ..Default::default()
            },
            data: vec![63, 191],
            ..Default::default()
        };
        blit_glyph(&mut page, 5, 1, 1, &glyph);
        for (index, &pixel) in page.as_chunks::<4>().0.iter().enumerate() {
            let expected = match (index % 5, index / 5) {
                (2, 2) => [255, 255, 255, 63],
                (3, 2) => [255, 255, 255, 191],
                (1..=4, 1..=3) => [0; 4],
                _ => [42; 4],
            };
            assert_eq!(pixel, expected);
        }
    }
}
