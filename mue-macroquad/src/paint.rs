use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
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

// These are intentionally below macroquad's default capacities (10,000 vertices and
// 5,000 indices). Keeping indices triangle-aligned also means every submitted
// geometry buffer contains complete triangles.
const MAX_SHAPE_VERTICES: usize = 9_996;
const MAX_SHAPE_INDICES: usize = 4_998;
const MAX_TILE_CELLS: usize = MAX_SHAPE_INDICES / 6;

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

fn append_mesh(
    shapes: &mut Shapes,
    texture: Option<SharedTexture>,
    buffers: VertexBuffers<Vertex, u32>,
) {
    debug_assert_eq!(buffers.indices.len() % 3, 0);
    if buffers.indices.is_empty() {
        return;
    }
    // Ordinary quads and bounded grid tiles can be appended without remapping.
    // Keep these meshes whole, including their UVs and shared vertices.
    if buffers.vertices.len() <= MAX_SHAPE_VERTICES && buffers.indices.len() <= MAX_SHAPE_INDICES {
        let shape = shapes.shape_for_texture(texture.clone());
        if shape.buffers.vertices.len() + buffers.vertices.len() > MAX_SHAPE_VERTICES
            || shape.buffers.indices.len() + buffers.indices.len() > MAX_SHAPE_INDICES
        {
            shapes.shapes.push(Shape {
                texture: texture.clone(),
                ..Default::default()
            });
        }
        let shape = shapes.shape_for_texture(texture);
        let offset = shape.buffers.vertices.len() as u32;
        shape
            .buffers
            .indices
            .extend(buffers.indices.iter().map(|&index| {
                debug_assert!((index as usize) < buffers.vertices.len());
                // The bounded vertex count guarantees this sum fits in u16.
                (offset + index) as u16
            }));
        shape.buffers.vertices.extend(buffers.vertices);
        return;
    }

    // A single tessellation can exceed either the drawcall limit or u16. Split
    // only on triangle boundaries, remapping shared vertices for each batch.
    let mut remap = HashMap::new();
    for triangle in buffers.indices.as_chunks::<3>().0 {
        let mut needed = SmallVec::<[u32; 3]>::new();
        for &index in triangle {
            assert!((index as usize) < buffers.vertices.len());
            if !needed.contains(&index) {
                needed.push(index);
            }
        }

        let shape = shapes.shape_for_texture(texture.clone());
        if shape.buffers.indices.len() + 3 > MAX_SHAPE_INDICES
            || shape.buffers.vertices.len() + needed.len() > MAX_SHAPE_VERTICES
        {
            shapes.shapes.push(Shape {
                texture: texture.clone(),
                ..Default::default()
            });
            remap.clear();
        }

        let shape = shapes.shape_for_texture(texture.clone());
        let mut destination = [0u16; 3];
        for (slot, &source) in triangle.iter().enumerate() {
            let target = *remap.entry(source).or_insert_with(|| {
                let target = shape.buffers.vertices.len() as u16;
                shape
                    .buffers
                    .vertices
                    .push(buffers.vertices[source as usize]);
                target
            });
            destination[slot] = target;
        }
        shape.buffers.indices.extend(destination);
    }
}

impl<'a> Inner<'a> {
    fn build<S: Shader>(
        &mut self,
        shapes: &mut Shapes,
        shader: S,
        f: impl FnOnce(
            &mut Cache,
            &mut BuffersBuilder<Vertex, u32, ShaderConstructor<S>>,
        ) -> TessellationResult,
    ) {
        let texture = shader.texture();
        let constructor = ShaderConstructor {
            transform: self.transform,
            shader,
            alpha: self.alpha,
        };
        let mut buffers = VertexBuffers::<Vertex, u32>::new();
        f(
            self.cache,
            &mut BuffersBuilder::new(&mut buffers, constructor),
        )
        .unwrap();
        append_mesh(shapes, texture, buffers);
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
        let columns = columns.max(1);
        let rows = rows.max(1);
        // Each tile fits even when a custom shader requests more than u16 vertices.
        // Sampling uses the original grid coordinates, including rounded edges/UVs.
        let tile_width = columns.min(MAX_TILE_CELLS);
        let tile_height = rows.min(MAX_TILE_CELLS / tile_width);
        for y0 in (0..rows).step_by(tile_height) {
            let y1 = y0 + tile_height.min(rows - y0);
            for x0 in (0..columns).step_by(tile_width) {
                let x1 = x0 + tile_width.min(columns - x0);
                let tile_columns = x1 - x0;
                let tile_rows = y1 - y0;
                let vertex_count = (tile_columns + 1) * (tile_rows + 1);
                let index_count = tile_columns * tile_rows * 6;
                let shape = self.shapes.shape_for_texture(texture.clone());
                if shape.buffers.vertices.len() + vertex_count > MAX_SHAPE_VERTICES
                    || shape.buffers.indices.len() + index_count > MAX_SHAPE_INDICES
                {
                    self.shapes.shapes.push(Shape {
                        texture: texture.clone(),
                        ..Default::default()
                    });
                }
                let shape = self.shapes.shape_for_texture(texture.clone());
                let offset = shape.buffers.vertices.len() as u16;
                for y in y0..=y1 {
                    let v = y as f32 / rows as f32;
                    let py = rect.h * v;
                    let dy = (radius - py.min(rect.h - py)).max(0.);
                    let inset = if radius > 0. {
                        radius - (radius * radius - dy * dy).max(0.).sqrt()
                    } else {
                        0.
                    };
                    for x in x0..=x1 {
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
                for y in 0..tile_rows {
                    for x in 0..tile_columns {
                        let a = offset + (y * (tile_columns + 1) + x) as u16;
                        let b = a + 1;
                        let c = b + (tile_columns + 1) as u16;
                        let d = c - 1;
                        shape.buffers.indices.extend([a, b, c, a, c, d]);
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::Color,
        shader::{Brush, ColorStop, LinearGradient},
    };

    fn assert_shape_limits(shapes: &Shapes) {
        for shape in &shapes.shapes {
            assert!(shape.buffers.vertices.len() <= MAX_SHAPE_VERTICES);
            assert!(shape.buffers.indices.len() <= MAX_SHAPE_INDICES);
            assert_eq!(shape.buffers.indices.len() % 3, 0);
            assert!(
                shape
                    .buffers
                    .indices
                    .iter()
                    .all(|&index| (index as usize) < shape.buffers.vertices.len())
            );
        }
    }

    #[test]
    fn many_glyph_quads_keep_all_geometry_without_gpu() {
        let uv = Rect::new(0.25, 0.5, 0.125, 0.25);
        let shapes = Shapes::build(|builder| {
            for index in 0..4_000 {
                // Same path as draw_texture, but None avoids creating a GPU texture.
                builder.fill_quad(
                    Rect::new((index % 100) as f32, (index / 100) as f32, 1., 1.),
                    0.,
                    None,
                    Some(uv),
                    Color::WHITE,
                );
            }
        });

        assert_shape_limits(&shapes);
        assert_eq!(
            shapes
                .shapes
                .iter()
                .map(|s| s.buffers.indices.len())
                .sum::<usize>(),
            24_000
        );
        assert_eq!(
            shapes
                .shapes
                .iter()
                .map(|s| s.buffers.vertices.len())
                .sum::<usize>(),
            16_000
        );
        assert!(shapes.shapes.len() > 1);
        let mut glyph = 0;
        for shape in &shapes.shapes {
            for indices in shape.buffers.indices.as_chunks::<6>().0 {
                let x = (glyph % 100) as f32;
                let y = (glyph / 100) as f32;
                let positions = [(x, y), (x + 1., y), (x + 1., y + 1.), (x, y + 1.)];
                let uvs = [
                    (uv.x, uv.y),
                    (uv.x + uv.w, uv.y),
                    (uv.x + uv.w, uv.y + uv.h),
                    (uv.x, uv.y + uv.h),
                ];
                for (&index, corner) in indices.iter().zip([0, 1, 2, 0, 2, 3]) {
                    let vertex = &shape.buffers.vertices[index as usize];
                    assert_eq!((vertex.position.x, vertex.position.y), positions[corner]);
                    assert_eq!((vertex.uv.x, vertex.uv.y), uvs[corner]);
                }
                glyph += 1;
            }
        }
        assert_eq!(glyph, 4_000);
    }

    #[test]
    fn tessellated_mesh_is_split_without_loss() {
        let mut buffers = VertexBuffers::<Vertex, u32>::new();
        // More than u16::MAX input vertices must still remap without wrapping.
        for index in 0..22_000u32 {
            let base = buffers.vertices.len() as u32;
            buffers.vertices.extend([
                Vertex::new(index as f32, 0., 0., 0., 0., macroquad::color::WHITE),
                Vertex::new(index as f32, 1., 0., 0., 0., macroquad::color::WHITE),
                Vertex::new(index as f32, 0., 1., 0., 0., macroquad::color::WHITE),
            ]);
            buffers.indices.extend([base, base + 1, base + 2]);
        }
        let mut shapes = Shapes::default();
        append_mesh(&mut shapes, None, buffers);

        assert_shape_limits(&shapes);
        assert_eq!(
            shapes
                .shapes
                .iter()
                .map(|s| s.buffers.indices.len())
                .sum::<usize>(),
            66_000
        );
        assert!(shapes.shapes.len() > 1);
        for (triangle, indices) in shapes
            .shapes
            .iter()
            .flat_map(|shape| {
                shape
                    .buffers
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(move |indices| (shape, indices))
            })
            .enumerate()
        {
            let (shape, indices) = indices;
            for &index in indices {
                assert_eq!(
                    shape.buffers.vertices[index as usize].position.x,
                    triangle as f32
                );
            }
        }
    }

    #[test]
    fn large_fill_and_stroke_paths_use_bounded_batches() {
        let mut path = lyon::path::Path::builder();
        for index in 0..2_000 {
            path.add_rectangle(
                &lyon::geom::Box2D::from_origin_and_size(
                    lyon::geom::point(index as f32 * 3., 0.),
                    lyon::geom::size(1., 1.),
                ),
                lyon::path::Winding::Positive,
            );
        }
        let path = path.build();
        let fill = Shapes::build(|builder| builder.fill_path(path.iter(), Color::WHITE));
        let stroke = Shapes::build(|builder| builder.stroke_path(path.iter(), Color::WHITE));
        for shapes in [&fill, &stroke] {
            assert_shape_limits(shapes);
            assert!(shapes.shapes.len() > 1);
        }
        assert_eq!(
            fill.shapes
                .iter()
                .map(|s| s.buffers.indices.len())
                .sum::<usize>(),
            12_000
        );
    }

    #[test]
    fn custom_grid_larger_than_u16_is_tiled() {
        struct DenseShader;
        impl Shader for DenseShader {
            fn subdivisions(&self, _: Rect) -> (usize, usize) {
                (256, 256)
            }
            fn texture(&self) -> Option<SharedTexture> {
                None
            }
            fn new_vertex(&self, mat: &Matrix, point: &Vector, alpha: f32) -> Vertex {
                Color::WHITE.new_vertex(mat, point, alpha)
            }
        }
        let shapes = Shapes::build(|builder| {
            builder.fill_rect(Rect::new(0., 0., 256., 256.), DenseShader);
        });
        assert_shape_limits(&shapes);
        assert_eq!(
            shapes
                .shapes
                .iter()
                .map(|s| s.buffers.indices.len())
                .sum::<usize>(),
            256 * 256 * 6
        );
    }

    #[test]
    fn maximum_gradient_subdivision_is_split_without_loss() {
        let brush = Brush::LinearGradient(LinearGradient::new(
            vec2(0., 0.),
            vec2(1., 1.),
            [
                ColorStop::new(0., Color::BLACK),
                ColorStop::new(1., Color::WHITE),
            ],
        ));
        let shapes = Shapes::build(|builder| {
            builder.fill_rect(
                Rect::new(0., 0., 192., 192.),
                brush.shader(Rect::new(0., 0., 192., 192.)),
            );
        });

        assert_shape_limits(&shapes);
        assert_eq!(
            shapes
                .shapes
                .iter()
                .map(|s| s.buffers.indices.len())
                .sum::<usize>(),
            32 * 32 * 6
        );
        assert!(shapes.shapes.len() > 1);
    }
}
