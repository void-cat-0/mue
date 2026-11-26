// Painting: the paint helpers below, colors, brushes and shaders, materials,
// textures, paths and SVG, clipping, shadows, offscreen layers, and render state.
pub mod clip;
pub mod color;
pub mod material;
pub mod offscreen;
pub mod path;
pub mod render;
pub mod shader;
pub mod shadow;
pub mod svg;
pub mod texture;

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
    math::{CornerRadii, Edges, Matrix, Rect, Vector, vec2},
    paint::path::{CornerShape, FillRule, PathData, StrokeStyle},
    paint::shader::{Brush, IntoShader, Shader, SharedTexture},
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
/// Check both the backend and the currently selected material before building
/// retained geometry. Unmatched custom materials keep the immediate path.
pub(crate) fn retained_enabled() -> bool {
    macroquad::retained::is_supported() && crate::paint::material::retained_material_matches()
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

    /// Build shapes with this paint's transform and opacity without drawing
    /// them, for callers that keep the geometry while its inputs are unchanged.
    pub fn capture(&self, f: impl FnOnce(&mut ShapesBuilder)) -> Rc<Shapes> {
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
        Rc::new(shapes)
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

    /// Submit a solid quad without allocating temporary Shapes or tessellation buffers.
    pub(crate) fn draw_solid_rect(&self, rect: Rect, color: crate::paint::color::Color) {
        self.draw_quad(rect, color, None);
    }

    /// Bind a cached mask. Its material derives high-precision UVs from local position.
    pub(crate) fn draw_mask_rect(&self, rect: Rect, texture: &Texture2D) {
        self.draw_quad(rect, crate::paint::color::Color::WHITE, Some(texture));
    }

    fn draw_quad(
        &self,
        rect: Rect,
        color: crate::paint::color::Color,
        texture: Option<&Texture2D>,
    ) {
        let transform = self.transform.get();
        let alpha = self.opacity.get();
        // Row-major corners and the diagonal match ShapesBuilder::fill_quad.
        let vertices = [
            rect.min(),
            vec2(rect.right(), rect.y),
            vec2(rect.x, rect.bottom()),
            rect.max(),
        ]
        .map(|point| color.new_vertex(&transform, &point, alpha));
        const INDICES: [u16; 6] = [0, 1, 3, 0, 3, 2];
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(texture);
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(&vertices, &INDICES);
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
    pub(crate) fn retained_batches(&self) -> impl Iterator<Item = RetainedBatch> + '_ {
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

/// Logical-to-device scale of `transform` (pixels per local unit).
fn device_scale(transform: Matrix) -> f32 {
    let det = (transform.x_axis.x * transform.y_axis.y - transform.x_axis.y * transform.y_axis.x)
        .abs()
        .sqrt();
    let dpi = crate::app::viewport::current_dpi_scale();
    let scale = det * dpi;
    if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    }
}

/// Closed contours of a filled region and, for each, whether it bounds a hole.
pub(crate) struct Contours {
    points: Vec<Vec<Vector>>,
    holes: Vec<bool>,
}

impl Contours {
    fn single(points: Vec<Vector>) -> Self {
        Self {
            holes: vec![false],
            points: vec![points],
        }
    }

    /// Flatten path events into contours and classify holes by `rule`.
    fn from_events(events: &[PathEvent], rule: FillRule, tolerance: f32) -> Self {
        use lyon::path::iterator::PathIterator;
        let mut points: Vec<Vec<Vector>> = Vec::new();
        let mut current: Vec<Vector> = Vec::new();
        for event in events.iter().copied().flattened(tolerance) {
            match event {
                PathEvent::Begin { at } => {
                    current.clear();
                    current.push(vec2(at.x, at.y));
                }
                PathEvent::Line { to, .. } => current.push(vec2(to.x, to.y)),
                PathEvent::End { .. } => {
                    if current.len() >= 3 {
                        points.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                }
                _ => {}
            }
        }
        let areas: Vec<f32> = points
            .iter()
            .map(|c| crate::paint::path::signed_area(c))
            .collect();
        let holes = (0..points.len())
            .map(|i| {
                let probe = points[i][0];
                let containers: Vec<usize> = (0..points.len())
                    .filter(|&j| j != i && crate::paint::path::polygon_contains(&points[j], probe))
                    .collect();
                match rule {
                    FillRule::EvenOdd => containers.len() % 2 == 1,
                    // Nonzero: a contour winding against its innermost container cuts a hole.
                    FillRule::NonZero => containers
                        .iter()
                        .min_by(|&&a, &&b| areas[a].abs().total_cmp(&areas[b].abs()))
                        .is_some_and(|&j| areas[j].signum() != areas[i].signum()),
                }
            })
            .collect();
        Self { points, holes }
    }
}

/// Antialias the edges of a filled region: a strip one device pixel wide
/// along every contour, fading from the fill color to transparent away from
/// the filled side. It keeps the fill batched with everything else.
fn push_fringe(
    buffers: &mut VertexBuffers<Vertex, u32>,
    shader: &impl Shader,
    transform: Matrix,
    alpha: f32,
    contours: &Contours,
) {
    let width = 1. / device_scale(transform);
    for (points, &hole) in contours.points.iter().zip(&contours.holes) {
        let n = points.len();
        if n < 3 {
            continue;
        }
        let area = crate::paint::path::signed_area(points);
        if area.abs() <= f32::EPSILON {
            continue;
        }
        // Normals point away from the contour's own region; a hole's fringe
        // points into it, away from the filled side.
        let outward = if (area > 0.) != hole { 1. } else { -1. };
        let normal = |a: Vector, b: Vector| {
            let d = (b - a).normalize_or_zero();
            vec2(d.y, -d.x) * outward
        };
        let base = buffers.vertices.len() as u32;
        for i in 0..n {
            let prev = points[(i + n - 1) % n];
            let point = points[i];
            let next = points[(i + 1) % n];
            let (n0, n1) = (normal(prev, point), normal(point, next));
            let miter = (n0 + n1).normalize_or_zero();
            let along = if miter == Vector::ZERO { n1 } else { miter };
            let scale = along.dot(n1).max(0.35);
            let outer = point + along * (width / scale);
            buffers
                .vertices
                .push(shader.new_vertex(&transform, &point, alpha));
            buffers
                .vertices
                .push(shader.new_vertex(&transform, &outer, 0.));
        }
        for i in 0..n as u32 {
            let j = (i + 1) % n as u32;
            let (a, b, c, d) = (
                base + i * 2,
                base + i * 2 + 1,
                base + j * 2 + 1,
                base + j * 2,
            );
            buffers.indices.extend([a, b, c, a, c, d]);
        }
    }
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
        self.build_with_fringe(shapes, shader, None, f);
    }

    fn build_with_fringe<S: Shader>(
        &mut self,
        shapes: &mut Shapes,
        shader: S,
        fringe: Option<&Contours>,
        f: impl FnOnce(
            &mut Cache,
            &mut BuffersBuilder<Vertex, u32, ShaderConstructor<S>>,
        ) -> TessellationResult,
    ) {
        let texture = shader.texture();
        // The fringe borrows the shader before tessellation takes it.
        let mut edge = VertexBuffers::<Vertex, u32>::new();
        if let Some(contours) = fringe {
            push_fringe(&mut edge, &shader, self.transform, self.alpha, contours);
        }
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
        append_mesh(shapes, texture.clone(), buffers);
        append_mesh(shapes, texture, edge);
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
            CornerRadii::ZERO,
            Some(texture),
            Some(uv_rect),
            color.into_shader(),
        );
    }

    fn fill_quad(
        &mut self,
        rect: Rect,
        radii: CornerRadii,
        texture: Option<SharedTexture>,
        uv: Option<Rect>,
        shader: impl Shader,
    ) {
        let (columns, rows) = shader.subdivisions(rect);
        let columns = columns.max(1);
        // Row positions (0..=1). Rounded corners add rows at the heights of
        // the corner outline's vertices, so the grid's edge is exactly the
        // antialiased outline and no gap opens between them.
        let mut stops: Vec<f32> = (0..=rows.max(1))
            .map(|y| y as f32 / rows.max(1) as f32)
            .collect();
        if !radii.is_zero() && rect.h > 0. {
            let scale = device_scale(self.inner.transform);
            for (radius, top) in [
                (radii.top_left, true),
                (radii.top_right, true),
                (radii.bottom_right, false),
                (radii.bottom_left, false),
            ] {
                if radius <= 0. {
                    continue;
                }
                let segments = crate::paint::path::corner_segments(radius, scale);
                for i in 0..=segments {
                    let t = i as f32 / segments as f32 * std::f32::consts::FRAC_PI_2;
                    let depth = radius * (1. - t.sin());
                    let y = if top { depth } else { rect.h - depth };
                    stops.push((y / rect.h).clamp(0., 1.));
                }
            }
            stops.sort_by(f32::total_cmp);
            stops.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        }
        let rows = stops.len() - 1;
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
                for &v in &stops[y0..=y1] {
                    let py = rect.h * v;
                    let left = corner_inset(radii.top_left, radii.bottom_left, py, rect.h);
                    let right = corner_inset(radii.top_right, radii.bottom_right, py, rect.h);
                    for x in x0..=x1 {
                        let u = x as f32 / columns as f32;
                        let point = vec2(rect.x + left + (rect.w - left - right) * u, rect.y + py);
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

    /// Fill a path with the given shader (nonzero rule).
    pub fn fill_path<I, S>(&mut self, path: I, shader: S)
    where
        I: IntoIterator<Item = PathEvent>,
        S: IntoShader,
    {
        let events: Vec<PathEvent> = path.into_iter().collect();
        let contours =
            Contours::from_events(&events, FillRule::NonZero, self.fill_options.tolerance);
        let options = self.fill_options;
        self.inner.build_with_fringe(
            self.shapes,
            shader.into_shader(),
            Some(&contours),
            |cache, builder| {
                cache
                    .fill_tess
                    .tessellate(events.iter().copied(), &options, builder)
            },
        );
    }

    /// Fill a path with the given rule. Edges are antialiased.
    pub fn fill<S: IntoShader>(&mut self, path: &PathData, rule: FillRule, shader: S) {
        let options = self.fill_options.with_fill_rule(rule.into());
        let events: Vec<PathEvent> = path.iter().collect();
        let contours = Contours::from_events(&events, rule, self.fill_options.tolerance);
        self.inner.build_with_fringe(
            self.shapes,
            shader.into_shader(),
            Some(&contours),
            |cache, builder| {
                cache
                    .fill_tess
                    .tessellate(events.iter().copied(), &options, builder)
            },
        );
    }

    /// Stroke a path with the given line style.
    pub fn stroke<S: IntoShader>(&mut self, path: &PathData, style: &StrokeStyle, shader: S) {
        if style.width <= 0. {
            return;
        }
        let options = style.options(self.stroke_options.tolerance);
        self.inner
            .build(self.shapes, shader.into_shader(), |cache, builder| {
                cache.stroke_tess.tessellate(path.iter(), &options, builder)
            });
    }

    /// Fill an axis-aligned rectangle. Interior samples follow the shader.
    pub(crate) fn fill_rect<S>(&mut self, rect: Rect, shader: S)
    where
        S: IntoShader,
    {
        let shader = shader.into_shader();
        self.fill_quad(rect, CornerRadii::ZERO, shader.texture(), None, shader);
    }

    /// Fill a rectangle with per-corner radii, sampling the interior for gradients.
    pub(crate) fn fill_rounded_rect<S>(&mut self, rect: Rect, radii: CornerRadii, shader: S)
    where
        S: IntoShader,
    {
        self.fill_shaped_rect(rect, radii, CornerShape::Round, shader);
    }

    /// Fill a rectangle whose corners have `radii` and `shape`, with
    /// antialiased edges.
    pub(crate) fn fill_shaped_rect<S>(
        &mut self,
        rect: Rect,
        radii: CornerRadii,
        shape: CornerShape,
        shader: S,
    ) where
        S: IntoShader,
    {
        let shader = shader.into_shader();
        let radii = radii.fit(rect);
        if radii.is_zero() {
            self.fill_quad(rect, radii, shader.texture(), None, shader);
            return;
        }
        let mut outline = Vec::new();
        crate::paint::path::corner_outline(
            rect,
            radii,
            shape,
            device_scale(self.inner.transform),
            &mut outline,
        );
        let contours = Contours::single(outline);
        // Boundary-only tessellation misses the center of a radial gradient.
        // Round corners sample the interior on a grid curved around the corners.
        if shader.subdivisions(rect) != (1, 1) && shape == CornerShape::Round {
            let mut edge = VertexBuffers::<Vertex, u32>::new();
            push_fringe(
                &mut edge,
                &shader,
                self.inner.transform,
                self.inner.alpha,
                &contours,
            );
            let texture = shader.texture();
            self.fill_quad(rect, radii, texture.clone(), None, shader);
            append_mesh(self.shapes, texture, edge);
            return;
        }
        let path = polygon_path(&contours.points);
        let options = self.fill_options;
        self.inner
            .build_with_fringe(self.shapes, shader, Some(&contours), |cache, builder| {
                cache.fill_tess.tessellate(path.iter(), &options, builder)
            });
    }

    /// Fill the ring between two rectangles whose corners share `shape`,
    /// with antialiased edges.
    pub(crate) fn fill_shaped_ring<S>(
        &mut self,
        outer: (Rect, CornerRadii),
        inner: (Rect, CornerRadii),
        shape: CornerShape,
        shader: S,
    ) where
        S: IntoShader,
    {
        let scale = device_scale(self.inner.transform);
        let mut outer_points = Vec::new();
        crate::paint::path::corner_outline(outer.0, outer.1, shape, scale, &mut outer_points);
        let mut inner_points = Vec::new();
        crate::paint::path::corner_outline(inner.0, inner.1, shape, scale, &mut inner_points);
        if outer_points.len() < 3 {
            return;
        }
        let contours = if inner_points.len() >= 3 {
            Contours {
                points: vec![outer_points, inner_points],
                holes: vec![false, true],
            }
        } else {
            Contours::single(outer_points)
        };
        let path = polygon_path(&contours.points);
        let options = self
            .fill_options
            .with_fill_rule(lyon::tessellation::FillRule::EvenOdd);
        self.inner.build_with_fringe(
            self.shapes,
            shader.into_shader(),
            Some(&contours),
            |cache, builder| cache.fill_tess.tessellate(path.iter(), &options, builder),
        );
    }
}

/// A lyon path of closed polygons.
fn polygon_path(contours: &[Vec<Vector>]) -> lyon::path::Path {
    let mut builder = lyon::path::Path::builder();
    for contour in contours {
        let mut points = contour.iter();
        let Some(first) = points.next() else {
            continue;
        };
        builder.begin(lyon::math::point(first.x, first.y));
        for point in points {
            builder.line_to(lyon::math::point(point.x, point.y));
        }
        builder.end(true);
    }
    builder.build()
}

/// Horizontal inset of a rounded edge at height `py` within a box of height `h`.
fn corner_inset(top: f32, bottom: f32, py: f32, h: f32) -> f32 {
    let inset = |radius: f32, distance: f32| {
        if radius <= 0. || distance >= radius {
            return 0.;
        }
        let dy = radius - distance;
        radius - (radius * radius - dy * dy).max(0.).sqrt()
    };
    inset(top, py).max(inset(bottom, h - py))
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
}

/// Resolve the four corner radius properties into one reactive value.
pub(crate) fn take_corner_radii(style: &mut Style) -> Prop<CornerRadii> {
    use crate::style::units::resolve_length;
    let corners = [
        resolve_length(style.take_radius_top_left(), "radius"),
        resolve_length(style.take_radius_top_right(), "radius"),
        resolve_length(style.take_radius_bottom_right(), "radius"),
        resolve_length(style.take_radius_bottom_left(), "radius"),
    ];
    if let [
        Prop::Static(top_left),
        Prop::Static(top_right),
        Prop::Static(bottom_right),
        Prop::Static(bottom_left),
    ] = corners
    {
        return Prop::Static(CornerRadii {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        });
    }
    let [top_left, top_right, bottom_right, bottom_left] = corners;
    computed(move |_| CornerRadii {
        top_left: top_left.get(),
        top_right: top_right.get(),
        bottom_right: bottom_right.get(),
        bottom_left: bottom_left.get(),
    })
    .into()
}

/// The declared corner radii without consuming them (so `use_background` can
/// still take them).
pub(crate) fn declared_corner_radii(style: &Style) -> Prop<CornerRadii> {
    let mut copy = Style::new();
    if let Some(value) = style.declared_radius_top_left() {
        copy.put_radius_top_left(value);
    }
    if let Some(value) = style.declared_radius_top_right() {
        copy.put_radius_top_right(value);
    }
    if let Some(value) = style.declared_radius_bottom_right() {
        copy.put_radius_bottom_right(value);
    }
    if let Some(value) = style.declared_radius_bottom_left() {
        copy.put_radius_bottom_left(value);
    }
    take_corner_radii(&mut copy)
}

fn is_transparent(brush: &Prop<Brush>) -> bool {
    matches!(brush, Prop::Static(Brush::Solid(color)) if color.a <= 0.)
}

/// Cache background, border, and outline geometry for a node.
///
/// The drop shadow, background, and border draw before the node's content
/// hooks; the outline draws after its children. Border widths come from the
/// node's layout (`border` widths); `border_color` paints them.
pub fn use_background(style: &mut Style, rect: ReadSignal<Rect>) {
    let background = style.take_background();
    let radii = take_corner_radii(style);
    let border_color = style.take_border_color();
    let outline_width =
        crate::style::units::resolve_length(style.take_outline_width(), "outline-width");
    let outline_offset =
        crate::style::units::resolve_length(style.take_outline_offset(), "outline-offset");
    let outline_color = style.take_outline_color();
    let shadows = style.take_box_shadow();
    let shadow_cache = style.take_shadow_cache();
    let backdrop_blur =
        crate::style::units::resolve_length(style.take_backdrop_blur(), "backdrop-blur");
    let backdrop = (!matches!(backdrop_blur, Prop::Static(blur) if blur <= 0.))
        .then(crate::paint::offscreen::backdrop_user);
    let mut shadow_masks: Vec<crate::paint::shadow::ShadowMaskState> = Vec::new();
    let paint = use_paint(style);
    let border = crate::node::NodeInner::with_mut(|node| *node.layout.border);
    // Default transparent containers need no cache, even during inherited opacity changes.
    let shapes = if is_transparent(&background) && is_transparent(&border_color) {
        None
    } else {
        Some(paint.build(move |p| {
            let rect = rect.get();
            if rect.w <= 0. || rect.h <= 0. {
                return;
            }
            let radii = radii.get().fit(rect);
            let shape = CornerShape::Round;
            let brush = background.get_clone();
            if !matches!(&brush, Brush::Solid(color) if color.a <= 0.) {
                if radii.is_zero() {
                    p.fill_rect(rect, brush.shader(rect));
                } else {
                    p.fill_shaped_rect(rect, radii, shape, brush.shader(rect));
                }
            }
            let border_brush = border_color.get_clone();
            let widths: Edges = if border.is_null() {
                Edges::ZERO
            } else {
                border.get()
            };
            if !widths.is_zero() && !matches!(&border_brush, Brush::Solid(color) if color.a <= 0.) {
                let inner = rect.deflate(widths);
                p.fill_shaped_ring(
                    (rect, radii),
                    (inner, radii.inset(widths)),
                    shape,
                    border_brush.shader(rect),
                );
            }
        }))
    };
    let outline = if matches!(outline_width, Prop::Static(width) if width <= 0.)
        || is_transparent(&outline_color)
    {
        None
    } else {
        Some(paint.build(move |p| {
            let rect = rect.get();
            let width = outline_width.get();
            if rect.w <= 0. || rect.h <= 0. || width <= 0. {
                return;
            }
            let offset = outline_offset.get();
            let radii = radii.get().fit(rect);
            let inner = rect.outset(offset);
            let outer = inner.outset(width);
            p.fill_shaped_ring(
                (outer, radii.outset(offset + width)),
                (inner, radii.outset(offset)),
                CornerShape::Round,
                outline_color.get_clone().shader(outer),
            );
        }))
    };
    if let Some(outline) = outline {
        crate::node::on_after_clip(move |_| outline.get_clone().draw());
    }
    style.on_render.append(move |_| {
        let rect = rect.get();
        if rect.w <= 0. || rect.h <= 0. {
            shadow_masks.clear();
            return;
        }
        match shadows.try_get_clone() {
            Some(list) if !list.is_empty() => crate::paint::shadow::draw_box_shadows(
                &paint,
                rect,
                radii.get().fit(rect),
                &list,
                shadow_cache.get(),
                &mut shadow_masks,
            ),
            _ => shadow_masks.clear(),
        }
        if backdrop.is_some() {
            let blur = backdrop_blur.get();
            if blur > 0. {
                let radii = radii.get().fit(rect);
                crate::paint::offscreen::draw_backdrop(blur, |texture| {
                    let texture = SharedTexture::from(texture.clone());
                    paint.draw(|p| {
                        let shader = crate::paint::shader::TextureShader::new(
                            texture,
                            Rect::new(0., 0., 1., 1.),
                            rect,
                            crate::paint::color::Color::WHITE,
                        );
                        if radii.is_zero() {
                            p.fill_rect(rect, shader);
                        } else {
                            p.fill_shaped_rect(rect, radii, CornerShape::Round, shader);
                        }
                    });
                });
            }
        }
        // Keep submission on the immediate path so neighboring backgrounds can batch.
        if let Some(shapes) = shapes {
            shapes.get_clone().draw();
        }
    });
}
