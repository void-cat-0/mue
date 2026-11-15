//! Example-local, GPU-only crossfade of complete, opaque Scene pictures.
//!
//! The host enters with the default camera, no raw miniquad pass, and a balanced
//! model-matrix stack. The viewport is the whole logical window at origin (0, 0);
//! mue clips use those coordinates directly, so cropped/sub-viewport rendering is
//! not supported. All custom/UI draws stay in begin's camera until finish_draw;
//! the host then calls present once. These boundaries
//! flush and leave the default camera, not an arbitrary caller camera/pass.
//! RenderStateGuard restores only material/draw mode/scissor/viewport. Pausing
//! means not calling present; the first host-resume presentation uses dt = 0.

use anyhow::{Result, ensure};
use macroquad::{
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    miniquad::{
        Backend, BlendFactor, BlendState, BlendValue, Equation, PipelineParams, ShaderSource,
    },
    prelude::{
        BLACK, Camera2D, Color, DrawMode, DrawTextureParams, FilterMode, RenderTarget, Texture2D,
        clear_background, draw_texture_ex, render_target, set_camera, set_default_camera, vec2,
    },
    window::{get_internal_gl, screen_dpi_scale},
};
use mue_macroquad::{RenderStateGuard, math::Rect};

pub(crate) struct Presenter {
    targets: Option<Targets>,
    blit_material: Option<Material>,
    clock: BlendClock,
    draw_guard: Option<RenderStateGuard>,
    live_finished: bool,
}

struct Targets {
    live: RenderTarget,
    presented: RenderTarget,
    frozen: RenderTarget,
    viewport: Rect,
    pixels: (u32, u32),
}

impl Targets {
    fn new(viewport: Rect, pixels: (u32, u32)) -> Self {
        let make = || {
            let target = render_target(pixels.0, pixels.1);
            target.texture.set_filter(FilterMode::Linear);
            target
        };
        Self {
            live: make(),
            presented: make(),
            frozen: make(),
            viewport,
            pixels,
        }
    }
}

impl Presenter {
    /// GPU allocation is deferred until the first begin, so construction is pure.
    pub(crate) fn new(duration: f32) -> Self {
        Self {
            targets: None,
            blit_material: None,
            clock: BlendClock::new(duration),
            draw_guard: None,
            live_finished: false,
        }
    }

    /// Start this frame's exclusive live Scene picture in logical window units.
    /// A changed visual key freezes the last *displayed composite*, by swapping
    /// target ownership, without retaining any old Scene or reading GPU pixels.
    pub(crate) fn begin(&mut self, key: u64, viewport: Rect) -> Result<()> {
        ensure!(
            self.draw_guard.is_none(),
            "finish_draw must precede another begin"
        );
        let pixels = target_pixels(viewport, screen_dpi_scale())?;
        if self.blit_material.is_none() {
            ensure!(
                unsafe { get_internal_gl() }.quad_context.info().backend == Backend::OpenGl,
                "scene-flow's presentation material requires OpenGL"
            );
            self.blit_material = Some(make_blit_material()?);
        }
        if self
            .targets
            .as_ref()
            .is_none_or(|targets| targets.viewport != viewport || targets.pixels != pixels)
        {
            self.targets = Some(Targets::new(viewport, pixels));
            self.clock.reset();
        }
        let targets = self.targets.as_mut().unwrap();
        if self.clock.begin(key) {
            std::mem::swap(&mut targets.presented, &mut targets.frozen);
        }

        self.draw_guard = Some(RenderStateGuard::enter());
        self.live_finished = false;
        set_camera(&target_camera(&targets.live, viewport));
        gl_use_default_material();
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.scissor(None);
        gl.draw_mode(DrawMode::Triangles);
        // A complete Scene picture has an opaque base, not transparent layers.
        // set_camera flushed previous work before this immediate target clear.
        clear_background(BLACK);
        Ok(())
    }

    /// Submit all custom/UI live draws before changing cameras. Idempotent.
    pub(crate) fn finish_draw(&mut self) {
        if let Some(guard) = self.draw_guard.take() {
            set_default_camera(); // Also closes any retained UI pass.
            drop(guard);
            self.live_finished = true;
        }
    }

    /// Compose into a separate cache, then draw that cache to the screen.
    /// The first presentation of a new fade is exactly its frozen source (p=0);
    /// subsequent calls integrate finite positive dt. No wall clock is sampled.
    pub(crate) fn present(&mut self, dt: f32, viewport: Rect) -> Result<()> {
        ensure!(
            self.draw_guard.is_none(),
            "finish_draw must precede present"
        );
        ensure!(
            self.live_finished,
            "present requires a freshly finished live draw"
        );
        let pixels = target_pixels(viewport, screen_dpi_scale())?;
        let targets = self.targets.as_ref().unwrap();
        if targets.viewport != viewport || targets.pixels != pixels {
            // A resize between drawing and presentation cannot reuse old pixels.
            // Rebuild lazily on begin; never change Scene input/capture or dt.
            self.reset_resize();
            return Ok(());
        }
        self.clock.advance(dt);
        let progress = self.clock.progress();
        let guard = RenderStateGuard::enter();
        set_camera(&target_camera(&targets.presented, viewport));
        unsafe { get_internal_gl() }.quad_gl.scissor(None);
        clear_background(BLACK);
        gl_use_material(self.blit_material.as_ref().unwrap());
        blit(&targets.live.texture, viewport, 1.);
        if self.clock.is_transitioning() {
            blit(&targets.frozen.texture, viewport, 1. - progress);
        }
        // Never sample the target being written. This also flushes composition.
        set_default_camera();
        blit(&targets.presented.texture, viewport, 1.);
        unsafe { get_internal_gl() }.flush();
        drop(guard);
        self.clock.did_present();
        self.live_finished = false;
        Ok(())
    }

    /// Readiness is latched by a *completed* present, never predicted by layout.
    /// The Scene submits RootInput::None until this is true for its visual key;
    /// Scene raw input remains complete and has its own gameplay policy.
    pub(crate) fn is_ready(&self, key: u64) -> bool {
        self.clock.is_ready(key)
    }

    pub(crate) fn is_transitioning(&self) -> bool {
        self.clock.is_transitioning()
    }

    pub(crate) fn progress(&self) -> f32 {
        self.clock.progress()
    }

    /// Discard only GPU picture caches/fade state; next begin allocates buffers.
    /// No Scene owner, input batch, pointer capture, or business state is touched.
    pub(crate) fn reset_resize(&mut self) {
        self.finish_draw();
        self.targets = None;
        self.live_finished = false;
        self.clock.reset();
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // An early return from Scene rendering must not leave its target active.
        self.finish_draw();
    }
}

fn target_camera(target: &RenderTarget, viewport: Rect) -> Camera2D {
    Camera2D {
        render_target: Some(target.clone()),
        // from_display_rect's negative zoom.y maps logical y down into the GL
        // target, matching mue's top-left scissor coordinates. Do not negate it.
        ..Camera2D::from_display_rect(viewport.into())
    }
}

fn target_pixels(viewport: Rect, dpi: f32) -> Result<(u32, u32)> {
    ensure!(
        viewport.x == 0. && viewport.y == 0.,
        "presentation requires an origin-zero logical window viewport"
    );
    ensure!(
        viewport.x.is_finite()
            && viewport.y.is_finite()
            && viewport.w.is_finite()
            && viewport.h.is_finite()
            && viewport.w > 0.
            && viewport.h > 0.
            && dpi.is_finite()
            && dpi > 0.,
        "presentation requires a finite positive logical viewport and DPI"
    );
    let width = (viewport.w * dpi).round().max(1.);
    let height = (viewport.h * dpi).round().max(1.);
    ensure!(
        width.is_finite()
            && height.is_finite()
            && width < i32::MAX as f32
            && height < i32::MAX as f32,
        "presentation target dimensions overflow"
    );
    Ok((width as u32, height as u32))
}

fn blit(texture: &Texture2D, viewport: Rect, alpha: f32) {
    draw_texture_ex(
        texture,
        viewport.x,
        viewport.y,
        Color::new(1., 1., 1., alpha),
        DrawTextureParams {
            dest_size: Some(vec2(viewport.w, viewport.h)),
            // Both target-to-target and target-to-default-camera copies need
            // this flip for the negative-y logical Camera2D used above.
            flip_y: true,
            ..Default::default()
        },
    );
}

// Default macroquad blending also multiplies destination alpha by source alpha.
// A cached composite would then darken when sampled again, especially on fade
// interruption. Scene pictures are opaque: ignore texture alpha, use only the
// requested overlay alpha, and accumulate alpha independently to keep it 1.
fn make_blit_material() -> Result<Material> {
    Ok(load_material(
        ShaderSource::Glsl {
            vertex: BLIT_VERTEX,
            fragment: BLIT_FRAGMENT,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )?)
}

const BLIT_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
uniform mat4 Model;
uniform mat4 Projection;
varying mediump vec2 uv;
varying lowp float opacity;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
    opacity = color0.a / 255.0;
}
"#;

const BLIT_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
varying mediump vec2 uv;
varying lowp float opacity;
void main() {
    gl_FragColor = vec4(texture2D(Texture, uv).rgb, opacity);
}
"#;

/// GPU-free scheduling/clock helper; true from begin means swap the last
/// presented target into frozen. It holds no Scene, texture, or wall-clock ref.
#[derive(Debug)]
pub(crate) struct BlendClock {
    duration: f32,
    elapsed: f32,
    current_key: Option<u64>,
    presented_key: Option<u64>,
    presented_ready: bool,
    transitioning: bool,
    first_present: bool,
}

impl BlendClock {
    pub(crate) fn new(duration: f32) -> Self {
        Self {
            duration: if duration.is_finite() && duration > 0. {
                duration
            } else {
                0.
            },
            elapsed: 0.,
            current_key: None,
            presented_key: None,
            presented_ready: false,
            transitioning: false,
            first_present: false,
        }
    }

    pub(crate) fn begin(&mut self, key: u64) -> bool {
        if self.current_key == Some(key) {
            return false;
        }
        self.transitioning = self.presented_key.is_some() && self.duration > 0.;
        // If another key arrives before present, frozen already holds the last
        // display. Do not replace it with the unused presented scratch target.
        let freeze = self.transitioning && !self.first_present;
        self.current_key = Some(key);
        self.presented_ready = false;
        self.elapsed = 0.;
        self.first_present = true;
        freeze
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        if self.transitioning && !self.first_present && dt.is_finite() && dt > 0. {
            self.elapsed = (self.elapsed + dt).min(self.duration);
            self.transitioning = self.elapsed < self.duration;
        }
    }

    pub(crate) fn did_present(&mut self) {
        self.presented_key = self.current_key;
        self.presented_ready = self.current_key.is_some() && !self.transitioning;
        self.first_present = false;
    }

    pub(crate) fn is_ready(&self, key: u64) -> bool {
        self.current_key == Some(key)
            && self.presented_key == Some(key)
            && self.presented_ready
            && !self.transitioning
    }

    pub(crate) fn is_transitioning(&self) -> bool {
        self.transitioning
    }

    pub(crate) fn progress(&self) -> f32 {
        if self.transitioning {
            (self.elapsed / self.duration).clamp(0., 1.)
        } else {
            1.
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn displayed(duration: f32, key: u64) -> BlendClock {
        let mut clock = BlendClock::new(duration);
        assert!(!clock.begin(key));
        assert!(!clock.is_ready(key));
        assert!(!clock.is_transitioning());
        clock.advance(100.);
        clock.did_present();
        assert!(clock.is_ready(key));
        clock
    }

    #[test]
    fn first_picture_and_disabled_durations_switch_without_fading() {
        displayed(0.25, 1);
        for duration in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut clock = displayed(duration, 1);
            assert!(!clock.is_ready(2)); // layout before begin of the new key
            assert!(!clock.begin(2));
            assert!(!clock.is_ready(2));
            assert!(!clock.is_transitioning());
            assert_eq!(clock.progress(), 1.);
            clock.did_present();
            assert!(clock.is_ready(2));
            assert!(!clock.is_ready(1));
        }
    }

    #[test]
    fn readiness_waits_for_completed_presentation_not_prediction() {
        let mut clock = displayed(0.25, 1);
        assert!(clock.begin(2));
        clock.advance(10.); // First new display stays exactly at its source.
        assert_eq!(clock.progress(), 0.);
        clock.did_present();
        clock.advance(0.125);
        assert_eq!(clock.progress(), 0.5);
        clock.did_present();
        assert!(!clock.is_ready(2));
        clock.advance(0.125);
        assert_eq!(clock.progress(), 1.);
        assert!(!clock.is_ready(2)); // GPU has not finished this present yet.
        clock.did_present();
        assert!(clock.is_ready(2));
        assert!(!clock.begin(2));
        assert!(clock.is_ready(2));
    }

    #[test]
    fn invalid_dt_and_pause_do_not_advance_the_clock() {
        let mut clock = displayed(1., 1);
        clock.begin(2);
        clock.did_present();
        clock.advance(0.25);
        clock.did_present();
        for dt in [-1., 0., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            clock.advance(dt);
            clock.did_present();
            assert_eq!(clock.progress(), 0.25);
        }
        assert!(!clock.begin(2)); // begin/render itself does not advance time.
        assert_eq!(clock.progress(), 0.25);
        clock.advance(0.); // The host's first resume dt.
        assert_eq!(clock.progress(), 0.25);
        clock.advance(100.);
        clock.did_present();
        assert_eq!(clock.progress(), 1.);
        assert!(clock.is_ready(2));
    }

    #[test]
    fn interruption_freezes_latest_composite_not_original_scene() {
        let mut clock = displayed(1., 1);
        let (mut presented, mut frozen) = (0., -1.);
        if clock.begin(2) {
            std::mem::swap(&mut presented, &mut frozen);
        }
        clock.did_present();
        clock.advance(0.4);
        // Simulate the GPU's live B (1) plus frozen A (0) at p=.4.
        presented = 1. * clock.progress() + frozen * (1. - clock.progress());
        clock.did_present();
        if clock.begin(3) {
            std::mem::swap(&mut presented, &mut frozen);
        }
        assert_eq!(frozen, 0.4); // Not A (0), nor the single live B (1).
        assert_eq!(clock.progress(), 0.);
        assert!(!clock.is_ready(3));
        assert!(!clock.begin(4)); // Still no new presented picture to freeze.
        assert_eq!(frozen, 0.4);
        clock.advance(0.5);
        assert_eq!(clock.progress(), 0.);
        clock.did_present();
        assert!(clock.begin(5)); // Now the next restart swaps the latest display.
    }

    #[test]
    fn resize_discards_readiness_and_transition_without_a_new_fade() {
        let mut clock = displayed(0.25, 1);
        clock.begin(2);
        clock.did_present();
        clock.advance(0.1);
        clock.reset();
        assert!(!clock.is_ready(1));
        assert!(!clock.is_ready(2));
        assert!(!clock.is_transitioning());
        assert!(!clock.begin(2));
        clock.did_present();
        assert!(clock.is_ready(2));
        assert!(clock.begin(3)); // Duration survived the cache reset.
    }

    #[test]
    fn target_size_is_logical_times_dpi_not_window_physical_twice() {
        assert_eq!(
            target_pixels(Rect::new(0., 0., 320., 180.), 2.).unwrap(),
            (640, 360)
        );
        assert_eq!(
            target_pixels(Rect::new(0., 0., 100.25, 50.25), 2.).unwrap(),
            (201, 101)
        );
        assert_eq!(
            target_pixels(Rect::new(0., 0., 0.1, 0.1), 1.).unwrap(),
            (1, 1)
        );
        assert!(target_pixels(Rect::new(5., 0., 10., 10.), 1.).is_err());
        assert!(target_pixels(Rect::new(0., 0., 0., 10.), 1.).is_err());
        assert!(target_pixels(Rect::new(0., 0., 10., 10.), f32::NAN).is_err());
        assert!(target_pixels(Rect::new(0., 0., f32::MAX, 10.), 2.).is_err());
    }
}
