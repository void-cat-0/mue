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
//!
//! A stable host sequence is: render and flush the external target, close any raw
//! pass and restore the default camera, publish its texture, then run
//! [`crate::App::try_frame_with_update`] and await
//! [`macroquad::window::next_frame`]. Do not wrap an App frame in a host
//! `begin_pass`/`end_pass` pair; ordinary frames may flush or switch internal
//! passes even though App does not generally take ownership of host camera/pass
//! state. Scene transition capture/composition has its own narrower OpenGL/full
//! window contract, documented in [`crate::scene_transition`].

use macroquad::prelude::{DrawMode, GlPipeline};
use macroquad::window::get_internal_gl;

/// Snapshot of the immediate-draw state a channel may change.
///
/// Restored on drop on the render thread. The saved pipeline is a non-owning
/// handle: keep its `Material` alive (and its pipeline registered) until after
/// the guard is dropped and any draws using it have been flushed. This also
/// applies to new materials queued inside the scope: the guard does not flush.
///
/// This restores logical `QuadGl` state, not the backend pipeline currently
/// applied on the GPU. In particular, an OpenGL `clear_background` uses the
/// last submitted pipeline's color-write mask, not the logically selected
/// material. Camera, render pass, and depth are left to the caller.
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
