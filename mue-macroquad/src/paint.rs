use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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
    math::{Matrix, Rect, Vector, vec2},
    shader::{Brush, IntoShader, Shader, SharedTexture},
    style::Style,
};

#[derive(Default)]
struct Cache {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
    static DRAW_TRANSFORM: Cell<Matrix> = const { Cell::new(Matrix::IDENTITY) };
}

/// Apply a transform to a subtree without changing its layout coordinates.
pub(crate) fn with_transform<R>(transform: Matrix, draw: impl FnOnce() -> R) -> R {
    if transform == Matrix::IDENTITY {
        return draw();
    }
    let previous = DRAW_TRANSFORM.replace(DRAW_TRANSFORM.get() * transform);
    let c = transform.to_cols_array();
    let matrix = macroquad::math::Mat4::from_cols_array(&[
        c[0], c[1], 0., 0., c[3], c[4], 0., 0., 0., 0., 1., 0., c[6], c[7], 0., 1.,
    ]);
    unsafe { get_internal_gl() }
        .quad_gl
        .push_model_matrix(matrix);
    let result = draw();
    unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
    DRAW_TRANSFORM.set(previous);
    result
}

/// Restrict drawing to an axis-aligned rectangle, preserving an enclosing clip.
pub fn with_clip<R>(rect: Rect, draw: impl FnOnce() -> R) -> R {
    let transform = DRAW_TRANSFORM.get();
    let corners = [
        rect.min(),
        rect.max(),
        vec2(rect.right(), rect.y),
        vec2(rect.x, rect.bottom()),
    ]
    .map(|point| transform.transform_point2(point));
    let min = corners.into_iter().reduce(Vector::min).unwrap();
    let max = corners.into_iter().reduce(Vector::max).unwrap();
    let rect = Rect::new(min.x, min.y, max.x - min.x, max.y - min.y);
    let gl = unsafe { macroquad::window::get_internal_gl() }.quad_gl;
    let previous = gl.get_scissor();
    let scale = macroquad::window::screen_dpi_scale();
    let mut bounds = Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.w * scale,
        rect.h * scale,
    );
    if let Some((x, y, w, h)) = previous {
        bounds = bounds
            .intersect(&Rect::new(x as f32, y as f32, w as f32, h as f32))
            .unwrap_or_default();
    }
    gl.scissor(Some((
        bounds.x.floor() as i32,
        bounds.y.floor() as i32,
        (bounds.right().ceil() - bounds.x.floor()).max(0.) as i32,
        (bounds.bottom().ceil() - bounds.y.floor()).max(0.) as i32,
    )));
    let result = draw();
    unsafe { macroquad::window::get_internal_gl() }
        .quad_gl
        .scissor(previous);
    result
}

#[derive(Clone)]
#[repr(transparent)]
struct Transform(Prop<Matrix>);

#[derive(Clone)]
struct Opacity(Prop<f32>);

/// Draw geometry using the node's inherited transform and opacity.
pub struct Paint {
    pub transform: Prop<Matrix>,
    opacity: Prop<f32>,
    tolerance: Prop<f32>,
}

impl Paint {
    /// Build cached shapes that update whenever their tracked dependencies change.
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

/// Obtain paint state while building a node, inheriting parent transform and opacity.
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
    /// Build reusable geometry in local coordinates for a `shape` node.
    pub fn build(f: impl FnOnce(&mut ShapesBuilder)) -> Rc<Self> {
        let mut shapes = Self::default();
        CACHE.with_borrow_mut(|cache| {
            f(&mut ShapesBuilder::new(
                cache,
                Matrix::IDENTITY,
                1.,
                0.2,
                &mut shapes,
            ));
        });
        Rc::new(shapes)
    }

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

fn texture_vertex(
    shader: &impl Shader,
    transform: Matrix,
    alpha: f32,
    point: Vector,
    uv: Vector,
) -> Vertex {
    let mut vertex = shader.new_vertex(&transform, &point, alpha);
    vertex.uv = macroquad::math::vec2(uv.x, uv.y);
    vertex
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
    pub fn draw_texture<S: IntoShader>(
        &mut self,
        draw_rect: Rect,
        texture: SharedTexture,
        uv_rect: Rect,
        color: S,
    ) {
        self.fill_quad(
            draw_rect,
            0.,
            Some(texture),
            Some(uv_rect),
            color.into_shader(),
        );
    }

    fn fill_quad(
        &mut self,
        rect: Rect,
        radius: f32,
        texture: Option<SharedTexture>,
        uv: Option<Rect>,
        shader: impl Shader,
    ) {
        let (columns, rows) = shader.subdivisions(rect);
        let vertices = (columns + 1) * (rows + 1);
        // Keep batches within macroquad's default geometry buffer limits.
        if self.shapes.shapes.last().is_some_and(|shape| {
            shape.buffers.vertices.len() + vertices > 10000
                || shape.buffers.indices.len() + columns * rows * 6 > 10000
        }) {
            self.shapes.shapes.push(Shape {
                texture: texture.clone(),
                ..Default::default()
            });
        }
        let shape = self.shapes.shape_for_texture(texture);
        let offset = shape.buffers.vertices.len() as u16;
        for y in 0..=rows {
            let v = y as f32 / rows as f32;
            let py = rect.h * v;
            let dy = (radius - py.min(rect.h - py)).max(0.);
            let inset = if radius > 0. {
                radius - (radius * radius - dy * dy).max(0.).sqrt()
            } else {
                0.
            };
            for x in 0..=columns {
                let u = x as f32 / columns as f32;
                let point = vec2(rect.x + inset + (rect.w - 2. * inset) * u, rect.y + py);
                let vertex = if let Some(uv) = uv {
                    texture_vertex(
                        &shader,
                        self.inner.transform,
                        self.inner.alpha,
                        point,
                        vec2(uv.x + uv.w * u, uv.y + uv.h * v),
                    )
                } else {
                    shader.new_vertex(&self.inner.transform, &point, self.inner.alpha)
                };
                shape.buffers.vertices.push(vertex);
            }
        }
        for y in 0..rows {
            for x in 0..columns {
                let a = offset + (y * (columns + 1) + x) as u16;
                let b = a + 1;
                let c = b + (columns + 1) as u16;
                let d = c - 1;
                shape.buffers.indices.extend([a, b, c, a, c, d]);
            }
        }
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
        let shader = shader.into_shader();
        self.fill_quad(rect, 0., shader.texture(), None, shader);
    }

    pub fn fill_rounded_rect<S>(&mut self, rect: Rect, radius: f32, shader: S)
    where
        S: IntoShader,
    {
        let shader = shader.into_shader();
        let radius = radius.max(0.).min(rect.w.min(rect.h) / 2.);
        // Boundary-only tessellation misses the center of a radial gradient.
        // Sample the interior too, curving the grid's edges around the corners.
        if shader.subdivisions(rect) != (1, 1) {
            self.fill_quad(rect, radius, shader.texture(), None, shader);
            return;
        }
        let mut path = lyon::path::Path::builder();
        path.add_rounded_rectangle(
            &lyon::geom::Box2D::from_origin_and_size(
                lyon::geom::point(rect.x, rect.y),
                lyon::geom::size(rect.w, rect.h),
            ),
            &lyon::path::builder::BorderRadii::new(radius),
            lyon::path::Winding::Positive,
        );
        let path = path.build();
        self.inner.build(self.shapes, shader, |cache, builder| {
            cache
                .fill_tess
                .tessellate(path.iter(), &self.fill_options, builder)
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

/// Add the background before the node's content hooks.
pub fn use_background(style: &mut Style, rect: ReadSignal<Rect>) {
    let background = style.take_background();
    let radius = crate::style::units::resolve_length(style.take_radius(), "radius");
    let paint = use_paint(style);
    style.on_render.append(move |_| {
        let brush = background.get_clone();
        if matches!(&brush, Brush::Solid(color) if color.a <= 0.) {
            return;
        }
        let rect = rect.get();
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let radius = radius.get().max(0.).min(rect.w.min(rect.h) / 2.);
        paint.draw(|p| {
            if radius > 0. {
                p.fill_rounded_rect(rect, radius, brush.shader(rect));
            } else {
                p.fill_rect(rect, brush.shader(rect));
            }
        });
    });
}
