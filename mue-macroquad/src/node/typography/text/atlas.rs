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
    /// CPU mirror of the page. Every write is staged here and flushed in
    /// end_pass, so a frame that adds many adjacent glyphs pays one texture
    /// upload per page instead of one per glyph. The mirror also keeps
    /// untouched texels correct inside a coalesced upload.
    staging: Option<Vec<u8>>,
    /// Rectangles written since the last flush.
    pending: Vec<DirtyRect>,
}

impl Page {
    fn new(page_size: i32) -> Self {
        Self {
            side: page_size as usize,
            allocator: AtlasAllocator::new(size2(page_size, page_size)),
            texture: None,
            staging: None,
            pending: Vec::new(),
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
    }

    /// Uploads every staged write. The caller must call this after all glyphs
    /// for the pass have been drawn and before the GPU geometry is submitted.
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
            let texture = self.pages[glyph.page].texture.as_ref()?.clone();
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
        let width = i32::try_from(placement.width).ok()?;
        let height = i32::try_from(placement.height).ok()?;
        if width <= 0 || height <= 0 {
            // Whitespace and other empty glyphs have nothing to upload.
            return None;
        }
        let padded_width = width.checked_add(PADDING * 2)?;
        let padded_height = height.checked_add(PADDING * 2)?;
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
        let requested = size2(width, height);
        if let Some(slot) = self.allocate_in_existing_pages(requested) {
            return Some(slot);
        }

        // Only pay for ordering once space is actually needed. Sorting the
        // evictable glyphs here is cheaper than unpinning and re-queueing every
        // entry on every frame, and it is skipped entirely until the atlas
        // fills up.
        let mut candidates: Vec<(u64, Key)> = self
            .glyphs
            .iter()
            .filter(|(_, glyph)| glyph.last_used < self.pass)
            .map(|(key, glyph)| (glyph.last_used, *key))
            .collect();
        candidates.sort_unstable_by_key(|(last_used, _)| *last_used);
        for (_, key) in candidates {
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
                return Some((page, allocation.id));
            }
        }
        None
    }

    fn allocate_in_existing_pages(
        &mut self,
        requested: Size2D<i32, UnknownUnit>,
    ) -> Option<(usize, AllocId)> {
        self.pages.iter_mut().enumerate().find_map(|(page, data)| {
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

/// Writes one padded glyph raster into the CPU page mirror.
///
/// The one-pixel border is cleared first: a reused slot still holds the
/// previous glyph's texels, and the border must not inherit them.
fn blit_glyph(staging: &mut [u8], side: usize, x: i32, y: i32, raster: Raster<'_>) -> bool {
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
