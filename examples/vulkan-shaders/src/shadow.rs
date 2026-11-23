//! Offscreen pixel and cache checks for the Vulkan drop-shadow path.

use std::time::{Duration, Instant};

use macroquad::prelude::*;
use mue_macroquad::{
    Shadow,
    color::Color as MueColor,
    math::{Matrix, Rect as LayoutRect, vec2 as layout_vec2},
    node::{IntoNode, Node, flexbox},
    shadow::{ShadowMaskCacheStats, clear_shadow_mask_cache, shadow_mask_cache_stats},
    style::Styleable,
};

const WIDTH: u32 = 384;
const HEIGHT: u32 = 256;
const BACKGROUND: [u8; 4] = [19, 24, 30, 255];
// Match the shader smoke's per-channel tolerance (3/255).
const CHANNEL_TOLERANCE: u8 = 3;
const BAKE_TIMEOUT: Duration = Duration::from_secs(8);

type Scissor = (i32, i32, i32, i32);
const B_SCISSOR: Scissor = (69, 132, 84, 72);

const WARM_COLOR: MueColor = MueColor::from_rgba(238, 48, 26, 208);
const TINT_COLOR: MueColor = MueColor::from_rgba(32, 176, 237, 194);
const COLD_COLOR: MueColor = MueColor::from_rgba(181, 49, 244, 215);

fn is_vulkan() -> bool {
    #[cfg(any(
        windows,
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    ))]
    {
        unsafe { get_internal_gl() }.quad_context.info().backend == miniquad::Backend::Vulkan
    }
    #[cfg(not(any(
        windows,
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    )))]
    {
        false
    }
}

struct Offscreen {
    camera: Camera2D,
}

impl Offscreen {
    fn new(width: u32, height: u32) -> Self {
        Self {
            camera: Camera2D {
                render_target: Some(render_target(width, height)),
                ..Camera2D::from_display_rect(Rect::new(0., 0., width as f32, height as f32))
            },
        }
    }

    fn texture(&self) -> &Texture2D {
        &self.camera.render_target.as_ref().unwrap().texture
    }

    fn draw(&self, draw: impl FnOnce()) {
        set_camera(&self.camera);
        clear_background(BACKGROUND.into());
        draw();
        // Flush queued draws before readback or yielding a host frame.
        set_default_camera();
    }

    fn read(&self) -> Image {
        self.texture().get_texture_data()
    }
}

fn with_scissor(scissor: Scissor, draw: impl FnOnce()) {
    // Use target pixels, independent of the window's DPI scale.
    let gl = unsafe { get_internal_gl() }.quad_gl;
    let previous = gl.get_scissor();
    gl.scissor(Some(scissor));
    draw();
    unsafe { get_internal_gl() }.quad_gl.scissor(previous);
}

struct Scene {
    a: Node,
    b: Node,
    translated: Node,
    tinted: Node,
}

impl Scene {
    fn new(cache: bool) -> Self {
        let warm = Shadow::new(9., 13.).color(WARM_COLOR);
        let cold = Shadow::new(11., 7.).color(COLD_COLOR);
        Self {
            a: panel(cache, [49., 48., 72., 41.], warm, 0.88, Matrix::IDENTITY),
            b: panel(
                cache,
                [53., 149., 86., 42.],
                cold,
                0.87,
                Matrix::from_angle(0.14),
            ),
            translated: panel(
                cache,
                [226.375, 42.25, 72., 41.],
                warm,
                0.88,
                Matrix::from_translation(layout_vec2(4.25, 2.125)) * Matrix::from_angle(0.27),
            ),
            tinted: panel(
                cache,
                [231.125, 151.375, 72., 41.],
                warm.color(TINT_COLOR),
                0.66,
                Matrix::from_translation(layout_vec2(-6.25, 4.625)) * Matrix::from_angle(-0.31),
            ),
        }
    }

    fn draw(&self, probes: &TextureProbes) {
        self.a.render();
        probes.draw(4.);
        // On the first mixed bake, A has already queued a sample when B writes
        // another mask. The two A peers below sample the old key again.
        with_scissor(B_SCISSOR, || self.b.render());
        self.translated.render();
        self.tinted.render();
        probes.draw(236.);
    }
}

fn panel(cache: bool, [x, y, w, h]: [f32; 4], shadow: Shadow, opacity: f32, model: Matrix) -> Node {
    let node = flexbox()
        .width(w)
        .height(h)
        .transform(model)
        .opacity(opacity)
        .shadow(shadow)
        .shadow_cache(cache)
        .into_node();
    // Leave the body transparent so the shader's sharp interior cutout is visible.
    node.prepare_layout_in(LayoutRect::new(x, y, w, h));
    node
}

// A non-symmetric 2x2 texture detects per-channel, orientation, and binding errors.
const PALETTE: [[u8; 4]; 4] = [
    [231, 53, 29, 255],
    [27, 211, 77, 255],
    [44, 76, 238, 255],
    [247, 194, 83, 255],
];
const PATCH: f32 = 12.;

struct TextureProbes {
    natural: Texture2D,
    target: Offscreen,
}

impl TextureProbes {
    fn new() -> Self {
        let natural = Texture2D::from_rgba8(2, 2, PALETTE.as_flattened());
        natural.set_filter(FilterMode::Nearest);
        let target = Offscreen::new(PATCH as u32, PATCH as u32);
        target.texture().set_filter(FilterMode::Nearest);
        target.draw(|| Self::stamp(&natural, 0., 0., false));
        // Don't read back this source: its GPU output is consumed by sampling.
        Self { natural, target }
    }

    fn stamp(texture: &Texture2D, x: f32, y: f32, flip_y: bool) {
        draw_texture_ex(
            texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(PATCH, PATCH)),
                flip_y,
                ..Default::default()
            },
        );
    }

    fn draw(&self, y: f32) {
        Self::stamp(&self.natural, 6., y, false);
        // Macroquad's offscreen targets use bottom-up rows, unlike uploaded images.
        Self::stamp(self.target.texture(), 25., y, true);
    }
}

fn pixel_at(image: &Image, x: usize, y: usize) -> [u8; 4] {
    image.get_image_data()[(image.height() - 1 - y) * image.width() + x]
}

fn rgba_error(a: [u8; 4], b: [u8; 4]) -> u8 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap()
}

fn assert_pixels(label: &str, expected: &Image, actual: &Image) {
    assert_eq!(
        (expected.width, expected.height),
        (actual.width, actual.height)
    );
    let (index, (reference, result)) = expected
        .get_image_data()
        .iter()
        .zip(actual.get_image_data())
        .enumerate()
        .max_by_key(|(_, (reference, result))| rgba_error(**reference, **result))
        .unwrap();
    let error = rgba_error(*reference, *result);
    let (x, y) = (
        index % actual.width(),
        actual.height() - 1 - index / actual.width(),
    );
    assert!(
        error <= CHANNEL_TOLERANCE,
        "{label}: pixel ({x}, {y}) expected {reference:?}, got {result:?} (max channel error {error})"
    );
}

fn assert_pixel(label: &str, image: &Image, x: usize, y: usize, expected: [u8; 4]) {
    let actual = pixel_at(image, x, y);
    assert!(
        rgba_error(expected, actual) <= CHANNEL_TOLERANCE,
        "{label}: pixel ({x}, {y}) expected {expected:?}, got {actual:?}"
    );
}

fn assert_probes(image: &Image) {
    for y in [4, 236] {
        for (x, label) in [(6, "uploaded texture"), (25, "sampled render target")] {
            for (i, color) in PALETTE.into_iter().enumerate() {
                assert_pixel(
                    label,
                    image,
                    x + 3 + 6 * (i % 2),
                    y + 3 + 6 * (i / 2),
                    color,
                );
            }
        }
    }

    // Absolute invariants prevent an empty or mis-scissored analytic image from
    // acting as an oracle. Test a visible halo, its trimmed neighbor, and its core.
    assert_pixel("transparent shadow cutout", image, 85, 68, BACKGROUND);
    assert_pixel("outside shadow scissor", image, 64, 195, BACKGROUND);
    let [r, _, b, _] = pixel_at(image, 85, 92);
    assert!(r > b.saturating_add(24), "warm red shadow halo missing");
    let [r, _, b, _] = pixel_at(image, 268, 199);
    assert!(b > r.saturating_add(24), "rotated blue shadow halo missing");
    let [r, _, b, _] = pixel_at(image, 73, 195);
    assert!(r > 40 && b > 60, "visible part of the violet halo missing");
}

fn assert_pool_bounds(stats: ShadowMaskCacheStats) {
    assert!(
        stats.slots <= 8
            && stats.resident_masks <= 8
            && stats.allocated_bytes <= 16 * 1024 * 1024
            && stats.pending_keys <= 16,
        "shadow mask cache exceeded its bounds: {stats:?}"
    );
}

async fn draw_until_bake(
    label: &str,
    draw: impl Fn(),
) -> (ShadowMaskCacheStats, ShadowMaskCacheStats) {
    let deadline = Instant::now() + BAKE_TIMEOUT;
    loop {
        let before = shadow_mask_cache_stats();
        draw();
        let after = shadow_mask_cache_stats();
        assert_pool_bounds(after);
        if after.bakes > before.bakes {
            return (before, after);
        }
        assert!(
            Instant::now() < deadline,
            "{label}: no Vulkan shadow bake: {after:?}"
        );
        next_frame().await;
    }
}

/// Fill the page budget, then force a new key to overwrite the oldest region.
async fn check_atlas_rewrite() {
    let target = Offscreen::new(384, 384);
    let previous = Offscreen::new(384, 384);
    let panel_for = |index, cache| {
        let elevation = 6. + 2. * index as f32;
        let side = 318. - 6. * elevation;
        let origin = (384. - side) * 0.5;
        // All masks cover 318 × 318 local pixels, or 638 × 638 texels with
        // the border. Only one fits in each 1024 × 1024 page. Varying the
        // elevation changes the shape with the same atlas allocation size.
        panel(
            cache,
            [origin, origin, side, side],
            Shadow::new(elevation, 12.).color(WARM_COLOR),
            1.,
            Matrix::IDENTITY,
        )
    };
    let first = panel_for(0, true);
    let last = panel_for(4, true);
    previous.draw(|| panel_for(0, false).render());
    let first_oracle = previous.read();
    target.draw(|| panel_for(4, false).render());
    let last_oracle = target.read();
    // This point is inside the first cutout and in the last shadow's halo.
    assert!(
        rgba_error(
            pixel_at(&first_oracle, 192, 72),
            pixel_at(&last_oracle, 192, 72)
        ) > 8
    );

    draw_until_bake("first large mask", || previous.draw(|| first.render())).await;
    for index in 1..4 {
        next_frame().await;
        let node = panel_for(index, true);
        draw_until_bake("fill large-mask pages", || target.draw(|| node.render())).await;
    }
    let full = shadow_mask_cache_stats();
    assert_eq!(full.resident_masks, 4);
    assert_eq!(full.allocated_bytes, 16 * 1024 * 1024);
    next_frame().await;
    let (_, replaced) = draw_until_bake("overwrite oldest large mask", || {
        target.draw(|| last.render())
    })
    .await;
    assert_eq!(replaced.evictions, full.evictions + 1);
    assert_eq!(replaced.resident_masks, full.resident_masks);
    assert_eq!(replaced.allocated_bytes, full.allocated_bytes);

    // No CPU readback between using the old region and writing the new one.
    assert_pixels(
        "earlier frame sampled the old mask",
        &first_oracle,
        &previous.read(),
    );
    assert_pixels(
        "new mask after atlas eviction",
        &last_oracle,
        &target.read(),
    );
    next_frame().await;
    target.draw(|| last.render());
    let hit = shadow_mask_cache_stats();
    assert_eq!(hit.hits, replaced.hits + 1);
    assert_eq!(hit.bakes, replaced.bakes);
    assert_pixels(
        "later frame sampled the replacement mask",
        &last_oracle,
        &target.read(),
    );
}

/// Run once on the live Macroquad render thread with its default camera and no
/// host render pass open. On other backends this is a no-op.
pub(super) async fn check() {
    if !is_vulkan() {
        return;
    }

    clear_shadow_mask_cache();
    let target = Offscreen::new(WIDTH, HEIGHT);
    let probes = TextureProbes::new();
    let analytic = Scene::new(false);
    let cached = Scene::new(true);

    let baseline = shadow_mask_cache_stats();
    assert_eq!(baseline.resident_masks, 0, "shadow cache did not clear");
    target.draw(|| analytic.draw(&probes));
    let oracle = target.read();
    assert_probes(&oracle);
    let disabled = shadow_mask_cache_stats();
    assert_eq!(
        (disabled.bakes, disabled.hits, disabled.fallbacks),
        (baseline.bakes, baseline.hits, baseline.fallbacks),
        "explicitly disabled shadows must bypass the cache"
    );

    target.draw(|| cached.draw(&probes));
    let cold = shadow_mask_cache_stats();
    assert_eq!((cold.bakes, cold.hits), (disabled.bakes, disabled.hits));
    assert_eq!(
        cold.fallbacks,
        disabled.fallbacks + 4,
        "cold masks fell back"
    );
    assert_pixels("cold Vulkan shadows", &oracle, &target.read());

    let (before, a) = draw_until_bake("warm key A", || target.draw(|| cached.a.render())).await;
    assert_eq!(a.bakes, before.bakes + 1);
    assert_eq!(a.resident_masks, 1);

    let (before_b, b) = draw_until_bake("interleaved key B", || {
        target.draw(|| cached.draw(&probes));
    })
    .await;
    assert_eq!(
        b.bakes,
        a.bakes + 1,
        "key B should bake alongside resident A"
    );
    assert_eq!(b.resident_masks, 2);
    assert!(
        b.hits >= before_b.hits + 3,
        "lost a same-frame key A sample: {b:?}"
    );
    // No intervening readback or await between sample A, bake B, and sample A.
    let mixed = target.read();
    assert_probes(&mixed);
    assert_pixels("sample A / bake B / sample A", &oracle, &mixed);

    next_frame().await;
    target.draw(|| cached.draw(&probes));
    let warm = shadow_mask_cache_stats();
    assert_eq!(
        warm.hits,
        b.hits + 4,
        "all translated/rotated shadows should hit"
    );
    assert_eq!((warm.bakes, warm.fallbacks), (b.bakes, b.fallbacks));
    assert_eq!(
        warm.allocated_bytes, b.allocated_bytes,
        "warm draws changed storage"
    );
    assert_pool_bounds(warm);
    assert_pixels("warm Vulkan shadows", &oracle, &target.read());

    target.draw(|| {
        with_scissor((66, 58, 20, 12), || cached.a.render()); // Strictly inside cutout.
        let before = shadow_mask_cache_stats();
        with_scissor((350, 5, 15, 11), || cached.a.render()); // Disjoint from shadow.
        let culled = shadow_mask_cache_stats();
        assert_eq!(culled.culled, before.culled + 1);
        assert_eq!((culled.bakes, culled.hits), (before.bakes, before.hits));
    });
    assert!(
        target
            .read()
            .get_image_data()
            .iter()
            .all(|p| rgba_error(*p, BACKGROUND) <= CHANNEL_TOLERANCE),
        "a completely clipped shadow modified the target"
    );
    clear_shadow_mask_cache();
    let cleared = shadow_mask_cache_stats();
    assert_eq!(cleared.resident_masks, 0);
    assert_eq!(cleared.pending_keys, 0);
    assert_pool_bounds(cleared);
    check_atlas_rewrite().await;
    clear_shadow_mask_cache();
}

/// Mean durations and measured cache work for one optional capture.
#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
struct Timestamps {
    pool: ash::vk::QueryPool,
    mask: u64,
    nanoseconds_per_tick: f64,
}

#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
impl Timestamps {
    fn new() -> Option<Self> {
        use ash::vk;
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context.vulkan_context()?;
        let limits = unsafe {
            ctx.instance
                .get_physical_device_properties(ctx.physical_device)
        }
        .limits;
        let families = unsafe {
            ctx.instance
                .get_physical_device_queue_family_properties(ctx.physical_device)
        };
        let bits = families
            .get(ctx.queue_family_index as usize)?
            .timestamp_valid_bits;
        if bits == 0 {
            return None;
        }
        let info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(2);
        let pool = unsafe { ctx.device.create_query_pool(&info, None) }.ok()?;
        Some(Self {
            pool,
            mask: u64::MAX >> (64 - bits),
            nanoseconds_per_tick: f64::from(limits.timestamp_period),
        })
    }

    fn mark(&self, begin: bool) {
        use ash::vk;
        let mut gl = unsafe { get_internal_gl() };
        gl.flush(); // Close Macroquad's passes before appending raw queries.
        let ctx = gl.quad_context.vulkan_context().unwrap();
        ctx.end_render_pass(); // The automatic window clear may still be open.
        ctx.flush_draw_commands(); // Replay renderer commands before the raw query.
        let cmd = ctx.frame_command_buffer();
        unsafe {
            if begin {
                ctx.device.cmd_reset_query_pool(cmd, self.pool, 0, 2);
                ctx.device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    self.pool,
                    0,
                );
            } else {
                ctx.device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    self.pool,
                    1,
                );
            }
        }
    }

    fn elapsed(&self) -> Duration {
        use ash::vk;
        let ctx = unsafe { get_internal_gl() }
            .quad_context
            .vulkan_context()
            .unwrap();
        let mut ticks = [0u64; 2];
        unsafe {
            ctx.device.get_query_pool_results(
                self.pool,
                0,
                2,
                &mut ticks,
                vk::QueryResultFlags::TYPE_64,
            )
        }
        .expect("shadow GPU timestamps after readback fence");
        let cycles = ticks[1].wrapping_sub(ticks[0]) & self.mask;
        Duration::from_secs_f64(cycles as f64 * self.nanoseconds_per_tick * 1e-9)
    }
}

#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
impl Drop for Timestamps {
    fn drop(&mut self) {
        let ctx = unsafe { get_internal_gl() }
            .quad_context
            .vulkan_context()
            .unwrap();
        // The sample loop has completed its fence before dropping the pool.
        unsafe { ctx.device.destroy_query_pool(self.pool, None) };
    }
}

#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
#[derive(Debug)]
pub(super) struct Profile {
    pub cpu_draw: Duration,
    pub gpu_draw: Option<Duration>,
    pub draw_and_wait: Duration,
    pub hits: u64,
    pub bakes: u64,
}

/// A steady-state A/B on twelve same-style shadows. This keeps draw/clear and
/// synchronization measured separately; the GPU bracket excludes presentation
/// and the one-pixel readback used to retire each pair of timestamps.
#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
pub(super) async fn profile(cache: bool, frames: u32) -> Profile {
    assert!(is_vulkan(), "shadow profiling requires Vulkan");
    assert!(frames > 0, "shadow profiling needs at least one frame");

    clear_shadow_mask_cache();
    let target = Offscreen::new(1024, 768);
    let fence = Offscreen::new(1, 1);
    fence.draw(|| {});
    let panels: Vec<_> = (0..12)
        .map(|i| {
            let x = 66. + (i % 3) as f32 * 325.;
            let y = 45. + (i / 3) as f32 * 180.;
            panel(
                cache,
                [x, y, 180., 106.],
                Shadow::new(18., 15.).color(WARM_COLOR),
                0.88,
                Matrix::IDENTITY,
            )
        })
        .collect();
    let draw = || target.draw(|| panels.iter().for_each(|panel| panel.render()));
    if cache {
        draw_until_bake("profile warm-up", &draw).await;
    }
    // Keep lazy pipeline allocation and the first GPU submission out of the sample.
    for _ in 0..12 {
        draw();
        fence.read();
        next_frame().await;
    }

    let before = shadow_mask_cache_stats();
    let timestamps = Timestamps::new();
    let mut cpu = Duration::ZERO;
    let mut gpu = Duration::ZERO;
    let mut total = Duration::ZERO;
    for _ in 0..frames {
        if let Some(timestamps) = &timestamps {
            timestamps.mark(true);
        }
        let start = Instant::now();
        draw();
        cpu += start.elapsed();
        if let Some(timestamps) = &timestamps {
            timestamps.mark(false);
        }
        fence.read();
        total += start.elapsed();
        if let Some(timestamps) = &timestamps {
            gpu += timestamps.elapsed();
        }
        next_frame().await;
    }
    let after = shadow_mask_cache_stats();
    let result = Profile {
        cpu_draw: cpu / frames,
        gpu_draw: timestamps.as_ref().map(|_| gpu / frames),
        draw_and_wait: total / frames,
        hits: after.hits - before.hits,
        bakes: after.bakes - before.bakes,
    };
    assert_eq!(result.hits, if cache { u64::from(frames) * 12 } else { 0 });
    assert_eq!(result.bakes, 0, "profile included a new bake");
    clear_shadow_mask_cache();
    result
}
