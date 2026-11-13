//! Save and restore macroquad's immediate-draw state around a channel render.
//!
//! A channel may change the active material/pipeline, draw mode, scissor, and
//! viewport. Those are global `QuadGl` state; if the channel does not restore
//! them, the next mue UI draw inherits them. This guard snapshots those four
//! fields and restores them on drop.
//!
//! It deliberately does not save camera/render pass/depth, and does not flush
//! or own a render pass. The channel contract still requires
//! `set_default_camera()` before returning and closing its own pass. Keeping
//! the guard narrow means it cannot steal a host raw miniquad pass.

use macroquad::prelude::{DrawMode, GlPipeline};
use macroquad::window::get_internal_gl;

/// Snapshot of the immediate-draw state a channel may change.
///
/// Restored on drop. Camera, render pass, and depth are intentionally not part
/// of this snapshot; the channel restores those itself.
#[must_use = "the guard restores render state on drop"]
pub struct RenderStateGuard {
    viewport: Option<(i32, i32, i32, i32)>,
    scissor: Option<(i32, i32, i32, i32)>,
    pipeline: Option<GlPipeline>,
    draw_mode: DrawMode,
}

impl RenderStateGuard {
    /// Capture the current material, draw mode, scissor, and viewport.
    pub fn enter() -> Self {
        let gl = unsafe { get_internal_gl() };
        let viewport = gl.quad_gl.get_viewport_raw();
        let scissor = gl.quad_gl.get_scissor();
        let pipeline = gl.quad_gl.get_pipeline();
        let draw_mode = gl.quad_gl.get_draw_mode();
        Self {
            viewport,
            scissor,
            pipeline,
            draw_mode,
        }
    }
}

impl Drop for RenderStateGuard {
    fn drop(&mut self) {
        let gl = unsafe { get_internal_gl() };
        gl.quad_gl.viewport(self.viewport);
        gl.quad_gl.scissor(self.scissor);
        gl.quad_gl.pipeline(self.pipeline);
        gl.quad_gl.draw_mode(self.draw_mode);

        #[cfg(debug_assertions)]
        {
            let gl = unsafe { get_internal_gl() };
            debug_assert_eq!(gl.quad_gl.get_viewport_raw(), self.viewport);
            debug_assert_eq!(gl.quad_gl.get_scissor(), self.scissor);
            debug_assert_eq!(gl.quad_gl.get_pipeline(), self.pipeline);
            debug_assert_eq!(gl.quad_gl.get_draw_mode(), self.draw_mode);
        }
    }
}
