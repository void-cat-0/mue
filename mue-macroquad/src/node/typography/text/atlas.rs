use std::collections::HashMap;

use cosmic_text::{CacheKey, FontSystem, Placement, SwashCache, SwashContent};
use guillotiere::euclid::{Size2D, UnknownUnit, size2};
use guillotiere::{AllocId, AtlasAllocator};
use macroquad::texture::{FilterMode, Image, Texture2D};

use crate::{math::Rect, shader::SharedTexture};

/// Preferred page side. FontState clamps this to the driver limit.
pub(super) const DEFAULT_PAGE_SIZE: i32 = 1024;
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
    /// Texture uploads issued (one per coalesced rectangle).
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
    texture: Option<SharedTexture>,
    /// CPU mirror of the page. Every write is staged here and flushed by
    /// end_pass for the current text batch, so adjacent glyphs pay one
    /// texture upload per page instead of one per glyph. The mirror also keeps
    /// untouched texels correct inside a coalesced upload.
    staging: Option<Vec<u8>>,
    /// Rectangles written since the last flush.
    pending: Vec<DirtyRect>,
    /// Any allocation or cache hit pins the entire page against resets.
    last_used: u64,
    /// Payloads on this page when the pass's eviction queue was built.
    candidates: Vec<Key>,
}

impl Page {
    fn new(page_size: i32) -> Self {
        Self {
            side: page_size as usize,
            allocator: AtlasAllocator::new(size2(page_size, page_size)),
            texture: None,
            staging: None,
            pending: Vec::new(),
            last_used: 0,
            candidates: Vec::new(),
        }
    }

    fn ensure_assets(&mut self) {
        if self.staging.is_none() {
            self.staging = Some(vec![0; self.side * self.side * 4]);
        }
        if self.texture.is_none() {
            let texture: SharedTexture = Texture2D::from_image(&Image {
                bytes: vec![0; self.side * self.side * 4],
                width: self.side as u16,
                height: self.side as u16,
            })
            .into();
            texture.set_filter(FilterMode::Linear);
            self.texture = Some(texture);
        }
    }

    fn mark_dirty(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.pending.push(DirtyRect { x, y, w, h });
    }

    /// Flushes staged writes.
    ///
    /// Adjacent glyphs are coalesced into one bounding-box upload, but a
    /// scattered set would make that box cover most of the page, so the flush
    /// falls back to one upload per rectangle when the box is much larger than
    /// the pixels that actually changed.
    fn upload(&mut self, stats: &mut GlyphAtlasStats) {
        if self.pending.is_empty() {
            return;
        }
        let (Some(texture), Some(staging)) = (&self.texture, &self.staging) else {
            self.pending.clear();
            return;
        };
        let stride = self.side * 4;
        let mut union = self.pending[0];
        let mut changed = 0usize;
        for rect in &self.pending {
            union = union.union(rect.x, rect.y, rect.w, rect.h);
            changed += rect.w as usize * rect.h as usize;
        }
        let boxed = union.w as usize * union.h as usize;
        if boxed <= changed.saturating_mul(4) {
            stats.uploads += 1;
            stats.uploaded_pixels += boxed as u64;
            upload_rect(texture, staging, stride, union);
        } else {
            stats.uploads += self.pending.len() as u64;
            for rect in &self.pending {
                stats.uploaded_pixels += rect.w as u64 * rect.h as u64;
                upload_rect(texture, staging, stride, *rect);
            }
        }
        self.pending.clear();
    }
}

/// Uploads one rectangle of the CPU mirror to the page texture.
fn upload_rect(texture: &SharedTexture, staging: &[u8], stride: usize, rect: DirtyRect) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let w = rect.w as usize;
    let h = rect.h as usize;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let start = (rect.y as usize + row) * stride + rect.x as usize * 4;
        bytes.extend_from_slice(&staging[start..start + w * 4]);
    }
    texture.update_part(
        &Image {
            bytes,
            width: rect.w as u16,
            height: rect.h as u16,
        },
        rect.x,
        rect.y,
        rect.w,
        rect.h,
    );
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
    #[cfg(test)]
    candidate_builds: u64,
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
            #[cfg(test)]
            candidate_builds: 0,
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

    /// Uploads every staged write for the current text batch. The caller must
    /// call this after that batch's glyphs have been drawn and before its GPU
    /// geometry is submitted; a later call is a no-op when nothing is staged.
    pub(super) fn end_pass(&mut self) {
        let (pages, stats) = (&mut self.pages, &mut self.stats);
        for page in pages {
            page.upload(stats);
        }
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
            let texture = page.texture.as_ref()?.clone();
            return Some(CachedGlyph {
                texture,
                uv: glyph.uv,
                placement: glyph.placement,
            });
        }

        // Deliberately bypass SwashCache's image cache: the atlas owns the
        // lifetime of this raster and cache churn must not grow unboundedly.
        let image = cache.get_image_uncached(font_system, key)?;
        let placement = image.placement;
        let width = match i32::try_from(placement.width) {
            Ok(width) => width,
            Err(_) => {
                self.stats.rejected_oversized += 1;
                return None;
            }
        };
        let height = match i32::try_from(placement.height) {
            Ok(height) => height,
            Err(_) => {
                self.stats.rejected_oversized += 1;
                return None;
            }
        };
        if width <= 0 || height <= 0 {
            // Whitespace and other empty glyphs have nothing to upload.
            return None;
        }
        let padded_width = match width.checked_add(PADDING * 2) {
            Some(width) => width,
            None => {
                self.stats.rejected_oversized += 1;
                return None;
            }
        };
        let padded_height = match height.checked_add(PADDING * 2) {
            Some(height) => height,
            None => {
                self.stats.rejected_oversized += 1;
                return None;
            }
        };
        if padded_width > self.page_size || padded_height > self.page_size {
            self.stats.rejected_oversized += 1;
            if self.stats.rejected_oversized == 1 {
                eprintln!(
                    "mue glyph atlas: rejected non-empty glyph {}x{} (page size {})",
                    padded_width, padded_height, self.page_size
                );
            }
            return None;
        }
        if !raster_data_valid(image.content, &image.data, width as usize, height as usize) {
            self.stats.rejected_raster += 1;
            return None;
        }

        let Some((page, allocation)) = self.allocate(padded_width, padded_height) else {
            self.stats.rejected_exhausted += 1;
            if self.stats.rejected_exhausted == 1 {
                eprintln!(
                    "mue glyph atlas: rejected non-empty glyph {}x{} (all slots in use)",
                    padded_width, padded_height
                );
            }
            return None;
        };

        let rect = self.pages[page].allocator[allocation];
        let x = rect.min.x + PADDING;
        let y = rect.min.y + PADDING;
        let staged = {
            let page = &mut self.pages[page];
            page.ensure_assets();
            let staging = page.staging.as_mut().expect("staging was just initialized");
            blit_glyph(
                staging,
                page.side,
                x,
                y,
                Raster {
                    content: image.content,
                    data: &image.data,
                    width: width as usize,
                    height: height as usize,
                },
            )
        };
        if !staged {
            self.pages[page].allocator.deallocate(allocation);
            self.stats.rejected_raster += 1;
            return None;
        }

        let page_data = &mut self.pages[page];
        page_data.mark_dirty(x - PADDING, y - PADDING, padded_width, padded_height);
        let texture = page_data
            .texture
            .as_ref()
            .expect("texture was just initialized")
            .clone();

        let page_size = self.page_size as f32;
        let uv = Rect::new(
            x as f32 / page_size,
            y as f32 / page_size,
            width as f32 / page_size,
            height as f32 / page_size,
        );
        self.stats.rasterized += 1;
        self.glyphs.insert(
            key,
            Glyph {
                page,
                allocation,
                uv,
                placement,
                last_used: self.pass,
            },
        );
        Some(CachedGlyph {
            texture,
            uv,
            placement,
        })
    }

    fn allocate(&mut self, width: i32, height: i32) -> Option<(usize, AllocId)> {
        if width <= 0 || height <= 0 || width > self.page_size || height > self.page_size {
            return None;
        }
        let requested = size2(width, height);
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
        if self.pages.len() < self.max_pages {
            let page = self.pages.len();
            self.pages.push(Page::new(self.page_size));
            if let Some(allocation) = self.pages[page].allocator.allocate(requested) {
                self.pages[page].last_used = self.pass;
                return Some((page, allocation.id));
            }
        }
        None
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
        #[cfg(test)]
        {
            self.candidate_builds += 1;
        }
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

/// One rasterized glyph handed to the page mirror.
struct Raster<'a> {
    content: SwashContent,
    data: &'a [u8],
    width: usize,
    height: usize,
}

fn raster_data_valid(content: SwashContent, data: &[u8], width: usize, height: usize) -> bool {
    let Some(pixels) = width.checked_mul(height) else {
        return false;
    };
    match content {
        SwashContent::Mask => data.len() == pixels,
        SwashContent::Color | SwashContent::SubpixelMask => {
            pixels.checked_mul(4).is_some_and(|len| data.len() == len)
        }
    }
}

/// Writes one padded glyph raster into the CPU page mirror.
///
/// The one-pixel border is cleared first: a reused slot still holds the
/// previous glyph's texels, and the border must not inherit them.
fn blit_glyph(staging: &mut [u8], side: usize, x: i32, y: i32, raster: Raster<'_>) -> bool {
    if !raster_data_valid(raster.content, raster.data, raster.width, raster.height) {
        return false;
    }
    let w = raster.width;
    let h = raster.height;
    let stride = side * 4;
    let pad = PADDING as usize;
    let left = x as usize - pad;
    let top = y as usize - pad;
    let padded_width = w + pad * 2;

    for row in 0..h + pad * 2 {
        let start = (top + row) * stride + left * 4;
        staging[start..start + padded_width * 4].fill(0);
    }

    let Some(pixels) = w.checked_mul(h) else {
        return false;
    };
    let glyph_start = |row: usize| (top + pad + row) * stride + (left + pad) * 4;
    match raster.content {
        SwashContent::Mask => {
            if raster.data.len() != pixels {
                return false;
            }
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
            true
        }
        SwashContent::Color => {
            if raster.data.len() != pixels * 4 {
                return false;
            }
            for row in 0..h {
                let start = glyph_start(row);
                let source = row * w * 4;
                staging[start..start + w * 4].copy_from_slice(&raster.data[source..source + w * 4]);
            }
            true
        }
        // Swash describes this as four bytes containing RGB coverage. Treat
        // the strongest channel as coverage and render it as white alpha;
        // this is deterministic and avoids panicking on an uncommon format.
        SwashContent::SubpixelMask => {
            if raster.data.len() != pixels * 4 {
                return false;
            }
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
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::CacheKeyFlags;
    use cosmic_text::fontdb::Weight;

    fn key(glyph_id: u16) -> Key {
        CacheKey {
            font_id: cosmic_text::fontdb::ID::dummy(),
            glyph_id,
            font_size_bits: 16.0f32.to_bits(),
            x_bin: cosmic_text::SubpixelBin::Zero,
            y_bin: cosmic_text::SubpixelBin::Zero,
            font_weight: Weight::NORMAL,
            flags: CacheKeyFlags::empty(),
        }
    }

    fn add_glyph(atlas: &mut Atlas, id: u16, width: i32, height: i32, last_used: u64) {
        let page = 0;
        let allocation = atlas.pages[page]
            .allocator
            .allocate(size2(width, height))
            .expect("test allocation")
            .id;
        atlas.glyphs.insert(
            key(id),
            Glyph {
                page,
                allocation,
                uv: Rect::default(),
                placement: Placement::default(),
                last_used,
            },
        );
    }

    #[test]
    fn page_allocations_stay_in_bounds_and_do_not_overlap() {
        let mut page = Page::new(32);
        let mut rectangles = Vec::new();
        for size in [(3, 5), (7, 2), (4, 4), (9, 3), (2, 8), (6, 6)] {
            let allocation = page
                .allocator
                .allocate(size2(size.0, size.1))
                .expect("allocation");
            let rect = allocation.rectangle;
            assert!(rect.min.x >= 0 && rect.min.y >= 0);
            assert!(rect.max.x <= 32 && rect.max.y <= 32);
            rectangles.push(rect);
        }
        for (index, a) in rectangles.iter().enumerate() {
            for b in rectangles.iter().skip(index + 1) {
                assert!(
                    a.max.x <= b.min.x
                        || b.max.x <= a.min.x
                        || a.max.y <= b.min.y
                        || b.max.y <= a.min.y
                );
            }
        }
    }

    #[test]
    fn zero_and_oversized_requests_are_rejected_without_pages() {
        let mut atlas = Atlas::new(8);
        assert!(atlas.allocate(0, 1).is_none());
        assert!(atlas.allocate(1, 0).is_none());
        assert!(atlas.allocate(9, 1).is_none());
        assert!(atlas.allocate(1, 9).is_none());
        assert!(atlas.pages.is_empty());
    }

    #[test]
    fn max_pages_is_bounded() {
        let mut atlas = Atlas::new(4);
        atlas.max_pages = 2;
        for _ in 0..2 {
            assert!(atlas.allocate(4, 4).is_some());
        }
        assert!(atlas.allocate(1, 1).is_none());
        assert_eq!(atlas.pages.len(), 2);
    }

    #[test]
    fn touched_after_candidate_build_is_not_evicted() {
        let mut atlas = Atlas::new(32);
        atlas.max_pages = 0;
        atlas.pages.push(Page::new(32));
        for id in 1..=64 {
            add_glyph(&mut atlas, id, 4, 4, 1);
        }
        atlas.begin_pass();
        atlas.build_candidates();
        let touched = atlas.candidates.last().unwrap().1;
        let mut fonts = FontSystem::new_with_locale_and_db(
            "en-US".into(),
            cosmic_text::fontdb::Database::new(),
        );
        // The real cache-hit path pins the page even though this no-GPU
        // fixture has no texture to return.
        assert!(
            atlas
                .cache_glyph(&mut fonts, &mut SwashCache::new(), touched)
                .is_none()
        );
        atlas.pages[0].mark_dirty(0, 0, 1, 1);
        assert!(atlas.allocate(4, 4).is_some());
        assert!(atlas.glyphs.contains_key(&touched));
        assert_eq!(atlas.pages[0].pending.len(), 1);
        assert_eq!(atlas.stats.evicted, 1);
        assert_eq!(atlas.candidate_builds, 1);
    }

    #[test]
    fn more_than_32_old_entries_are_evicted_without_rebuilding_queue() {
        let mut atlas = Atlas::new(32);
        atlas.max_pages = 0;
        atlas.pages.push(Page::new(32));
        for id in 0..64 {
            add_glyph(&mut atlas, id, 4, 4, 1);
        }
        atlas.begin_pass();
        // A current-pass survivor forbids whole-page reclamation, so this
        // exercises the individual candidate queue, including >32 retries.
        atlas.glyphs.get_mut(&key(0)).unwrap().last_used = atlas.pass;
        atlas.pages[0].last_used = atlas.pass;
        for id in 64..104 {
            let (page, allocation) = atlas.allocate(4, 4).expect("eviction allocation");
            atlas.glyphs.insert(
                key(id),
                Glyph {
                    page,
                    allocation,
                    uv: Rect::default(),
                    placement: Placement::default(),
                    last_used: atlas.pass,
                },
            );
        }
        assert_eq!(atlas.stats.evicted, 40);
        assert_eq!(atlas.candidate_builds, 1);
    }

    #[test]
    fn candidate_build_is_bounded_to_once_per_pass_under_churn() {
        let mut atlas = Atlas::new(16);
        atlas.max_pages = 0;
        atlas.pages.push(Page::new(16));
        for id in 0..16 {
            add_glyph(&mut atlas, id, 4, 4, 1);
        }
        for pass in 0..1000 {
            atlas.begin_pass();
            for id in 1000 + pass * 10..1000 + pass * 10 + 10 {
                let id = id as u16;
                let (page, allocation) = atlas.allocate(4, 4).expect("churn allocation");
                atlas.glyphs.insert(
                    key(id),
                    Glyph {
                        page,
                        allocation,
                        uv: Rect::default(),
                        placement: Placement::default(),
                        last_used: atlas.pass,
                    },
                );
            }
        }
        assert_eq!(atlas.candidate_builds, 1000);
    }

    #[test]
    fn pressure_resets_only_untouched_page_and_retains_staging() {
        let mut atlas = Atlas::new(8);
        atlas.max_pages = 1;
        atlas.pages.push(Page::new(8));
        for id in 0..4 {
            add_glyph(&mut atlas, id, 4, 4, 1);
        }
        let page = &mut atlas.pages[0];
        page.last_used = atlas.pass;
        page.staging = Some(vec![77; 8 * 8 * 4]);
        page.mark_dirty(0, 0, 4, 4);
        let staging_ptr = page.staging.as_ref().unwrap().as_ptr();

        atlas.begin_pass();
        // Merely beginning a pass never resets or evicts anything.
        assert_eq!(atlas.glyphs.len(), 4);
        assert_eq!(atlas.stats.evicted, 0);
        assert_eq!(atlas.pages[0].pending.len(), 1);
        assert!(atlas.allocate(8, 8).is_some());
        assert!(atlas.glyphs.is_empty());
        assert_eq!(atlas.stats.evicted, 4);
        assert_eq!(atlas.pages.len(), 1);
        assert!(atlas.pages[0].pending.is_empty());
        assert_eq!(
            atlas.pages[0].staging.as_ref().unwrap().as_ptr(),
            staging_ptr
        );
        assert!(
            atlas.pages[0]
                .staging
                .as_ref()
                .unwrap()
                .iter()
                .all(|&b| b == 77)
        );
        // The newly allocated slot pins this page even before insertion.
        assert!(atlas.allocate(1, 1).is_none());
        assert_eq!(atlas.stats.evicted, 4);
    }

    #[test]
    fn varying_sizes_turn_over_pages_without_evicting_current_pass() {
        let mut atlas = Atlas::new(512);
        for pass in 0..12u32 {
            atlas.begin_pass();
            // A few cache hits survive from the previous layout. Those pages
            // must never be reset, even after the candidate queue is built.
            let mut pins: Vec<_> = atlas
                .glyphs
                .iter()
                .map(|(key, glyph)| (*key, glyph.page, glyph.allocation))
                .collect();
            pins.sort_unstable_by_key(|(key, _, _)| *key);
            pins.truncate(32);
            for (key, page, _) in &pins {
                atlas.glyphs.get_mut(key).unwrap().last_used = atlas.pass;
                atlas.pages[*page].last_used = atlas.pass;
            }
            for id in 0..4000u16 {
                let width = 4 + ((u32::from(id) * 17 + pass * 7) % 20) as i32;
                let height = 4 + ((u32::from(id) * 13 + pass * 11) % 20) as i32;
                let (page, allocation) = atlas
                    .allocate(width, height)
                    .unwrap_or_else(|| panic!("pass {pass}, glyph {id}, {width}x{height}"));
                let mut key = key(id);
                key.font_size_bits = pass;
                atlas.glyphs.insert(
                    key,
                    Glyph {
                        page,
                        allocation,
                        uv: Rect::default(),
                        placement: Placement::default(),
                        last_used: atlas.pass,
                    },
                );
            }
            for (key, page, allocation) in &pins {
                let glyph = atlas.glyphs.get(key).expect("current-pass glyph evicted");
                assert_eq!((glyph.page, glyph.allocation), (*page, *allocation));
            }
            if pass % 4 == 0 {
                let mut occupied = vec![vec![false; 512 * 512]; atlas.pages.len()];
                for glyph in atlas.glyphs.values() {
                    let rect = atlas.pages[glyph.page].allocator[glyph.allocation];
                    assert!(rect.min.x >= 0 && rect.min.y >= 0);
                    assert!(rect.max.x <= 512 && rect.max.y <= 512);
                    for y in rect.min.y..rect.max.y {
                        for x in rect.min.x..rect.max.x {
                            let cell = &mut occupied[glyph.page][(y * 512 + x) as usize];
                            assert!(!*cell, "live allocation overlap");
                            *cell = true;
                        }
                    }
                }
            }
            assert!(atlas.pages.len() <= MAX_PAGES);
            assert!(atlas.candidate_builds <= u64::from(pass) + 1);
        }
        assert!(atlas.stats.evicted > 4000);
        assert!(
            atlas
                .pages
                .iter()
                .all(|page| page.texture.is_none() && page.staging.is_none())
        );
    }

    #[test]
    fn malformed_raster_is_rejected_before_staging() {
        let mut staging = vec![0; 4 * 4 * 4];
        assert!(!blit_glyph(
            &mut staging,
            4,
            1,
            1,
            Raster {
                content: SwashContent::Mask,
                data: &[255; 3],
                width: 2,
                height: 2,
            },
        ));
        assert_eq!(staging, vec![0; 4 * 4 * 4]);
    }

    #[test]
    fn blit_clears_transparent_border_and_validates_data_lengths() {
        let mut staging = vec![77; 6 * 6 * 4];
        assert!(blit_glyph(
            &mut staging,
            6,
            2,
            2,
            Raster {
                content: SwashContent::Mask,
                data: &[255],
                width: 1,
                height: 1,
            },
        ));
        for row in 1..=3 {
            for col in 1..=3 {
                let pixel = &staging[(row * 6 + col) * 4..(row * 6 + col + 1) * 4];
                if row == 2 && col == 2 {
                    assert_eq!(pixel, &[255, 255, 255, 255]);
                } else {
                    assert_eq!(pixel, &[0, 0, 0, 0]);
                }
            }
        }
        assert!(!raster_data_valid(SwashContent::Color, &[0; 3], 1, 1));
        assert!(!raster_data_valid(
            SwashContent::SubpixelMask,
            &[0; 3],
            1,
            1
        ));
        assert!(raster_data_valid(SwashContent::Color, &[0; 4], 1, 1));
    }
}
