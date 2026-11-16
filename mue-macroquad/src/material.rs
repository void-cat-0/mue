//! Immediate-mode UI materials.
//!
//! A UiMaterial pairs a fragment shader with the fixed vertex shader that
//! exposes mue's global position (v_pos), texture coordinate (v_uv), and
//! color (v_color). Use with_material or UiMaterial::with around the draws
//! that should use it.

use macroquad::{
    Error,
    material::{Material, MaterialParams, gl_use_default_material, gl_use_material, load_material},
    math::{Mat4, Vec2, Vec3, Vec4},
    miniquad::{
        BlendFactor, BlendState, BlendValue, Equation, PipelineParams, ShaderSource, UniformDesc,
    },
    prelude::GlPipeline,
    texture::Texture2D,
    window::get_internal_gl,
};

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
    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        with_material(&self.material, f)
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
///
/// This forces a batch break on entry and exit. QuadGl::pipeline only breaks
/// batching when the pipeline changes; two scopes using the same material with
/// different uniforms would otherwise batch together and reuse the first
/// scope's uniform snapshot.
///
/// Nested scopes restore the pipeline, but not the outer material's uniforms:
/// re-set them after the inner scope if the outer draw continues.
pub fn with_material<R>(material: &Material, f: impl FnOnce() -> R) -> R {
    let _guard = MaterialGuard::enter(material);
    f()
}

struct MaterialGuard {
    previous: Option<GlPipeline>,
}

impl MaterialGuard {
    fn enter(material: &Material) -> Self {
        let previous = unsafe { get_internal_gl() }.quad_gl.get_pipeline();
        // Toggle through the default pipeline so entering the same material
        // still starts a new draw call.
        gl_use_default_material();
        gl_use_material(material);
        Self { previous }
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
    }
}
