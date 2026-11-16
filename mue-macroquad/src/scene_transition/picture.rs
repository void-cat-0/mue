//! Exclusive picture passes. This cache owns no Scene and no transition clock.

use anyhow::{Context, Result, ensure};
use macroquad::{
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    miniquad::{
        Backend, BlendFactor, BlendState, Equation, PipelineParams, ShaderSource, UniformDesc,
        UniformType,
    },
    prelude::{
        BLACK, Camera2D, DrawMode, DrawTextureParams, FilterMode, RenderTarget, Texture2D, WHITE,
        clear_background, draw_rectangle, draw_texture_ex, render_target, set_camera,
        set_default_camera, vec2,
    },
    window::{get_internal_gl, screen_dpi_scale},
};

use crate::{math::Rect, render::RenderStateGuard};

use super::{Draw, ScenePictures, SceneTransition, TransitionFrame};

/// Source, live and last successfully presented composite are separate targets.
/// All begin/finish boundaries require exclusive use of macroquad drawing.
/// Capture callbacks keep the supplied target camera and a balanced model stack.
pub(crate) struct PictureCache {
    key: Option<PictureKey>,
    targets: Option<Targets>,
    copy_material: Option<Material>,
    opaque_material: Option<Material>,
    crossfade_material: Option<Material>,
    boundary: Option<DrawBoundary>,
    drawing: Option<Capture>,
    source_ready: bool,
    live_ready: bool,
    presented_ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PictureKey {
    viewport: Rect,
    pixels: (u32, u32),
}

struct Targets {
    from: RenderTarget,
    to: RenderTarget,
    presented: RenderTarget,
}

impl Targets {
    fn new(pixels: (u32, u32)) -> Self {
        let make = || {
            let target = render_target(pixels.0, pixels.1);
            target.texture.set_filter(FilterMode::Linear);
            target
        };
        Self {
            from: make(),
            to: make(),
            presented: make(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Source,
    Live,
}

impl PictureCache {
    /// No textures, shader compilation, window query, or GL state access.
    pub(crate) fn new() -> Self {
        Self {
            key: None,
            targets: None,
            copy_material: None,
            opaque_material: None,
            crossfade_material: None,
            boundary: None,
            drawing: None,
            source_ready: false,
            live_ready: false,
            presented_ready: false,
        }
    }

    pub(crate) fn needs_source(&self, viewport: Rect) -> Result<bool> {
        Ok(self.key != Some(self.picture_key(viewport)?) || !self.source_ready)
    }

    /// App renders the departing Scene once in this pass, without a Scene/Node
    /// update, before releasing its owner.
    pub(crate) fn begin_source(&mut self, viewport: Rect) -> Result<()> {
        self.ensure_idle()?;
        self.prepare_targets(viewport)?;
        self.source_ready = false;
        self.live_ready = false;
        self.presented_ready = false;
        self.drawing = Some(Capture::Source);
        self.boundary = Some(DrawBoundary::target(
            &self.targets.as_ref().unwrap().from,
            viewport,
        ));
        // A complete Scene picture always has an opaque base. The camera change
        // flushed pending work before this immediate clear.
        clear_background(BLACK);
        Ok(())
    }

    pub(crate) fn finish_source(&mut self) {
        if self.drawing == Some(Capture::Source) {
            self.make_opaque(self.key.unwrap().viewport);
            self.finish();
            self.source_ready = true;
        }
    }

    /// App draws the current Scene/UI once into the live target each frame.
    pub(crate) fn begin_live(&mut self, viewport: Rect) -> Result<()> {
        self.ensure_idle()?;
        self.prepare_targets(viewport)?;
        self.live_ready = false;
        self.drawing = Some(Capture::Live);
        self.boundary = Some(DrawBoundary::target(
            &self.targets.as_ref().unwrap().to,
            viewport,
        ));
        clear_background(BLACK);
        Ok(())
    }

    pub(crate) fn finish_live(&mut self) {
        if self.drawing == Some(Capture::Live) {
            self.make_opaque(self.key.unwrap().viewport);
            self.finish();
            self.live_ready = true;
        }
    }

    /// Compose into the third target, flush, then copy it to the default pass.
    /// Returning Ok acknowledges actual submission; only then may App call the
    /// phase clock's `did_present`. Errors never expose a partial composite.
    pub(crate) fn compose(
        &mut self,
        frame: TransitionFrame,
        transition: &mut SceneTransition,
    ) -> Result<()> {
        self.ensure_idle()?;
        ensure!(
            transition.is_composite(),
            "an overlay does not require picture composition"
        );
        ensure!(
            self.live_ready,
            "composition requires a freshly finished live picture"
        );
        if self.key != Some(self.picture_key(frame.viewport)?) {
            self.invalidate();
            anyhow::bail!("the viewport changed between live drawing and composition");
        }
        self.live_ready = false;
        // The output is about to be overwritten, so even a failed callback must
        // not let a later interruption freeze this partially drawn scratch.
        self.presented_ready = false;
        let targets = self.targets.as_ref().unwrap();
        let pictures = ScenePictures {
            from: if self.source_ready {
                &targets.from.texture
            } else {
                // Resize cannot reconstruct a released Scene. Keep scheduling
                // time and use live for both inputs, never sample the output.
                &targets.to.texture
            },
            to: &targets.to.texture,
        };
        let boundary = DrawBoundary::target(&targets.presented, frame.viewport);
        clear_background(BLACK);
        match &mut transition.draw {
            Draw::Crossfade => {
                // A fresh request needs a fresh clock, not a fresh GPU pipeline.
                if self.crossfade_material.is_none() {
                    self.crossfade_material = Some(make_crossfade_material()?);
                }
                crossfade(self.crossfade_material.as_ref().unwrap(), frame, pictures);
            }
            Draw::Composite(draw) => draw(frame, pictures)?,
            Draw::Overlay(_) => unreachable!("composite kind checked above"),
        }
        // Custom blending may reduce alpha even over the opaque base. Frozen
        // composites must obey the same complete-picture contract as Scene draws.
        self.make_opaque(frame.viewport);
        // This camera change submits all custom composition before sampling it.
        set_default_camera();
        reset_draw_state();
        gl_use_material(self.copy_material.as_ref().unwrap());
        blit(&targets.presented.texture, frame.viewport);
        // Drop flushes the screen copy, closes retained work and restores only
        // RenderStateGuard's material/mode/scissor/viewport responsibilities.
        drop(boundary);
        self.presented_ready = true;
        Ok(())
    }

    /// Normal overlays use the default camera but allocate no picture resources.
    pub(crate) fn draw_overlay(
        &mut self,
        frame: TransitionFrame,
        transition: &mut SceneTransition,
    ) -> Result<()> {
        self.ensure_idle()?;
        ensure!(
            !transition.is_composite(),
            "a composite requires Scene pictures"
        );
        let boundary = DrawBoundary::default_camera();
        let result = transition.draw_overlay(frame);
        drop(boundary); // Flush/restore on Ok and Err before App acknowledges.
        result
    }

    /// Reuse only the last successfully displayed composite on interruption.
    /// Swapping ownership needs no copy, readback, Scene reference or clock reset.
    /// An overlay is not captured, so no arbitrary overlay continuity is promised.
    pub(crate) fn freeze_presented(&mut self, viewport: Rect) -> Result<bool> {
        self.ensure_idle()?;
        // Reusing old pixels is optional; a temporarily unavailable surface only
        // invalidates the cache. Actual capture still validates drawable sizes.
        let Ok(key) = self.picture_key(viewport) else {
            self.invalidate();
            return Ok(false);
        };
        if self.key != Some(key) {
            self.invalidate();
            return Ok(false);
        }
        if !self.presented_ready {
            // Repeated interruptions before a new drawing keep the same source.
            return Ok(self.source_ready);
        }
        if let Some(targets) = self.targets.as_mut() {
            std::mem::swap(&mut targets.from, &mut targets.presented);
        }
        self.source_ready = true;
        self.live_ready = false;
        // A second unpresented interruption must not swap the unused scratch.
        self.presented_ready = false;
        Ok(true)
    }

    /// Close pending normal Scene draws on an error before App faults/drops an
    /// effect. Ordinary Scene drawing is not owned by a picture boundary, so an
    /// early return may otherwise leave immediate or retained work queued.
    ///
    /// This restores the default camera, not an arbitrary caller raw pass or
    /// material snapshot. Captures still finish/abort their RenderStateGuard
    /// boundary separately. The strict no-raw-pass/balanced-model-stack contract
    /// applies here too.
    pub(crate) fn cleanup_scene_error(&mut self) {
        set_default_camera();
        unsafe { get_internal_gl() }.flush();
        // Do not leave a selected Scene-owned pipeline behind after its
        // owner is dropped by explicit recovery. Old queued draws ran first.
        reset_draw_state();
    }

    fn make_opaque(&self, viewport: Rect) {
        // Preserve RGB. Default alpha blending also blends the alpha channel,
        // so a BLACK clear alone cannot make these sampled pictures opaque.
        unsafe { get_internal_gl() }.flush();
        reset_draw_state();
        gl_use_material(self.opaque_material.as_ref().unwrap());
        draw_rectangle(viewport.x, viewport.y, viewport.w, viewport.h, WHITE);
    }

    /// Abort an unfinished capture and flush/restore its owned camera boundary.
    /// Idempotent and GPU-free when the cache has not opened a draw boundary.
    /// Call `finish_source`/`finish_live` only to commit a successful capture.
    pub(crate) fn finish(&mut self) {
        self.drawing = None;
        drop(self.boundary.take());
    }

    /// Discard pictures only. The phase clock belongs to App and is untouched.
    pub(crate) fn invalidate(&mut self) {
        self.finish();
        self.key = None;
        self.targets = None;
        self.source_ready = false;
        self.live_ready = false;
        self.presented_ready = false;
    }

    fn ensure_idle(&self) -> Result<()> {
        ensure!(
            self.drawing.is_none(),
            "finish the active picture capture before drawing again"
        );
        Ok(())
    }

    fn picture_key(&self, viewport: Rect) -> Result<PictureKey> {
        let dpi = screen_dpi_scale();
        Ok(PictureKey {
            viewport,
            pixels: target_pixels(viewport, dpi)?,
        })
    }

    fn prepare_targets(&mut self, viewport: Rect) -> Result<()> {
        let key = self.picture_key(viewport)?;
        if self.key != Some(key) {
            self.invalidate();
            self.key = Some(key);
        }
        // Verify backend/compile before allocating any picture. Metal is an
        // explicit error, not a GLSL shader panic on the vendor Metal path.
        if self.copy_material.is_none() {
            self.copy_material = Some(make_copy_material()?);
        }
        if self.opaque_material.is_none() {
            self.opaque_material = Some(make_opaque_material()?);
        }
        if self.targets.is_none() {
            self.targets = Some(Targets::new(key.pixels));
        }
        Ok(())
    }
}

// If App returns early or unwinds during a capture, the boundary still submits
// pending draws and leaves the default camera before restoring its four states.
impl Drop for PictureCache {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Camera/pass ownership is intentionally separate from RenderStateGuard.
/// We own macroquad camera boundaries, never a caller's arbitrary raw pass.
struct DrawBoundary {
    _state: RenderStateGuard,
}

impl DrawBoundary {
    fn target(target: &RenderTarget, viewport: Rect) -> Self {
        let boundary = Self {
            _state: RenderStateGuard::enter(),
        };
        set_camera(&Camera2D {
            render_target: Some(target.clone()),
            // Negative zoom.y agrees with mue's top-left scissor coordinates.
            ..Camera2D::from_display_rect(viewport.into())
        });
        reset_draw_state();
        boundary
    }

    fn default_camera() -> Self {
        let boundary = Self {
            _state: RenderStateGuard::enter(),
        };
        set_default_camera();
        reset_draw_state();
        boundary
    }
}

impl Drop for DrawBoundary {
    fn drop(&mut self) {
        set_default_camera(); // Flushes immediate work and closes retained passes.
    }
}

fn reset_draw_state() {
    gl_use_default_material();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(None);
    gl.viewport(None);
    gl.draw_mode(DrawMode::Triangles);
}

fn target_pixels(viewport: Rect, dpi: f32) -> Result<(u32, u32)> {
    ensure!(
        viewport.x == 0. && viewport.y == 0.,
        "Scene pictures require an origin-zero logical window viewport"
    );
    ensure!(
        viewport.w.is_finite()
            && viewport.h.is_finite()
            && viewport.w > 0.
            && viewport.h > 0.
            && dpi.is_finite()
            && dpi > 0.,
        "Scene pictures require finite positive dimensions and DPI"
    );
    // Use f64 for the product/range check so i32::MAX is not rounded up to an
    // unrepresentable i32 by an f32 boundary conversion. Subpixel targets are 1.
    let width = (f64::from(viewport.w) * f64::from(dpi)).round().max(1.);
    let height = (f64::from(viewport.h) * f64::from(dpi)).round().max(1.);
    ensure!(
        width <= f64::from(i32::MAX) && height <= f64::from(i32::MAX),
        "Scene picture target dimensions overflow"
    );
    Ok((width as u32, height as u32))
}

fn blit(texture: &Texture2D, viewport: Rect) {
    draw_texture_ex(
        texture,
        viewport.x,
        viewport.y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(viewport.w, viewport.h)),
            flip_y: true,
            ..Default::default()
        },
    );
}

fn require_opengl() -> Result<()> {
    ensure!(
        unsafe { get_internal_gl() }.quad_context.info().backend == Backend::OpenGl,
        "Scene picture transitions currently require OpenGL; Metal is unsupported (use an overlay)"
    );
    Ok(())
}

fn make_copy_material() -> Result<Material> {
    require_opengl()?;
    // Overwrite RGBA and force alpha=1: default alpha blending would multiply
    // cached alpha again and darken later copies/frozen interrupted composites.
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: COPY_FRAGMENT,
        },
        MaterialParams::default(),
    )
    .context("compile Scene picture copy material")
}

fn make_opaque_material() -> Result<Material> {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: "#version 100\nprecision mediump float;\nvoid main() { gl_FragColor = vec4(1.0); }",
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                // Preserve RGB with blending, not a color-write mask: GL clears
                // inherit the last applied mask even after restoring QuadGl state.
                color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Zero, BlendFactor::One)),
                alpha_blend: Some(BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::Zero)),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .context("compile Scene picture opaque material")
}

fn make_crossfade_material() -> Result<Material> {
    require_opengl()?;
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: CROSSFADE_FRAGMENT,
        },
        MaterialParams {
            uniforms: vec![UniformDesc::new("Progress", UniformType::Float1)],
            textures: vec!["From".to_owned()],
            ..Default::default()
        },
    )
    .context("compile Scene crossfade material")
}

fn crossfade(material: &Material, frame: TransitionFrame, pictures: ScenePictures<'_>) {
    material.set_uniform("Progress", frame.progress);
    material.set_texture("From", pictures.from.clone());
    gl_use_material(material);
    blit(pictures.to, frame.viewport);
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
uniform mat4 Model;
uniform mat4 Projection;
varying mediump vec2 uv;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
}
"#;

const COPY_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
varying mediump vec2 uv;
void main() {
    gl_FragColor = vec4(texture2D(Texture, uv).rgb, 1.0);
}
"#;

const CROSSFADE_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
uniform sampler2D From;
uniform float Progress;
varying mediump vec2 uv;
void main() {
    vec3 from = texture2D(From, uv).rgb;
    vec3 to = texture2D(Texture, uv).rgb;
    gl_FragColor = vec4(mix(from, to, Progress), 1.0);
}
"#;
