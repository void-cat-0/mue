use std::{cell::RefCell, rc::Rc};

use lyon::{
    path::PathEvent,
    tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor,
        StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor,
        TessellationResult, VertexBuffers,
    },
};
use macroquad::prelude::*;
use mue_core::{
    effect::computed_always,
    prop::Prop,
    signal::{Access, ReadSignal},
};
use smallvec::SmallVec;

use crate::{
    math::{vec2, Matrix, Rect, Vector},
    shader::{IntoShader, Shader, SharedTexture},
    style::Style,
};

#[derive(Default)]
struct Cache {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
}

#[derive(Clone)]
#[repr(transparent)]
struct Transform(Prop<Matrix>);

#[derive(Clone)]
struct Opacity(Prop<f32>);

pub struct Paint {
    pub transform: Prop<Matrix>,
    opacity: Prop<f32>,
    tolerance: Prop<f32>,
}

impl Paint {
    /// Build shapes reactively. Returns a ReadSignal<Shapes> that updates when dependencies change.
    pub fn build(&self, mut f: impl FnMut(&mut ShapesBuilder) + 'static) -> ReadSignal<Rc<Shapes>> {
        let transform = self.transform;
        let alpha = self.opacity;
        let tolerance = self.tolerance;
        computed_always(move |_| {
            let mut shapes = Shapes::default();
            CACHE.with_borrow_mut(|cache| {
                f(&mut ShapesBuilder::new(
                    cache,
                    transform.get(),
                    alpha.get(),
                    tolerance.get(),
                    &mut shapes,
                ));
            });
            Rc::new(shapes)
        })
    }

    /// Draw shapes immediately in immediate mode.
    pub fn draw(&self, f: impl FnOnce(&mut ShapesBuilder)) {
        let mut shapes = Shapes::default();
        CACHE.with_borrow_mut(|cache| {
            f(&mut ShapesBuilder::new(
                cache,
                self.transform.get(),
                self.opacity.get(),
                self.tolerance.get(),
                &mut shapes,
            ));
        });
        shapes.draw();
    }
}

fn frobenius_norm(m: &Matrix) -> f32 {
    (m.col(0).length_squared() + m.col(1).length_squared() + m.col(2).length_squared()).sqrt()
}

pub fn use_paint(style: &mut Style) -> Paint {
    fn extract<T: Clone + PartialEq + 'static>(
        prev: Option<Prop<T>>,
        style: Option<Prop<T>>,
        default: T,
        mut reduce: impl FnMut(T, T) -> T + 'static,
    ) -> Prop<T> {
        match (prev, style) {
            (Some(prev), Some(style)) => Prop::Dynamic(mue_core::effect::computed(move |_| {
                reduce(prev.get_clone(), style.get_clone())
            })),
            (Some(prev), None) => prev,
            (None, Some(style)) => style,
            (None, None) => Prop::Static(default),
        }
    }

    let transform = extract(
        mue_core::scope::inject::<Transform>().map(|it| it.0),
        style.transform,
        Matrix::IDENTITY,
        |a, b| a * b,
    );
    let opacity = extract(
        mue_core::scope::inject::<Opacity>().map(|it| it.0),
        style.opacity,
        1.0,
        |a, b| a * b,
    );
    let tolerance = transform.map(|t| {
        let scale = frobenius_norm(&t);
        0.2 / scale
    });

    Paint {
        transform,
        opacity,
        tolerance,
    }
}

#[derive(Default)]
pub struct Shapes {
    shapes: SmallVec<[Shape; 1]>,
}

impl Shapes {
    pub fn draw(&self) {
        for shape in &self.shapes {
            shape.draw();
        }
    }

    fn shape_for_texture(&mut self, texture: Option<SharedTexture>) -> &mut Shape {
        if self.shapes.last().is_some_and(|it| it.texture == texture) {
            self.shapes.last_mut().unwrap()
        } else {
            self.shapes.push(Shape {
                texture,
                ..Default::default()
            });
            self.shapes.last_mut().unwrap()
        }
    }
}

struct Shape {
    buffers: VertexBuffers<Vertex, u16>,
    texture: Option<SharedTexture>,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            buffers: VertexBuffers::new(),
            texture: None,
        }
    }
}

impl Shape {
    fn draw(&self) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(self.texture.as_deref());
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(&self.buffers.vertices, &self.buffers.indices);
    }
}

struct ShaderConstructor<T: Shader> {
    transform: Matrix,
    shader: T,
    alpha: f32,
}

impl<T: Shader> FillVertexConstructor<Vertex> for ShaderConstructor<T> {
    fn new_vertex(&mut self, vertex: FillVertex) -> Vertex {
        let pos = vertex.position();
        self.shader
            .new_vertex(&self.transform, &vec2(pos.x, pos.y), self.alpha)
    }
}

impl<T: Shader> StrokeVertexConstructor<Vertex> for ShaderConstructor<T> {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> Vertex {
        let pos = vertex.position();
        self.shader
            .new_vertex(&self.transform, &vec2(pos.x, pos.y), self.alpha)
    }
}

struct Inner<'a> {
    cache: &'a mut Cache,

    transform: Matrix,
    alpha: f32,
}

impl<'a> Inner<'a> {
    fn build<S: Shader>(
        &mut self,
        shapes: &mut Shapes,
        shader: S,
        f: impl FnOnce(
            &mut Cache,
            &mut BuffersBuilder<Vertex, u16, ShaderConstructor<S>>,
        ) -> TessellationResult,
    ) {
        let texture = shader.texture();
        let shape = shapes.shape_for_texture(texture);

        let constructor = ShaderConstructor {
            transform: self.transform,
            shader,
            alpha: self.alpha,
        };
        f(
            self.cache,
            &mut BuffersBuilder::new(&mut shape.buffers, constructor),
        )
        .unwrap();
    }
}

pub struct ShapesBuilder<'a> {
    inner: Inner<'a>,
    pub fill_options: FillOptions,
    pub stroke_options: StrokeOptions,
    shapes: &'a mut Shapes,
}

impl<'a> ShapesBuilder<'a> {
    fn new(
        cache: &'a mut Cache,
        transform: Matrix,
        alpha: f32,
        tolerance: f32,
        shapes: &'a mut Shapes,
    ) -> Self {
        Self {
            inner: Inner {
                cache,
                transform,
                alpha,
            },
            fill_options: FillOptions::tolerance(tolerance),
            stroke_options: StrokeOptions::tolerance(tolerance),
            shapes,
        }
    }
}

impl ShapesBuilder<'_> {
    pub fn draw_texture(
        &mut self,
        draw_rect: Rect,
        texture: SharedTexture,
        uv_rect: Rect,
        color: Color,
    ) {
        let shape = self.shapes.shape_for_texture(Some(texture));
        let dmin = draw_rect.min();
        let umin = uv_rect.min();
        let dmax = draw_rect.max();
        let umax = uv_rect.max();
        let offset = shape.buffers.vertices.len() as u16;
        shape.buffers.vertices.extend([
            Vertex::new(dmin.x, dmin.y, 0., umin.x, umin.y, color),
            Vertex::new(dmax.x, dmin.y, 0., umax.x, umin.y, color),
            Vertex::new(dmax.x, dmax.y, 0., umax.x, umax.y, color),
            Vertex::new(dmin.x, dmax.y, 0., umin.x, umax.y, color),
        ]);
        shape
            .buffers
            .indices
            .extend([0, 1, 2, 0, 2, 3].map(|i| i + offset));
    }

    /// Fill a path with the given shader.
    pub fn fill_path<I, S>(&mut self, path: I, shader: S)
    where
        I: IntoIterator<Item = PathEvent>,
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache
                    .fill_tess
                    .tessellate(path, &self.fill_options, builder)
            });
    }

    /// Fill a rectangle with the given shader.
    pub fn fill_rect<S>(&mut self, rect: Rect, shader: S)
    where
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache.fill_tess.tessellate_rectangle(
                    &lyon::geom::Box2D::from_origin_and_size(
                        lyon::geom::point(rect.x, rect.y),
                        lyon::geom::size(rect.w, rect.h),
                    ),
                    &self.fill_options,
                    builder,
                )
            });
    }

    /// Fill a circle with the given shader.
    pub fn fill_circle<S>(&mut self, center: Vector, radius: f32, shader: S)
    where
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache.fill_tess.tessellate_circle(
                    lyon::geom::point(center.x, center.y),
                    radius,
                    &self.fill_options,
                    builder,
                )
            });
    }
}

impl ShapesBuilder<'_> {
    /// Stroke a path with the given shader.
    pub fn stroke_path<I, S>(&mut self, path: I, shader: S)
    where
        I: IntoIterator<Item = PathEvent>,
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache
                    .stroke_tess
                    .tessellate(path, &self.stroke_options, builder)
            });
    }

    /// Stroke a rectangle with the given shader.
    pub fn stroke_rect<S>(&mut self, rect: Rect, shader: S)
    where
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache.stroke_tess.tessellate_rectangle(
                    &lyon::geom::Box2D::from_origin_and_size(
                        lyon::geom::point(rect.x, rect.y),
                        lyon::geom::size(rect.w, rect.h),
                    ),
                    &self.stroke_options,
                    builder,
                )
            });
    }

    /// Stroke a circle with the given shader.
    pub fn stroke_circle<S>(&mut self, center: Vector, radius: f32, shader: S)
    where
        S: IntoShader,
    {
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache.stroke_tess.tessellate_circle(
                    lyon::geom::point(center.x, center.y),
                    radius,
                    &self.stroke_options,
                    builder,
                )
            });
    }
}
