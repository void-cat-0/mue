//! Immediate-mode UI materials.
//!
//! A UiMaterial pairs a fragment shader with the fixed vertex shader that
//! exposes mue's global position (v_pos), texture coordinate (v_uv), and
//! color (v_color). Use with_material or UiMaterial::with around the draws
//! that should use it.
//!
//! # Built-in uniforms
//!
//! Every UiMaterial also declares these uniforms, which [`UiMaterial::with`]
//! and [`UiMaterial::with_rect`] fill before drawing. Declare the ones you use
//! in the fragment shader; unused ones cost nothing.
//!
//! | Uniform | Type | Value |
//! | --- | --- | --- |
//! | `u_time` | `float` | Seconds since the host started (macroquad time) |
//! | `u_resolution` | `vec2` | Window size in logical pixels (the range of `v_pos`) |
//! | `u_dpi` | `float` | Physical pixels per logical pixel |
//! | `u_rect` | `vec4` | `x, y, w, h` of the drawn rectangle, set by `with_rect` or [`shader_box`](fn@crate::node::shader_box) |

use std::{cell::RefCell, ops::Range};

use macroquad::{
    Error,
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    math::{Mat4, Vec2, Vec3, Vec4},
    miniquad::{
        BlendFactor, BlendState, BlendValue, Equation, PipelineParams, ShaderSource, UniformDesc,
        UniformType,
    },
    prelude::GlPipeline,
    retained::{self, RetainedMaterial, RetainedMesh},
    texture::Texture2D,
    window::get_internal_gl,
};

/// Uniforms every UiMaterial declares; see the module documentation.
pub const GLOBAL_UNIFORMS: [(&str, UniformType); 4] = [
    ("u_time", UniformType::Float1),
    ("u_resolution", UniformType::Float2),
    ("u_dpi", UniformType::Float1),
    ("u_rect", UniformType::Float4),
];

/// Vertex shader shared by every UiMaterial.
///
/// v_pos is the position after Model, i.e. macroquad's global UI space.
/// Phira's pos0 = position.xy assumes vertices are already global; mue keeps
/// vertices local and applies Model, so a material must use this varying.
pub const UI_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying highp vec2 v_pos;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    vec4 world = Model * vec4(position, 1.0);
    gl_Position = Projection * world;
    v_pos = world.xy;
    v_uv = texcoord;
    v_color = color0 / 255.0;
}
"#;

/// A fragment-shader material for mue's immediate Shapes draws.
///
/// The fragment shader can declare v_pos, v_uv, and v_color; the vertex shader
/// is fixed. Uniforms and textures are supplied by the caller.
#[derive(Clone, Debug)]
pub struct UiMaterial {
    material: Material,
}

impl UiMaterial {
    /// Build a material from a fragment shader and its uniform/texture list.
    ///
    /// The pipeline uses standard alpha blending. Shader compilation errors are
    /// returned as macroquad::Error. The shader is GLSL; a Metal/MSL variant is
    /// not provided yet.
    pub fn new(
        fragment: &str,
        uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
    ) -> Result<Self, Error> {
        let mut uniforms = uniforms;
        for (name, ty) in GLOBAL_UNIFORMS {
            if !uniforms.iter().any(|uniform| uniform.name == name) {
                uniforms.push(UniformDesc::new(name, ty));
            }
        }
        let material = load_material(
            ShaderSource::Glsl {
                vertex: UI_VERTEX,
                fragment,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    )),
                    ..Default::default()
                },
                uniforms,
                textures,
            },
        )?;
        Ok(Self { material })
    }

    /// The underlying macroquad material, for APIs not covered here.
    pub fn material(&self) -> &Material {
        &self.material
    }

    /// Set a uniform value.
    pub fn set_uniform(&self, name: &str, value: impl UniformValue) {
        value.apply(&self.material, name);
    }

    /// Bind a named texture.
    pub fn set_texture(&self, name: &str, texture: Texture2D) {
        self.material.set_texture(name, texture);
    }

    /// Draw f with this material, restoring the previous pipeline afterwards.
    /// Text and shape nodes use immediate geometry while this material is active.
    /// Fills `u_time`, `u_resolution`, and `u_dpi` first.
    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        self.set_globals();
        with_material(&self.material, f)
    }

    /// Like [`Self::with`], also setting `u_rect` to `rect`.
    pub fn with_rect<R>(&self, rect: crate::math::Rect, f: impl FnOnce() -> R) -> R {
        self.set_uniform("u_rect", Vec4::new(rect.x, rect.y, rect.w, rect.h));
        self.with(f)
    }

    /// Fill the built-in time, resolution, and DPI uniforms.
    pub fn set_globals(&self) {
        self.set_uniform("u_time", macroquad::time::get_time() as f32);
        self.set_uniform(
            "u_resolution",
            Vec2::new(
                macroquad::window::screen_width(),
                macroquad::window::screen_height(),
            ),
        );
        self.set_uniform("u_dpi", macroquad::window::screen_dpi_scale());
    }

    /// Bind the matching retained shader for a built-in clip scope.
    pub(crate) fn with_retained<R>(
        &self,
        retained: Option<RetainedMaterial>,
        f: impl FnOnce() -> R,
    ) -> R {
        let _guard = MaterialGuard::enter(&self.material, retained);
        f()
    }
}

/// A value that can be uploaded to a material uniform.
pub trait UniformValue: Copy {
    #[doc(hidden)]
    fn apply(self, material: &Material, name: &str);
}

macro_rules! impl_uniform_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl UniformValue for $ty {
                fn apply(self, material: &Material, name: &str) {
                    material.set_uniform(name, self);
                }
            }
        )*
    };
}

impl_uniform_value!(f32, i32, Vec2, Vec3, Vec4, Mat4);

/// Draw f with material, restoring the previous pipeline afterwards.
/// Text and shape nodes fall back to immediate geometry; arbitrary macroquad
/// materials do not share the retained vertex layout or shader contract.
///
/// This forces a batch break on entry and exit. QuadGl::pipeline only breaks
/// batching when the pipeline changes; two scopes using the same material with
/// different uniforms would otherwise batch together and reuse the first
/// scope's uniform snapshot.
///
/// Nested scopes restore the pipeline, but not the outer material's uniforms:
/// re-set them after the inner scope if the outer draw continues.
pub fn with_material<R>(material: &Material, f: impl FnOnce() -> R) -> R {
    let _guard = MaterialGuard::enter(material, None);
    f()
}

struct RetainedBinding {
    pipeline: GlPipeline,
    material: RetainedMaterial,
}

thread_local! {
    static ACTIVE_RETAINED: RefCell<Option<RetainedBinding>> = const { RefCell::new(None) };
}

/// Only the default pipeline or the current scope's matching shader can use retained geometry.
pub(crate) fn retained_material_matches() -> bool {
    let pipeline = unsafe { get_internal_gl() }.quad_gl.get_pipeline();
    pipeline.is_none()
        || ACTIVE_RETAINED.with(|active| {
            active
                .borrow()
                .as_ref()
                .is_some_and(|binding| Some(binding.pipeline) == pipeline)
        })
}

/// Call only after retained_enabled has checked the backend and current material.
pub(crate) fn draw_retained_meshes_ranges(
    meshes: &[RetainedMesh],
    ranges: &[Option<Range<u32>>],
    model: Mat4,
    alpha: f32,
) {
    let pipeline = unsafe { get_internal_gl() }.quad_gl.get_pipeline();
    if pipeline.is_none() {
        retained::draw_meshes_ranges(meshes, ranges, model, alpha);
    } else {
        ACTIVE_RETAINED.with(|active| {
            let active = active.borrow();
            let binding = active
                .as_ref()
                .filter(|binding| Some(binding.pipeline) == pipeline)
                .expect("retained draw requires a matching material");
            retained::draw_meshes_ranges_with_material(
                meshes,
                ranges,
                model,
                alpha,
                &binding.material,
            );
        });
    }
}

pub(crate) fn draw_retained_meshes(meshes: &[RetainedMesh], model: Mat4, alpha: f32) {
    draw_retained_meshes_ranges(meshes, &[], model, alpha);
}

struct MaterialGuard {
    previous: Option<GlPipeline>,
    previous_retained: Option<RetainedBinding>,
}

impl MaterialGuard {
    fn enter(material: &Material, retained: Option<RetainedMaterial>) -> Self {
        let previous = unsafe { get_internal_gl() }.quad_gl.get_pipeline();
        // Toggle through the default pipeline so entering the same material
        // still starts a new draw call.
        gl_use_default_material();
        gl_use_material(material);
        let binding = retained.map(|material| RetainedBinding {
            pipeline: unsafe { get_internal_gl() }
                .quad_gl
                .get_pipeline()
                .expect("material pipeline"),
            material,
        });
        // None deliberately hides an outer partner, even when the pipeline is unchanged.
        let previous_retained = ACTIVE_RETAINED.with(|active| active.replace(binding));
        Self {
            previous,
            previous_retained,
        }
    }
}

impl Drop for MaterialGuard {
    fn drop(&mut self) {
        gl_use_default_material();
        if let Some(previous) = self.previous {
            unsafe { get_internal_gl() }
                .quad_gl
                .pipeline(Some(previous));
        }
        ACTIVE_RETAINED.with(|active| active.replace(self.previous_retained.take()));
    }
}
