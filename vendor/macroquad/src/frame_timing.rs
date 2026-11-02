//! Opt-in CPU wall timing for backend frame work (the `frame-timing` feature).
//!
//! Native targets use `Instant` and a fixed thread-local accumulator, with no
//! per-frame allocation. This instrumentation does not enable telemetry, issue
//! GPU queries, or add GPU synchronization such as `glFinish`. On wasm32 the
//! guards are inert and [`take`] returns an all-zero snapshot.
//!
//! [`Snapshot::end_frame_ms`] is inclusive of all individually timed end-frame
//! subparts. Do not add those subparts to `end_frame_ms`: they are already
//! included. Render passes flushed earlier by application code
//! are outside these end-frame subparts; their existing behavior is unchanged.
//! These are CPU wall durations, including any driver blocking inside the calls,
//! not GPU execution durations or exact presentation times. Event-loop waiting
//! and presentation work outside the measured methods are not included.
//!
//! To collect the backend work during one `next_frame().await` suspension,
//! discard [`take`] immediately before awaiting, then collect immediately after:
//!
//! ```no_run
//! # async fn example() {
//! use macroquad::{frame_timing, window::next_frame};
//!
//! let _ = frame_timing::take();
//! next_frame().await;
//! let snapshot = frame_timing::take();
//! # }
//! ```
//!
//! This captures the current frame's end and the next frame's begin on the
//! rendering thread. Values accumulate across calls until taken, rather than
//! resetting automatically at a frame boundary.
//!
//! Nested texture and scene stages are inclusive. `quad_draw_*` spans and counters
//! cover every `QuadGl::draw`, including camera-triggered flushes outside the final
//! `draw_submit_ms`. They overlap scene camera set/pop and other enclosing spans;
//! do not sum them with their parents. Texture queue insertion performs no readback;
//! optional atlas building reads textures back through the existing API. Workload
//! counters describe calls/payloads submitted to the backend, not physical GPU work,
//! driver uploads or hardware binds. All counters wrap at `u64::MAX`.

#[cfg(not(target_arch = "wasm32"))]
use std::{cell::Cell, time::Instant};

/// Accumulated CPU wall durations in milliseconds on the calling thread.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Snapshot {
    /// Entire `Context::begin_frame` call.
    pub begin_frame_ms: f64,
    /// Entire `Context::end_frame` call, including the subparts below.
    pub end_frame_ms: f64,
    /// The `perform_render_passes` flush performed inside `end_frame`.
    pub render_passes_ms: f64,
    /// UI drawing inside `end_frame`.
    pub ui_draw_ms: f64,
    /// Final `gl.draw` submission inside `end_frame` (after UI drawing).
    pub draw_submit_ms: f64,
    /// Backend `commit_frame` call; not an exact presentation measurement.
    pub commit_ms: f64,
    /// `experimental::scene::update` inside `end_frame`.
    pub scene_update_ms: f64,
    /// Pixel-perfect projection matrix calculation inside `end_frame`.
    pub projection_ms: f64,
    /// `telemetry::end_gpu_query` CPU wall time; not GPU execution time.
    pub gpu_query_ms: f64,
    /// Input event and mouse-position resets inside `end_frame`.
    pub input_reset_ms: f64,
    /// Texture garbage collection inside `end_frame`.
    pub texture_gc_ms: f64,
    /// Touch state and dropped-file cleanup inside `end_frame`.
    pub touch_cleanup_ms: f64,
    /// Image file decode and conversion to RGBA8.
    pub image_decode_ms: f64,
    /// from_rgba8 called by from_file_with_format only, including all nested setup.
    pub image_upload_ms: f64,
    /// Successfully decoded images loaded through `Texture2D::from_file_with_format`.
    pub image_load_count: u64,
    /// Cumulative RGBA8 bytes produced by those successful image decodes.
    pub image_decoded_bytes: u64,
    /// All Texture2D::from_rgba8 calls, including setup and queue insertion.
    pub texture_from_rgba8_ms: f64,
    /// Backend texture allocation calls, including from_rgba8 and empty textures.
    pub texture_new_ms: f64,
    /// Managed texture storage inside from_rgba8.
    pub texture_store_ms: f64,
    /// Default filter setup inside from_rgba8.
    pub texture_filter_ms: f64,
    /// Weak-handle queue insertion inside from_rgba8; no readback.
    pub texture_batcher_add_ms: f64,
    /// All get_texture_data calls, including allocation and existing readback.
    pub texture_readback_ms: f64,
    /// Zeroed RGBA8 Image allocation inside get_texture_data.
    pub texture_readback_alloc_ms: f64,
    /// Existing texture_read_pixels CPU wall time, including driver blocking.
    pub texture_readback_gpu_ms: f64,
    /// Optional build_textures_atlas, including readback and sprite caching.
    pub texture_atlas_build_ms: f64,
    /// Sprite caching inside build_textures_atlas.
    pub texture_atlas_cache_ms: f64,
    /// Scene ready/update/fixed-update loops and time bookkeeping.
    pub scene_node_logic_ms: f64,
    /// Scene node draw loops, accumulated across cameras.
    pub scene_node_draw_ms: f64,
    /// Scene camera push/set, including the existing rendering flush.
    pub scene_camera_set_ms: f64,
    /// Scene camera pop, including the existing rendering flush.
    pub scene_camera_pop_ms: f64,
    /// Scene pending dense-node insertion/removal bookkeeping.
    pub scene_bookkeeping_ms: f64,
    /// All QuadGl::draw calls, including camera flushes and empty calls.
    pub quad_draw_ms: f64,
    /// Stream buffer creation and bindings allocation inside QuadGl::draw.
    pub quad_draw_buffer_create_ms: f64,
    /// Render pass begin/end calls inside QuadGl::draw.
    pub quad_draw_pass_ms: f64,
    /// Vertex/index buffer_update calls inside QuadGl::draw.
    pub quad_draw_upload_ms: f64,
    /// Image binding preparation and pipeline/viewport/scissor/bindings application.
    pub quad_draw_bindings_ms: f64,
    /// Uniform preparation and backend application inside QuadGl::draw.
    pub quad_draw_uniforms_ms: f64,
    /// Backend draw CPU wall time; not GPU execution time.
    pub quad_draw_backend_ms: f64,
    /// Existing screen-texture snapshot requests inside QuadGl::draw.
    pub quad_draw_snapshot_ms: f64,
    /// Existing optional telemetry drawcall capture inside QuadGl::draw.
    pub quad_draw_capture_ms: f64,
    /// Number of from_rgba8 calls.
    pub texture_from_rgba8_count: u64,
    /// Total input slice lengths passed to from_rgba8.
    pub texture_from_rgba8_bytes: u64,
    /// Number of get_texture_data calls.
    pub texture_readback_count: u64,
    /// Total zeroed RGBA8 allocation bytes in get_texture_data.
    pub texture_readback_bytes: u64,
    /// Number of QuadGl::draw invocations, including empty calls.
    pub quad_draw_invocations: u64,
    /// Number of backend draw calls submitted by QuadGl::draw.
    pub quad_draw_calls: u64,
    /// Sum of vertices submitted by QuadGl::draw.
    pub quad_draw_vertices: u64,
    /// Sum of indices submitted by QuadGl::draw.
    pub quad_draw_indices: u64,
    /// Number of vertex/index buffer_update calls (two per draw call).
    pub quad_draw_buffer_uploads: u64,
    /// Vertex/index buffer payload bytes, using size_of::<Vertex/u16>().
    pub quad_draw_buffer_upload_bytes: u64,
    /// Number of stream vertex/index buffers created by QuadGl::draw.
    pub quad_draw_buffers_created: u64,
    /// Number of render passes begun by QuadGl::draw.
    pub quad_draw_render_passes: u64,
    /// Number of screen-texture snapshot requests by QuadGl::draw.
    pub quad_draw_screen_snapshots: u64,
}

#[cfg(not(target_arch = "wasm32"))]
std::thread_local! {
    static ACCUMULATOR: Cell<Snapshot> = Cell::new(Snapshot::default());
}

/// Returns the calling thread's accumulated timings and resets them to zero.
///
/// Call on the rendering thread. On wasm32 this always returns zeroes.
pub fn take() -> Snapshot {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ACCUMULATOR.with(|accumulator| accumulator.take())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Snapshot::default()
    }
}

#[cfg(feature = "frame-timing")]
pub(crate) fn record_image_load(decoded_bytes: usize) {
    #[cfg(not(target_arch = "wasm32"))]
    ACCUMULATOR.with(|accumulator| {
        let mut snapshot = accumulator.get();
        snapshot.image_load_count = snapshot.image_load_count.wrapping_add(1);
        snapshot.image_decoded_bytes = snapshot
            .image_decoded_bytes
            .wrapping_add(decoded_bytes as u64);
        accumulator.set(snapshot);
    });
}

/// Workload submitted to the backend; not a hardware performance counter.
pub(crate) enum Counter {
    TextureFromRgba8Count,
    TextureFromRgba8Bytes,
    TextureReadbackCount,
    TextureReadbackBytes,
    QuadDrawInvocations,
    QuadDrawCalls,
    QuadDrawVertices,
    QuadDrawIndices,
    QuadDrawBufferUploads,
    QuadDrawBufferUploadBytes,
    QuadDrawBuffersCreated,
    QuadDrawRenderPasses,
    QuadDrawScreenSnapshots,
}

pub(crate) fn record_counter(counter: Counter, amount: u64) {
    #[cfg(not(target_arch = "wasm32"))]
    ACCUMULATOR.with(|accumulator| {
        let mut snapshot = accumulator.get();
        let value = match counter {
            Counter::TextureFromRgba8Count => &mut snapshot.texture_from_rgba8_count,
            Counter::TextureFromRgba8Bytes => &mut snapshot.texture_from_rgba8_bytes,
            Counter::TextureReadbackCount => &mut snapshot.texture_readback_count,
            Counter::TextureReadbackBytes => &mut snapshot.texture_readback_bytes,
            Counter::QuadDrawInvocations => &mut snapshot.quad_draw_invocations,
            Counter::QuadDrawCalls => &mut snapshot.quad_draw_calls,
            Counter::QuadDrawVertices => &mut snapshot.quad_draw_vertices,
            Counter::QuadDrawIndices => &mut snapshot.quad_draw_indices,
            Counter::QuadDrawBufferUploads => &mut snapshot.quad_draw_buffer_uploads,
            Counter::QuadDrawBufferUploadBytes => &mut snapshot.quad_draw_buffer_upload_bytes,
            Counter::QuadDrawBuffersCreated => &mut snapshot.quad_draw_buffers_created,
            Counter::QuadDrawRenderPasses => &mut snapshot.quad_draw_render_passes,
            Counter::QuadDrawScreenSnapshots => &mut snapshot.quad_draw_screen_snapshots,
        };
        *value = value.wrapping_add(amount);
        accumulator.set(snapshot);
    });
    #[cfg(target_arch = "wasm32")]
    let _ = (counter, amount);
}

pub(crate) enum Stage {
    BeginFrame,
    EndFrame,
    RenderPasses,
    UiDraw,
    DrawSubmit,
    Commit,
    SceneUpdate,
    Projection,
    GpuQuery,
    InputReset,
    TextureGc,
    TouchCleanup,
    ImageDecode,
    ImageUpload,
    TextureFromRgba8,
    TextureNew,
    TextureStore,
    TextureFilter,
    TextureBatcherAdd,
    TextureReadback,
    TextureReadbackAlloc,
    TextureReadbackGpu,
    TextureAtlasBuild,
    TextureAtlasCache,
    SceneNodeLogic,
    SceneNodeDraw,
    SceneCameraSet,
    SceneCameraPop,
    SceneBookkeeping,
    QuadDraw,
    QuadDrawBufferCreate,
    QuadDrawPass,
    QuadDrawUpload,
    QuadDrawBindings,
    QuadDrawUniforms,
    QuadDrawBackend,
    QuadDrawSnapshot,
    QuadDrawCapture,
}

pub(crate) struct Span {
    #[cfg(not(target_arch = "wasm32"))]
    stage: Stage,
    #[cfg(not(target_arch = "wasm32"))]
    start: Instant,
}

impl Span {
    pub(crate) fn new(stage: Stage) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            stage,
            #[cfg(not(target_arch = "wasm32"))]
            start: Instant::now(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
            ACCUMULATOR.with(|accumulator| {
                let mut snapshot = accumulator.get();
                let duration = match self.stage {
                    Stage::BeginFrame => &mut snapshot.begin_frame_ms,
                    Stage::EndFrame => &mut snapshot.end_frame_ms,
                    Stage::RenderPasses => &mut snapshot.render_passes_ms,
                    Stage::UiDraw => &mut snapshot.ui_draw_ms,
                    Stage::DrawSubmit => &mut snapshot.draw_submit_ms,
                    Stage::Commit => &mut snapshot.commit_ms,
                    Stage::SceneUpdate => &mut snapshot.scene_update_ms,
                    Stage::Projection => &mut snapshot.projection_ms,
                    Stage::GpuQuery => &mut snapshot.gpu_query_ms,
                    Stage::InputReset => &mut snapshot.input_reset_ms,
                    Stage::TextureGc => &mut snapshot.texture_gc_ms,
                    Stage::TouchCleanup => &mut snapshot.touch_cleanup_ms,
                    Stage::ImageDecode => &mut snapshot.image_decode_ms,
                    Stage::ImageUpload => &mut snapshot.image_upload_ms,
                    Stage::TextureFromRgba8 => &mut snapshot.texture_from_rgba8_ms,
                    Stage::TextureNew => &mut snapshot.texture_new_ms,
                    Stage::TextureStore => &mut snapshot.texture_store_ms,
                    Stage::TextureFilter => &mut snapshot.texture_filter_ms,
                    Stage::TextureBatcherAdd => &mut snapshot.texture_batcher_add_ms,
                    Stage::TextureReadback => &mut snapshot.texture_readback_ms,
                    Stage::TextureReadbackAlloc => &mut snapshot.texture_readback_alloc_ms,
                    Stage::TextureReadbackGpu => &mut snapshot.texture_readback_gpu_ms,
                    Stage::TextureAtlasBuild => &mut snapshot.texture_atlas_build_ms,
                    Stage::TextureAtlasCache => &mut snapshot.texture_atlas_cache_ms,
                    Stage::SceneNodeLogic => &mut snapshot.scene_node_logic_ms,
                    Stage::SceneNodeDraw => &mut snapshot.scene_node_draw_ms,
                    Stage::SceneCameraSet => &mut snapshot.scene_camera_set_ms,
                    Stage::SceneCameraPop => &mut snapshot.scene_camera_pop_ms,
                    Stage::SceneBookkeeping => &mut snapshot.scene_bookkeeping_ms,
                    Stage::QuadDraw => &mut snapshot.quad_draw_ms,
                    Stage::QuadDrawBufferCreate => &mut snapshot.quad_draw_buffer_create_ms,
                    Stage::QuadDrawPass => &mut snapshot.quad_draw_pass_ms,
                    Stage::QuadDrawUpload => &mut snapshot.quad_draw_upload_ms,
                    Stage::QuadDrawBindings => &mut snapshot.quad_draw_bindings_ms,
                    Stage::QuadDrawUniforms => &mut snapshot.quad_draw_uniforms_ms,
                    Stage::QuadDrawBackend => &mut snapshot.quad_draw_backend_ms,
                    Stage::QuadDrawSnapshot => &mut snapshot.quad_draw_snapshot_ms,
                    Stage::QuadDrawCapture => &mut snapshot.quad_draw_capture_ms,
                };
                *duration += elapsed_ms;
                accumulator.set(snapshot);
            });
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{Span, Stage, take};
    use std::time::Duration;

    #[test]
    fn nested_spans_accumulate_and_take_resets_all_fields() {
        let _ = take();
        #[cfg(feature = "frame-timing")]
        {
            super::record_image_load(16);
            super::record_image_load(32);
        }
        {
            let _outer = Span::new(Stage::EndFrame);
            {
                let _inner = Span::new(Stage::SceneUpdate);
                std::thread::sleep(Duration::from_millis(1));
            }
            {
                let _inner = Span::new(Stage::Projection);
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        let first = take();
        assert!(first.end_frame_ms >= first.scene_update_ms);
        assert!(first.end_frame_ms >= first.projection_ms);
        assert!(first.scene_update_ms > 0.0);
        assert!(first.projection_ms > 0.0);
        #[cfg(feature = "frame-timing")]
        {
            assert_eq!(first.image_load_count, 2);
            assert_eq!(first.image_decoded_bytes, 48);
        }
        let reset = take();
        assert_eq!(reset.begin_frame_ms, 0.0);
        assert_eq!(reset.end_frame_ms, 0.0);
        assert_eq!(reset.render_passes_ms, 0.0);
        assert_eq!(reset.ui_draw_ms, 0.0);
        assert_eq!(reset.draw_submit_ms, 0.0);
        assert_eq!(reset.commit_ms, 0.0);
        assert_eq!(reset.scene_update_ms, 0.0);
        assert_eq!(reset.projection_ms, 0.0);
        assert_eq!(reset.gpu_query_ms, 0.0);
        assert_eq!(reset.input_reset_ms, 0.0);
        assert_eq!(reset.texture_gc_ms, 0.0);
        assert_eq!(reset.touch_cleanup_ms, 0.0);
        assert_eq!(reset.image_decode_ms, 0.0);
        assert_eq!(reset.image_upload_ms, 0.0);
        assert_eq!(reset.image_load_count, 0);
        assert_eq!(reset.image_decoded_bytes, 0);
    }

    #[test]
    fn finer_stages_and_counters_accumulate_and_reset() {
        let _ = take();
        drop(Span::new(Stage::TextureFromRgba8));
        drop(Span::new(Stage::TextureNew));
        drop(Span::new(Stage::TextureStore));
        drop(Span::new(Stage::TextureFilter));
        drop(Span::new(Stage::TextureBatcherAdd));
        drop(Span::new(Stage::TextureReadback));
        drop(Span::new(Stage::TextureReadbackAlloc));
        drop(Span::new(Stage::TextureReadbackGpu));
        drop(Span::new(Stage::TextureAtlasBuild));
        drop(Span::new(Stage::TextureAtlasCache));
        drop(Span::new(Stage::SceneNodeLogic));
        drop(Span::new(Stage::SceneNodeDraw));
        drop(Span::new(Stage::SceneCameraSet));
        drop(Span::new(Stage::SceneCameraPop));
        drop(Span::new(Stage::SceneBookkeeping));
        drop(Span::new(Stage::QuadDraw));
        drop(Span::new(Stage::QuadDrawBufferCreate));
        drop(Span::new(Stage::QuadDrawPass));
        drop(Span::new(Stage::QuadDrawUpload));
        drop(Span::new(Stage::QuadDrawBindings));
        drop(Span::new(Stage::QuadDrawUniforms));
        drop(Span::new(Stage::QuadDrawBackend));
        drop(Span::new(Stage::QuadDrawSnapshot));
        drop(Span::new(Stage::QuadDrawCapture));
        super::record_counter(super::Counter::TextureFromRgba8Count, 2);
        super::record_counter(super::Counter::TextureFromRgba8Count, 3);
        super::record_counter(super::Counter::TextureFromRgba8Bytes, 2);
        super::record_counter(super::Counter::TextureFromRgba8Bytes, 3);
        super::record_counter(super::Counter::TextureReadbackCount, 2);
        super::record_counter(super::Counter::TextureReadbackCount, 3);
        super::record_counter(super::Counter::TextureReadbackBytes, 2);
        super::record_counter(super::Counter::TextureReadbackBytes, 3);
        super::record_counter(super::Counter::QuadDrawInvocations, 2);
        super::record_counter(super::Counter::QuadDrawInvocations, 3);
        super::record_counter(super::Counter::QuadDrawCalls, 2);
        super::record_counter(super::Counter::QuadDrawCalls, 3);
        super::record_counter(super::Counter::QuadDrawVertices, 2);
        super::record_counter(super::Counter::QuadDrawVertices, 3);
        super::record_counter(super::Counter::QuadDrawIndices, 2);
        super::record_counter(super::Counter::QuadDrawIndices, 3);
        super::record_counter(super::Counter::QuadDrawBufferUploads, 2);
        super::record_counter(super::Counter::QuadDrawBufferUploads, 3);
        super::record_counter(super::Counter::QuadDrawBufferUploadBytes, 2);
        super::record_counter(super::Counter::QuadDrawBufferUploadBytes, 3);
        super::record_counter(super::Counter::QuadDrawBuffersCreated, 2);
        super::record_counter(super::Counter::QuadDrawBuffersCreated, 3);
        super::record_counter(super::Counter::QuadDrawRenderPasses, 2);
        super::record_counter(super::Counter::QuadDrawRenderPasses, 3);
        super::record_counter(super::Counter::QuadDrawScreenSnapshots, 2);
        super::record_counter(super::Counter::QuadDrawScreenSnapshots, 3);
        let snapshot = take();
        assert!(
            snapshot.texture_from_rgba8_ms > 0.0,
            "texture_from_rgba8_ms"
        );
        assert!(snapshot.texture_new_ms > 0.0, "texture_new_ms");
        assert!(snapshot.texture_store_ms > 0.0, "texture_store_ms");
        assert!(snapshot.texture_filter_ms > 0.0, "texture_filter_ms");
        assert!(
            snapshot.texture_batcher_add_ms > 0.0,
            "texture_batcher_add_ms"
        );
        assert!(snapshot.texture_readback_ms > 0.0, "texture_readback_ms");
        assert!(
            snapshot.texture_readback_alloc_ms > 0.0,
            "texture_readback_alloc_ms"
        );
        assert!(
            snapshot.texture_readback_gpu_ms > 0.0,
            "texture_readback_gpu_ms"
        );
        assert!(
            snapshot.texture_atlas_build_ms > 0.0,
            "texture_atlas_build_ms"
        );
        assert!(
            snapshot.texture_atlas_cache_ms > 0.0,
            "texture_atlas_cache_ms"
        );
        assert!(snapshot.scene_node_logic_ms > 0.0, "scene_node_logic_ms");
        assert!(snapshot.scene_node_draw_ms > 0.0, "scene_node_draw_ms");
        assert!(snapshot.scene_camera_set_ms > 0.0, "scene_camera_set_ms");
        assert!(snapshot.scene_camera_pop_ms > 0.0, "scene_camera_pop_ms");
        assert!(snapshot.scene_bookkeeping_ms > 0.0, "scene_bookkeeping_ms");
        assert!(snapshot.quad_draw_ms > 0.0, "quad_draw_ms");
        assert!(
            snapshot.quad_draw_buffer_create_ms > 0.0,
            "quad_draw_buffer_create_ms"
        );
        assert!(snapshot.quad_draw_pass_ms > 0.0, "quad_draw_pass_ms");
        assert!(snapshot.quad_draw_upload_ms > 0.0, "quad_draw_upload_ms");
        assert!(
            snapshot.quad_draw_bindings_ms > 0.0,
            "quad_draw_bindings_ms"
        );
        assert!(
            snapshot.quad_draw_uniforms_ms > 0.0,
            "quad_draw_uniforms_ms"
        );
        assert!(snapshot.quad_draw_backend_ms > 0.0, "quad_draw_backend_ms");
        assert!(
            snapshot.quad_draw_snapshot_ms > 0.0,
            "quad_draw_snapshot_ms"
        );
        assert!(snapshot.quad_draw_capture_ms > 0.0, "quad_draw_capture_ms");
        assert_eq!(
            snapshot.texture_from_rgba8_count, 5,
            "texture_from_rgba8_count"
        );
        assert_eq!(
            snapshot.texture_from_rgba8_bytes, 5,
            "texture_from_rgba8_bytes"
        );
        assert_eq!(snapshot.texture_readback_count, 5, "texture_readback_count");
        assert_eq!(snapshot.texture_readback_bytes, 5, "texture_readback_bytes");
        assert_eq!(snapshot.quad_draw_invocations, 5, "quad_draw_invocations");
        assert_eq!(snapshot.quad_draw_calls, 5, "quad_draw_calls");
        assert_eq!(snapshot.quad_draw_vertices, 5, "quad_draw_vertices");
        assert_eq!(snapshot.quad_draw_indices, 5, "quad_draw_indices");
        assert_eq!(
            snapshot.quad_draw_buffer_uploads, 5,
            "quad_draw_buffer_uploads"
        );
        assert_eq!(
            snapshot.quad_draw_buffer_upload_bytes, 5,
            "quad_draw_buffer_upload_bytes"
        );
        assert_eq!(
            snapshot.quad_draw_buffers_created, 5,
            "quad_draw_buffers_created"
        );
        assert_eq!(
            snapshot.quad_draw_render_passes, 5,
            "quad_draw_render_passes"
        );
        assert_eq!(
            snapshot.quad_draw_screen_snapshots, 5,
            "quad_draw_screen_snapshots"
        );
        assert_eq!(take(), super::Snapshot::default());
    }

    #[test]
    fn workload_counters_wrap_and_are_thread_local() {
        let _ = take();
        super::record_counter(super::Counter::QuadDrawCalls, u64::MAX);
        super::record_counter(super::Counter::QuadDrawCalls, 2);
        std::thread::spawn(|| {
            assert_eq!(take(), super::Snapshot::default());
            super::record_counter(super::Counter::QuadDrawCalls, 7);
            assert_eq!(take().quad_draw_calls, 7);
        })
        .join()
        .unwrap();
        assert_eq!(take().quad_draw_calls, 1);
        assert_eq!(take(), super::Snapshot::default());
    }
}
