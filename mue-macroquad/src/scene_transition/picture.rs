//! Exclusive picture passes. This cache owns no Scene and no transition clock.

use anyhow::{Context, Result, ensure};
use macroquad::{
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    miniquad::{Backend, ShaderSource, UniformDesc, UniformType},
    prelude::{
        BLACK, Camera2D, DrawMode, DrawTextureParams, FilterMode, RenderTarget, Texture2D, WHITE,
        clear_background, draw_texture_ex, render_target, set_camera, set_default_camera, vec2,
    },
    window::{get_internal_gl, screen_dpi_scale},
};

use crate::{math::Rect, render::RenderStateGuard};

use super::{ScenePictures, SceneTransition, TransitionFrame};

/// Source, live and last successfully presented composite are separate targets.
/// All begin/finish boundaries require exclusive use of macroquad drawing.
/// Capture callbacks keep the supplied target camera and a balanced model stack.
pub(crate) struct PictureCache {
    headless: bool,
    key: Option<PictureKey>,
    targets: Option<Targets>,
    copy_material: Option<Material>,
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
            headless: false,
            key: None,
            targets: None,
            copy_material: None,
            boundary: None,
            drawing: None,
            source_ready: false,
            live_ready: false,
            presented_ready: false,
        }
    }

    /// Scheduler tests may execute GPU-free overlay closures without macroquad.
    /// Capture calls return no target; composition itself is an explicit error.
    #[cfg(test)]
    pub(crate) fn headless() -> Self {
        let mut cache = Self::new();
        cache.headless = true;
        cache
    }

    pub(crate) fn needs_source(&self, viewport: Rect) -> Result<bool> {
        Ok(self.key != Some(self.picture_key(viewport)?) || !self.source_ready)
    }

    /// App renders the departing Scene once in this pass, without a Scene/Node
    /// update, before releasing its owner. The target clone is optional so the
    /// same scheduling path can be exercised headlessly.
    pub(crate) fn begin_source(&mut self, viewport: Rect) -> Result<Option<RenderTarget>> {
        self.ensure_idle()?;
        self.prepare_targets(viewport)?;
        self.source_ready = false;
        self.live_ready = false;
        self.presented_ready = false;
        self.drawing = Some(Capture::Source);
        if self.headless {
            return Ok(None);
        }
        let target = self.targets.as_ref().unwrap().from.clone();
        self.boundary = Some(DrawBoundary::target(&target, viewport));
        // A complete Scene picture always has an opaque base. The camera change
        // flushed pending work before this immediate clear.
        clear_background(BLACK);
        Ok(Some(target))
    }

    pub(crate) fn finish_source(&mut self) {
        if self.drawing == Some(Capture::Source) {
            self.finish();
            self.source_ready = true;
        }
    }

    /// App draws the current Scene/UI once into the live target each frame.
    pub(crate) fn begin_live(&mut self, viewport: Rect) -> Result<Option<RenderTarget>> {
        self.ensure_idle()?;
        self.prepare_targets(viewport)?;
        self.live_ready = false;
        self.drawing = Some(Capture::Live);
        if self.headless {
            return Ok(None);
        }
        let target = self.targets.as_ref().unwrap().to.clone();
        self.boundary = Some(DrawBoundary::target(&target, viewport));
        clear_background(BLACK);
        Ok(Some(target))
    }

    pub(crate) fn finish_live(&mut self) {
        if self.drawing == Some(Capture::Live) {
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
            !self.headless,
            "Scene picture composition is unavailable in headless mode"
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
        transition.draw_composite(frame, pictures)?;
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
    /// Headless mode invokes only the closure and never queries window/GL state.
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
        if self.headless {
            return transition.draw_overlay(frame);
        }
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
        if self.key != Some(self.picture_key(viewport)?) {
            self.invalidate();
            return Ok(false);
        }
        if !self.presented_ready {
            return Ok(false);
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
    /// applies here too. Headless scheduler tests never touch macroquad.
    pub(crate) fn cleanup_scene_error(&mut self) {
        if !self.headless {
            set_default_camera();
            unsafe { get_internal_gl() }.flush();
        }
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
        // This branch is deliberately before any macroquad window call.
        let dpi = if self.headless {
            1.
        } else {
            screen_dpi_scale()
        };
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
        if !self.headless {
            // Verify backend/compile before allocating any picture. Metal is an
            // explicit error, not a GLSL shader panic on the vendor Metal path.
            if self.copy_material.is_none() {
                self.copy_material = Some(make_copy_material()?);
            }
            if self.targets.is_none() {
                self.targets = Some(Targets::new(key.pixels));
            }
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
    state: Option<RenderStateGuard>,
}

impl DrawBoundary {
    fn target(target: &RenderTarget, viewport: Rect) -> Self {
        let boundary = Self {
            state: Some(RenderStateGuard::enter()),
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
            state: Some(RenderStateGuard::enter()),
        };
        set_default_camera();
        reset_draw_state();
        boundary
    }
}

impl Drop for DrawBoundary {
    fn drop(&mut self) {
        set_default_camera(); // Flushes immediate work and closes retained passes.
        drop(self.state.take());
    }
}

fn reset_draw_state() {
    gl_use_default_material();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(None);
    gl.viewport(None);
    gl.draw_mode(DrawMode::Triangles);
}

pub(super) fn target_pixels(viewport: Rect, dpi: f32) -> Result<(u32, u32)> {
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

pub(super) fn make_crossfade_material() -> Result<Material> {
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

pub(super) fn crossfade(material: &Material, frame: TransitionFrame, pictures: ScenePictures<'_>) {
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
