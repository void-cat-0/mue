use std::time::Instant;

use guillotiere::{AllocId, Allocation, AtlasAllocator, size2};
use macroquad::miniquad::{RenderPass, RenderingBackend, TextureId};
use macroquad::window::get_internal_gl;

use super::{Baker, Key, MAX_BYTES, MAX_SLOTS, MaskSample, STABLE_FOR, ShadowMaskCacheStats};

const PAGE_SIDE: u32 = 1024;

#[derive(Default)]
pub(super) struct VulkanAtlas {
    pages: Vec<Page>,
    entries: Vec<Entry>,
}

struct EvictionPlan {
    page: usize,
    allocator: AtlasAllocator,
    allocation: Allocation,
    entries: Vec<usize>,
}

struct Page {
    texture: TextureId,
    pass: RenderPass,
    size: (u32, u32),
    initialized: bool,
    allocator: AtlasAllocator,
}

impl Page {
    fn fits(&self, key: Key) -> bool {
        key.pixels.0 <= self.size.0 && key.pixels.1 <= self.size.1
    }

    fn bytes(&self) -> usize {
        image_bytes(self.size)
    }

    fn sample(&self, region: [i32; 4]) -> MaskSample {
        let [x, y, w, h] = region;
        let sx = 1. / self.size.0 as f32;
        let sy = 1. / self.size.1 as f32;
        MaskSample {
            texture: self.texture,
            atlas: Some((
                [x as f32 * sx, y as f32 * sy, w as f32 * sx, h as f32 * sy],
                [sx, sy],
            )),
        }
    }
}

struct Entry {
    key: Key,
    page: usize,
    allocation: AllocId,
    region: [i32; 4],
    touched: Instant,
}

fn plan_page_eviction(
    entries: &[Entry],
    page: usize,
    allocator: &AtlasAllocator,
    key: Key,
    now: Instant,
) -> Option<EvictionPlan> {
    let mut eligible: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.page == page && now.duration_since(entry.touched) >= STABLE_FOR)
        .collect();
    if eligible.is_empty() {
        return None;
    }
    eligible.sort_unstable_by_key(|(index, entry)| (entry.touched, *index));
    let mut trial = allocator.clone();
    let mut victims = Vec::new();
    let requested = size2(key.pixels.0 as i32, key.pixels.1 as i32);
    for (index, entry) in eligible {
        trial.deallocate(entry.allocation);
        victims.push(index);
        if let Some(allocation) = trial.allocate(requested) {
            return Some(EvictionPlan {
                page,
                allocator: trial,
                allocation,
                entries: victims,
            });
        }
    }
    // The old pixels and their keys remain usable when no trial fits.
    None
}

fn image_bytes(size: (u32, u32)) -> usize {
    size.0 as usize * size.1 as usize * 4
}

fn page_size(key: Key, limit: u32) -> Option<(u32, u32)> {
    let (w, h) = key.pixels;
    if w > limit || h > limit {
        return None;
    }
    let bucket = |side: u32| side.next_power_of_two().max(PAGE_SIDE).min(limit);
    Some((bucket(w), bucket(h)))
}

fn active_recording_frame(_ctx: &mut dyn RenderingBackend) -> bool {
    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    {
        _ctx.vulkan_context()
            .is_some_and(|vk| vk.frame_active && vk.recording && !vk.frame_submitted)
    }
    #[cfg(not(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    )))]
    false
}

impl VulkanAtlas {
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn allocated_bytes(&self) -> usize {
        self.pages.iter().map(Page::bytes).sum()
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        for page in &mut self.pages {
            page.allocator.clear();
        }
        // A future bake flushes any queued samplers before overwriting pixels.
        // There is nothing to synchronize (or destroy) just to forget keys.
    }

    pub(super) fn find(&mut self, key: Key, now: Instant) -> Option<MaskSample> {
        let entry = self.entries.iter_mut().find(|entry| entry.key == key)?;
        entry.touched = now;
        Some(self.pages[entry.page].sample(entry.region))
    }

    pub(super) fn can_bake(&self, key: Key, now: Instant) -> bool {
        if self.entries.len() >= MAX_SLOTS && self.oldest(now).is_none() {
            return false;
        }
        self.pages.iter().any(|page| page.fits(key))
            || page_size(key, super::MAX_SIDE)
                .is_some_and(|size| self.allocated_bytes() + image_bytes(size) <= MAX_BYTES)
    }

    fn oldest(&self, now: Instant) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| now.duration_since(entry.touched) >= STABLE_FOR)
            .min_by_key(|(_, entry)| entry.touched)
            .map(|(i, _)| i)
    }

    fn plan_eviction(&self, key: Key, now: Instant) -> Option<EvictionPlan> {
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, page)| page.fits(key))
            .filter_map(|(index, page)| {
                plan_page_eviction(&self.entries, index, &page.allocator, key, now)
            })
            // Prefer fewer evictions, then the oldest set of victims.
            .min_by_key(|plan| {
                (
                    plan.entries.len(),
                    self.entries[*plan.entries.last().unwrap()].touched,
                )
            })
    }

    fn commit_eviction(
        &mut self,
        mut plan: EvictionPlan,
        stats: &mut ShadowMaskCacheStats,
    ) -> (usize, Allocation) {
        self.pages[plan.page].allocator = plan.allocator;
        // These indices refer to the original entries. Remove in reverse so
        // swap_remove cannot invalidate the still-pending indices.
        plan.entries.sort_unstable();
        for index in plan.entries.into_iter().rev() {
            self.entries.swap_remove(index);
            stats.evictions += 1;
        }
        (plan.page, plan.allocation)
    }

    fn evict(&mut self, index: usize, stats: &mut ShadowMaskCacheStats) {
        let entry = self.entries.swap_remove(index);
        self.pages[entry.page]
            .allocator
            .deallocate(entry.allocation);
        stats.evictions += 1;
    }

    fn allocate_existing(&mut self, key: Key) -> Option<(usize, Allocation)> {
        self.pages.iter_mut().enumerate().find_map(|(index, page)| {
            page.allocator
                .allocate(size2(key.pixels.0 as i32, key.pixels.1 as i32))
                .map(|allocation| (index, allocation))
        })
    }

    fn allocate_page(
        &mut self,
        key: Key,
        max_side: u32,
        ctx: &mut dyn RenderingBackend,
    ) -> Option<(usize, Allocation)> {
        let (w, h) = page_size(key, max_side)?;
        if self.allocated_bytes() + image_bytes((w, h)) > MAX_BYTES {
            return None;
        }
        let mut allocator = AtlasAllocator::new(size2(w as i32, h as i32));
        let allocation = allocator.allocate(size2(key.pixels.0 as i32, key.pixels.1 as i32))?;
        let texture = ctx.try_new_rgba8_render_texture(w, h)?;
        let pass = ctx.new_render_pass(texture, None);
        let index = self.pages.len();
        self.pages.push(Page {
            texture,
            pass,
            size: (w, h),
            initialized: false,
            allocator,
        });
        Some((index, allocation))
    }

    pub(super) fn bake(
        &mut self,
        key: Key,
        now: Instant,
        baker: &Baker,
        max_side: u32,
        stats: &mut ShadowMaskCacheStats,
    ) -> Option<MaskSample> {
        let mut gl = unsafe { get_internal_gl() };
        if !active_recording_frame(gl.quad_context) {
            return None;
        }
        // Confirm capacity before touching an allocator. An eviction plan
        // makes its own room, even when the separate mask-count cap is full.
        let capacity_victim = if self.entries.len() >= MAX_SLOTS {
            Some(self.oldest(now)?)
        } else {
            None
        };
        let (page_index, allocation) = if let Some(placement) = self
            .allocate_existing(key)
            .or_else(|| self.allocate_page(key, max_side, gl.quad_context))
        {
            if let Some(index) = capacity_victim {
                self.evict(index, stats);
            }
            placement
        } else {
            let plan = self.plan_eviction(key, now)?;
            self.commit_eviction(plan, stats)
        };
        let page = &mut self.pages[page_index];
        let region = [
            allocation.rectangle.min.x,
            allocation.rectangle.min.y,
            allocation.rectangle.width(),
            allocation.rectangle.height(),
        ];
        // Queued Macroquad draws must read the previous contents. The backend
        // records the sampled->attachment and attachment->sampled barriers in
        // this same graphics stream, also ordering reads in older frame slots.
        gl.flush();
        baker.bake_atlas(
            gl.quad_context,
            page.pass,
            page.texture,
            key,
            region,
            !page.initialized,
        );
        page.initialized = true;
        self.entries.push(Entry {
            key,
            page: page_index,
            allocation: allocation.id,
            region,
            touched: Instant::now(),
        });
        stats.bakes += 1;
        Some(page.sample(region))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pixels: (u32, u32)) -> Key {
        Key {
            width: 160.,
            height: 70.,
            elevation: 12.,
            radius: 10.,
            pixels,
        }
    }

    fn add_entry(allocator: &mut AtlasAllocator, pixels: (u32, u32), touched: Instant) -> Entry {
        let allocation = allocator
            .allocate(size2(pixels.0 as i32, pixels.1 as i32))
            .unwrap();
        let rect = allocation.rectangle;
        Entry {
            key: key(pixels),
            page: 0,
            allocation: allocation.id,
            region: [rect.min.x, rect.min.y, rect.width(), rect.height()],
            touched,
        }
    }

    #[test]
    fn failed_placement_does_not_evict_older_masks() {
        let now = Instant::now();
        let old = now - STABLE_FOR;
        let mut allocator = AtlasAllocator::new(size2(1024, 1024));
        let recent = add_entry(&mut allocator, (700, 700), now);
        let stale = add_entry(&mut allocator, (300, 300), old);
        let entries = [recent, stale];

        assert!(plan_page_eviction(&entries, 0, &allocator, key((900, 900)), now).is_none());
        assert_eq!(allocator[entries[0].allocation].size(), size2(700, 700));
        assert_eq!(allocator[entries[1].allocation].size(), size2(300, 300));
    }

    #[test]
    fn successful_trial_reclaims_only_the_eligible_region_and_preserves_survivor_ids() {
        let now = Instant::now();
        let old = now - STABLE_FOR;
        let mut allocator = AtlasAllocator::new(size2(1024, 1024));
        let recent = add_entry(&mut allocator, (1024, 512), now);
        let stale = add_entry(&mut allocator, (1024, 512), old);
        let entries = [recent, stale];

        assert!(
            plan_page_eviction(
                &entries,
                0,
                &allocator,
                key((1024, 500)),
                now - std::time::Duration::from_millis(1),
            )
            .is_none()
        );
        let plan = plan_page_eviction(&entries, 0, &allocator, key((1024, 500)), now).unwrap();
        assert_eq!(plan.entries, [1]);
        assert_eq!(plan.allocation.rectangle.min.y, entries[1].region[1]);
        assert_eq!(
            plan.allocator[entries[0].allocation],
            allocator[entries[0].allocation]
        );
        assert_eq!(allocator[entries[1].allocation].size(), size2(1024, 512));
    }

    #[test]
    fn page_bucket_grows_only_the_needed_axis() {
        assert_eq!(page_size(key((490, 230)), 2048), Some((1024, 1024)));
        assert_eq!(page_size(key((1750, 140)), 2048), Some((2048, 1024)));
        assert_eq!(page_size(key((980, 1216)), 2048), Some((1024, 2048)));
        assert_eq!(page_size(key((1025, 300)), 1024), None);
        assert_eq!(page_size(key((200, 150)), 768), Some((768, 768)));
    }
}
