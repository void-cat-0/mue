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
    effect::{computed, computed_always},
    prop::Prop,
    scope::{inject, provide},
    signal::{Access, ReadSignal},
};
use smallvec::SmallVec;

use crate::{
    math::{Matrix, Rect, Vector, vec2},
    shader::{Brush, IntoShader, Shader, SharedTexture},
    style::Style,
};
use macroquad::retained::RetainedVertex;

#[derive(Default)]
struct Cache {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
    static DRAW_TRANSFORM: Cell<Matrix> = const { Cell::new(Matrix::IDENTITY) };
    static MATERIAL_CLIP_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Keeps retained geometry disabled while a clip material is active.
pub(crate) struct MaterialClipGuard;

impl Drop for MaterialClipGuard {
    fn drop(&mut self) {
        MATERIAL_CLIP_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub(crate) fn enter_material_clip() -> MaterialClipGuard {
    MATERIAL_CLIP_DEPTH.with(|depth| depth.set(depth.get() + 1));
    MaterialClipGuard
}

fn material_clip_active() -> bool {
    MATERIAL_CLIP_DEPTH.with(|depth| depth.get() > 0)
}

/// Apply a transform to a subtree without changing its layout coordinates.
pub(crate) fn with_transform<R>(transform: Matrix, draw: impl FnOnce() -> R) -> R {
    if transform == Matrix::IDENTITY {
        return draw();
    }
    let previous = DRAW_TRANSFORM.replace(DRAW_TRANSFORM.get() * transform);
    let matrix = crate::math::model_matrix(transform);
    unsafe { get_internal_gl() }
        .quad_gl
        .push_model_matrix(matrix);
    let result = draw();
    unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
    DRAW_TRANSFORM.set(previous);
    result
}

/// The accumulated transform for the node currently rendering.
pub(crate) fn current_transform() -> Matrix {
    DRAW_TRANSFORM.get()
}

/// Whether mue should use macroquad's retained geometry path.
///
/// This includes the backend check, so callers can skip building retained
/// geometry entirely on backends that only support the immediate path.
pub(crate) fn retained_enabled() -> bool {
    macroquad::retained::is_supported()
        && (!material_clip_active() || crate::clip::retained_material_active())
}

/// Axis-aligned bounds of a rectangle after a canvas transform.
pub(crate) fn transform_rect(transform: Matrix, rect: Rect) -> Rect {
    let corners = [
        rect.min(),
        rect.max(),
        vec2(rect.right(), rect.y),
        vec2(rect.x, rect.bottom()),
    ]
    .map(|point| crate::math::project_point(transform, point));
    let min = corners.into_iter().reduce(Vector::min).unwrap();
    let max = corners.into_iter().reduce(Vector::max).unwrap();
    Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
}

/// Current scissor in logical, top-left coordinates.
pub(crate) fn current_clip_logical() -> Option<Rect> {
    let scale = macroquad::window::screen_dpi_scale().max(f32::EPSILON);
    let gl = unsafe { get_internal_gl() };
    gl.quad_gl.get_scissor().map(|(x, y, w, h)| {
        Rect::new(
            x as f32 / scale,
            y as f32 / scale,
            w as f32 / scale,
            h as f32 / scale,
        )
    })
}

/// Whether `rect` intersects `clip`, or the clip is absent.
pub(crate) fn rect_visible(clip: Option<Rect>, rect: &Rect) -> bool {
    clip.is_none_or(|clip| clip.intersect(rect).is_some())
}

/// Restrict drawing to an axis-aligned rectangle, preserving an enclosing clip.
pub fn with_clip<R>(rect: Rect, draw: impl FnOnce() -> R) -> R {
    let transform = DRAW_TRANSFORM.get();
    let rect = transform_rect(transform, rect);
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
struct PaintContext {
    opacity: Prop<f32>,
}

fn combine_opacity(inherited: Option<Prop<f32>>, local: Prop<f32>) -> Prop<f32> {
    match inherited {
        Some(Prop::Static(parent)) => local.map(move |local| parent * local),
        Some(parent) => computed(move |_| parent.get() * local.get()).into(),
        None => local,
    }
}

/// Bind this node's final opacity before mounting its children or paint consumers.
///
/// Called once after transition binding, this retains the local property in Style
/// while publishing the effective alpha for both this node and its descendants.
pub(crate) fn bind_node_style(style: &mut Style) {
    let inherited = inject::<PaintContext>().map(|context| context.opacity);
    let opacity = combine_opacity(
        inherited,
        style.declared_opacity().unwrap_or(Prop::Static(1.)),
    );
    provide(PaintContext { opacity });
}

/// Draw geometry using the node's paint transform and effective inherited opacity.
pub struct Paint {
    pub transform: Prop<Matrix>,
    opacity: Prop<f32>,
    tolerance: Prop<f32>,
}

impl Paint {
    /// Effective alpha for retained geometry.
    pub(crate) fn opacity(&self) -> f32 {
        self.opacity.get()
    }

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

/// Obtain this node's already bound paint state while building its consumers.
pub fn use_paint(style: &mut Style) -> Paint {
    let (transform, opacity) = if let Some(context) = inject::<PaintContext>() {
        // Node::render applies the node transform around the entire subtree.
        // Geometry stays in layout coordinates, and local alpha is already bound.
        (Prop::Static(Matrix::IDENTITY), context.opacity)
    } else {
        // Preserve direct use outside the node mount protocol.
        (
            style
                .declared_matrix_transform()
                .unwrap_or(Prop::Static(Matrix::IDENTITY)),
            style.declared_opacity().unwrap_or(Prop::Static(1.)),
        )
    };
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

impl mue_core::prop::PropValue for Shapes {}

/// One texture batch of retained geometry.
pub(crate) struct RetainedBatch {
    pub(crate) texture: Option<SharedTexture>,
    pub(crate) vertices: Vec<RetainedVertex>,
    pub(crate) indices: Vec<u32>,
}

impl Shapes {
    /// Total vertex count across all texture batches.
    pub(crate) fn vertex_count(&self) -> usize {
        self.shapes
            .iter()
            .map(|shape| shape.buffers.vertices.len())
            .sum()
    }

    /// Convert the cached geometry into immutable buffer batches.
    pub(crate) fn retained_batches(&self) -> Vec<RetainedBatch> {
        self.shapes
            .iter()
            .filter(|shape| !shape.buffers.vertices.is_empty() && !shape.buffers.indices.is_empty())
            .map(|shape| RetainedBatch {
                texture: shape.texture.clone(),
                vertices: shape
                    .buffers
                    .vertices
                    .iter()
                    .map(|vertex| RetainedVertex {
                        position: [vertex.position.x, vertex.position.y],
                        texcoord: [vertex.uv.x, vertex.uv.y],
                        color: [
                            vertex.color[0] as f32 / 255.,
                            vertex.color[1] as f32 / 255.,
                            vertex.color[2] as f32 / 255.,
                            vertex.color[3] as f32 / 255.,
                        ],
                    })
                    .collect(),
                indices: shape
                    .buffers
                    .indices
                    .iter()
                    .map(|&index| index as u32)
                    .collect(),
            })
            .collect()
    }

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
    let shadow = style.take_shadow();
    let paint = use_paint(style);
    style.on_render.append(move |_| {
        let rect = rect.get();
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        if let Some(shadow) = shadow.try_get() {
            crate::shadow::draw_shadow(&paint, rect, shadow);
        }
        let brush = background.get_clone();
        if matches!(&brush, Brush::Solid(color) if color.a <= 0.) {
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
