//! Exclusive picture passes. This cache owns no Scene and no transition clock.

use macroquad::{
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    miniquad::{
        Backend, BlendFactor, BlendState, Equation, PipelineParams, ShaderSource, UniformDesc,
        UniformType,
    },
    prelude::{
        BLACK, Camera2D, DrawMode, DrawTextureParams, FilterMode, RenderTarget, Texture2D, WHITE,
        draw_rectangle, draw_texture_ex, render_target, set_camera, set_default_camera, vec2,
    },
    window::{get_internal_gl, screen_dpi_scale},
};

use crate::{CallbackError, Result, TransitionError, math::Rect, render::RenderStateGuard};

use super::{Draw, ScenePictures, SceneTransition, TransitionFrame};

type TransitionResult<T> = std::result::Result<T, TransitionError>;

/// Owns only pictures and their reusable materials, never an open pass.
pub(crate) struct PictureCache {
    targets: Option<Targets>,
    materials: Option<PictureMaterials>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PictureKey {
    viewport: Rect,
    pixels: (u32, u32),
}

struct Targets {
    key: PictureKey,
    source: Option<RenderTarget>,
    live: RenderTarget,
    output: Option<OutputTarget>,
}

enum OutputTarget {
    Scratch(RenderTarget),
    Presented(RenderTarget),
}

impl OutputTarget {
    fn into_target(self) -> RenderTarget {
        match self {
            Self::Scratch(target) | Self::Presented(target) => target,
        }
    }
}

struct PictureMaterials {
    copy: Material,
    opaque: Material,
    crossfade: Option<Material>,
}

impl PictureCache {
    /// No textures, shader compilation, window query, or GL state access.
    pub(crate) fn new() -> Self {
        Self {
            targets: None,
            materials: None,
        }
    }

    pub(crate) fn needs_source(&self, viewport: Rect) -> TransitionResult<bool> {
        let key = PictureKey::new(viewport)?;
        Ok(self
            .targets
            .as_ref()
            .is_none_or(|targets| targets.key != key || targets.source.is_none()))
    }

    /// Capture the departing Scene before App releases its owner. The target
    /// is only published after a successful draw and its final flush.
    pub(crate) fn capture_source<T>(
        &mut self,
        viewport: Rect,
        draw: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let (targets, materials) = self.prepare(viewport)?;
        let target = targets
            .source
            .take()
            .unwrap_or_else(|| picture_target(targets.key.pixels));
        targets.output = targets
            .output
            .take()
            .map(|output| OutputTarget::Scratch(output.into_target()));
        let result = draw_picture(&target, viewport, &materials.opaque, draw)?;
        targets.source = Some(target);
        Ok(result)
    }

    /// Capture live, compose into a separate target, then submit the screen copy.
    /// App can acknowledge the frame only on success. A failed composite does
    /// not publish scratch pixels or overwrite the screen with them.
    pub(crate) fn draw_composite<T>(
        &mut self,
        frame: TransitionFrame,
        transition: &mut SceneTransition,
        draw_live: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let viewport = frame.viewport;
        let (targets, materials) = self.prepare(viewport)?;
        let drawn = draw_picture(&targets.live, viewport, &materials.opaque, draw_live)?;
        if targets.key != PictureKey::new(viewport)? {
            return Err(TransitionError::ChangedViewport.into());
        }

        // Resize cannot reconstruct a released Scene. Use live for both inputs
        // in that case, never the output. App's transition clock is unaffected.
        let output = targets.output.take().map_or_else(
            || picture_target(targets.key.pixels),
            OutputTarget::into_target,
        );
        let pictures = ScenePictures {
            from: &targets.source.as_ref().unwrap_or(&targets.live).texture,
            to: &targets.live.texture,
        };
        draw_picture(&output, viewport, &materials.opaque, || {
            match &mut transition.draw {
                Draw::Crossfade => {
                    // A new request needs a fresh clock, not a fresh pipeline.
                    if materials.crossfade.is_none() {
                        materials.crossfade = Some(make_crossfade_material()?);
                    }
                    crossfade(materials.crossfade.as_ref().unwrap(), frame, pictures);
                }
                Draw::Composite(draw) => draw(frame, pictures)
                    .map_err(|error| CallbackError::new("transition composite", error))?,
                Draw::Overlay(_) => unreachable!("overlay passed to draw_composite"),
            }
            Ok(())
        })?;

        {
            let _boundary = DrawBoundary::default_camera();
            gl_use_material(&materials.copy);
            blit(&output.texture, viewport);
        }
        targets.output = Some(OutputTarget::Presented(output));
        Ok(drawn)
    }

    /// Normal overlays use the default camera but allocate no picture resources.
    pub(crate) fn draw_overlay(
        frame: TransitionFrame,
        transition: &mut SceneTransition,
    ) -> std::result::Result<(), CallbackError> {
        let Draw::Overlay(draw) = &mut transition.draw else {
            unreachable!("composite passed to draw_overlay");
        };
        let _boundary = DrawBoundary::default_camera();
        let result = draw(frame).map_err(|error| CallbackError::new("transition overlay", error));
        queue_color_write_reset();
        result
    }

    /// Reuse only the last successfully displayed composite on interruption.
    /// Swapping ownership needs no copy, readback, Scene reference or clock reset.
    /// An overlay is not captured, so no arbitrary overlay continuity is promised.
    pub(crate) fn freeze_presented(&mut self, viewport: Rect) -> bool {
        // Reusing old pixels is optional; a temporarily unavailable surface only
        // invalidates the cache. Actual capture still validates drawable sizes.
        let Ok(key) = PictureKey::new(viewport) else {
            self.invalidate();
            return false;
        };
        let Some(targets) = self.targets.as_mut().filter(|targets| targets.key == key) else {
            self.invalidate();
            return false;
        };
        if let Some(OutputTarget::Presented(presented)) = targets
            .output
            .take_if(|output| matches!(output, OutputTarget::Presented(_)))
        {
            targets.output = targets.source.replace(presented).map(OutputTarget::Scratch);
        }
        // Repeated interruptions before a new drawing keep the same source.
        targets.source.is_some()
    }

    /// Flush pending ordinary Scene draws on error before their owner can drop.
    /// This resets to the default camera and material. Captures close their own
    /// scopes instead. The same no-raw-pass/balanced-model-stack contract applies.
    pub(crate) fn cleanup_scene_error() {
        set_default_camera();
        queue_color_write_reset();
        unsafe { get_internal_gl() }.flush();
    }

    /// Discard pictures only. The phase clock belongs to App and is untouched.
    pub(crate) fn invalidate(&mut self) {
        self.targets = None;
    }

    fn prepare(
        &mut self,
        viewport: Rect,
    ) -> TransitionResult<(&mut Targets, &mut PictureMaterials)> {
        let key = PictureKey::new(viewport)?;
        if self
            .targets
            .as_ref()
            .is_some_and(|targets| targets.key != key)
        {
            self.invalidate();
        }
        // Verify backend and compile materials before allocating any targets.
        if self.materials.is_none() {
            require_picture_backend()?;
            self.materials = Some(PictureMaterials {
                copy: make_copy_material()?,
                opaque: make_opaque_material()?,
                crossfade: None,
            });
        }
        let targets = self.targets.get_or_insert_with(|| Targets {
            key,
            source: None,
            live: picture_target(key.pixels),
            output: None,
        });
        Ok((targets, self.materials.as_mut().unwrap()))
    }
}

impl PictureKey {
    fn new(viewport: Rect) -> TransitionResult<Self> {
        Ok(Self {
            viewport,
            pixels: target_pixels(viewport, screen_dpi_scale())?,
        })
    }
}

fn picture_target((width, height): (u32, u32)) -> RenderTarget {
    let target = render_target(width, height);
    target.texture.set_filter(FilterMode::Linear);
    target
}

fn draw_picture<T>(
    target: &RenderTarget,
    viewport: Rect,
    opaque: &Material,
    draw: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _boundary = DrawBoundary::target(target, viewport);
    // GL clears inherit the last applied color mask. An opaque default-material
    // draw applies the all-channel mask through miniquad instead. Submit it now
    // so the Scene can immediately clear its target with the expected mask.
    draw_rectangle(viewport.x, viewport.y, viewport.w, viewport.h, BLACK);
    unsafe { get_internal_gl() }.flush();
    let result = draw();
    if result.is_ok() {
        // Preserve RGB, but force alpha back to one if custom blending reduced it.
        unsafe { get_internal_gl() }.flush();
        reset_draw_state();
        gl_use_material(opaque);
        draw_rectangle(viewport.x, viewport.y, viewport.w, viewport.h, WHITE);
    } else {
        queue_color_write_reset();
    }
    result // The boundary flushes before releasing the target borrow.
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

// Apply the all-channel default pipeline without covering pixels. A raw
// glColorMask call would desynchronize miniquad's cached mask.
fn queue_color_write_reset() {
    reset_draw_state();
    draw_rectangle(0., 0., 0., 0., BLACK);
}

fn reset_draw_state() {
    gl_use_default_material();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(None);
    gl.viewport(None);
    gl.draw_mode(DrawMode::Triangles);
}

fn target_pixels(viewport: Rect, dpi: f32) -> TransitionResult<(u32, u32)> {
    if viewport.x != 0. || viewport.y != 0. {
        return Err(TransitionError::NonzeroViewportOrigin);
    }
    if !(viewport.w.is_finite()
        && viewport.h.is_finite()
        && viewport.w > 0.
        && viewport.h > 0.
        && dpi.is_finite()
        && dpi > 0.)
    {
        return Err(TransitionError::InvalidPictureDimensions);
    }
    // Use f64 for the product/range check so i32::MAX is not rounded up to an
    // unrepresentable i32 by an f32 boundary conversion. Subpixel targets are 1.
    let width = (f64::from(viewport.w) * f64::from(dpi)).round().max(1.);
    let height = (f64::from(viewport.h) * f64::from(dpi)).round().max(1.);
    if !(width <= f64::from(i32::MAX) && height <= f64::from(i32::MAX)) {
        return Err(TransitionError::PictureDimensionsOverflow);
    }
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

fn require_picture_backend() -> TransitionResult<()> {
    match unsafe { get_internal_gl() }.quad_context.info().backend {
        Backend::OpenGl => Ok(()),
        #[cfg(all(
            feature = "vulkan",
            any(
                windows,
                target_os = "android",
                all(target_os = "linux", not(target_env = "ohos"))
            )
        ))]
        Backend::Vulkan => Ok(()),
        _ => Err(TransitionError::UnsupportedBackend),
    }
}

fn picture_shader(fragment: PictureFragment) -> ShaderSource<'static> {
    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    if unsafe { get_internal_gl() }.quad_context.info().backend == Backend::Vulkan {
        let (vertex, fragment) = match fragment {
            PictureFragment::Copy => (
                include_bytes!("shaders/vulkan/spirv/picture-copy.vert.spv").as_slice(),
                include_bytes!("shaders/vulkan/spirv/picture-copy.frag.spv").as_slice(),
            ),
            PictureFragment::Opaque => (
                include_bytes!("shaders/vulkan/spirv/picture-opaque.vert.spv").as_slice(),
                include_bytes!("shaders/vulkan/spirv/picture-opaque.frag.spv").as_slice(),
            ),
            PictureFragment::Crossfade => (
                include_bytes!("shaders/vulkan/spirv/picture-crossfade.vert.spv").as_slice(),
                include_bytes!("shaders/vulkan/spirv/picture-crossfade.frag.spv").as_slice(),
            ),
        };
        // Each pair was compiled together with the corresponding QuadGl material layout.
        return unsafe { ShaderSource::spirv(vertex, fragment, &[], false) };
    }
    ShaderSource::Glsl {
        vertex: VERTEX,
        fragment: match fragment {
            PictureFragment::Copy => COPY_FRAGMENT,
            PictureFragment::Opaque => OPAQUE_FRAGMENT,
            PictureFragment::Crossfade => CROSSFADE_FRAGMENT,
        },
    }
}

#[derive(Clone, Copy)]
enum PictureFragment {
    Copy,
    Opaque,
    Crossfade,
}

fn make_copy_material() -> TransitionResult<Material> {
    // Overwrite RGBA and force alpha=1: default alpha blending would multiply
    // cached alpha again and darken later copies/frozen interrupted composites.
    load_material(
        picture_shader(PictureFragment::Copy),
        MaterialParams::default(),
    )
    .map_err(|source| TransitionError::Material {
        operation: "load Scene picture copy material",
        source,
    })
}

fn make_opaque_material() -> TransitionResult<Material> {
    load_material(
        picture_shader(PictureFragment::Opaque),
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
    .map_err(|source| TransitionError::Material { operation: "load Scene picture opaque material", source })
}

fn make_crossfade_material() -> TransitionResult<Material> {
    load_material(
        picture_shader(PictureFragment::Crossfade),
        MaterialParams {
            uniforms: vec![UniformDesc::new("Progress", UniformType::Float1)],
            textures: vec!["From".to_owned()],
            ..Default::default()
        },
    )
    .map_err(|source| TransitionError::Material {
        operation: "load Scene crossfade material",
        source,
    })
}

fn crossfade(material: &Material, frame: TransitionFrame, pictures: ScenePictures<'_>) {
    material.set_uniform("Progress", frame.progress);
    material.set_texture("From", pictures.from.clone());
    gl_use_material(material);
    blit(pictures.to, frame.viewport);
}

const VERTEX: &str = include_str!("shaders/vulkan/picture.vert.glsl");
const OPAQUE_FRAGMENT: &str = include_str!("shaders/vulkan/picture-opaque.frag.glsl");
const COPY_FRAGMENT: &str = include_str!("shaders/vulkan/picture-copy.frag.glsl");
const CROSSFADE_FRAGMENT: &str = include_str!("shaders/vulkan/picture-crossfade.frag.glsl");
