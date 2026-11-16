use super::{Node, NodeContext};
use mue_core::prelude::Access;
use std::{cell::RefCell, rc::Rc};

use crate::{
    layout::use_layout,
    math::{Matrix, Rect, vec2},
    paint::{
        Shapes, current_clip_logical, current_transform, rect_visible, retained_enabled,
        transform_rect, use_paint,
    },
};
use macroquad::retained::{self, RetainedMesh, RetainedVertex};

/// Large static `Shapes` are uploaded once instead of every frame.
const RETAINED_SHAPE_MIN_VERTICES: usize = 4_096;

#[derive(Default)]
struct RetainedShapeCache {
    source: Option<Rc<Shapes>>,
    meshes: Vec<RetainedMesh>,
    /// Union of every batch's vertex positions, in shape-local space.
    bounds: Option<Rect>,
    disabled: bool,
}

fn vertex_bounds(vertices: &[RetainedVertex]) -> Option<Rect> {
    let mut iter = vertices.iter();
    let first = iter.next()?;
    let mut min = vec2(first.position[0], first.position[1]);
    let mut max = min;
    for vertex in iter {
        let position = vec2(vertex.position[0], vertex.position[1]);
        min = min.min(position);
        max = max.max(position);
    }
    Some(Rect::new(min.x, min.y, max.x - min.x, max.y - min.y))
}

/// Draw prebuilt shapes in local coordinates within a node's layout and lifetime.
#[mue_macros::node]
pub fn shape(ctx: NodeContext, shapes: Rc<Shapes>) -> Node {
    let retained_static = matches!(&shapes, mue_core::prop::Prop::Static(_));
    ctx.finish_without_defaults(move |style| {
        let rect = use_layout(style).rect;
        let paint = use_paint(style);
        let cache = Rc::new(RefCell::new(RetainedShapeCache::default()));
        style.on_render.append(move |_| {
            let rect_value = rect.get();
            if retained_static && retained_enabled() {
                let shapes_rc = shapes.get_clone();
                let disabled = cache.borrow().disabled;
                if !disabled && shapes_rc.vertex_count() >= RETAINED_SHAPE_MIN_VERTICES {
                    let mut cache = cache.borrow_mut();
                    let rebuild = cache
                        .source
                        .as_ref()
                        .is_none_or(|source| !Rc::ptr_eq(source, &shapes_rc));
                    if rebuild {
                        let mut meshes = Vec::new();
                        let mut bounds: Option<Rect> = None;
                        for batch in shapes_rc.retained_batches() {
                            let texture = batch
                                .texture
                                .as_ref()
                                .map_or_else(retained::white_texture, |texture| {
                                    texture.raw_miniquad_id()
                                });
                            if let Some(mesh) =
                                RetainedMesh::new(&batch.vertices, &batch.indices, texture)
                            {
                                if let Some(batch_bounds) = vertex_bounds(&batch.vertices) {
                                    bounds = Some(match bounds {
                                        Some(bounds) => bounds.union(&batch_bounds),
                                        None => batch_bounds,
                                    });
                                }
                                meshes.push(mesh);
                            }
                        }
                        if meshes.is_empty() {
                            cache.disabled = true;
                            cache.meshes.clear();
                            cache.bounds = None;
                            cache.source = None;
                        } else {
                            cache.meshes = meshes;
                            cache.bounds = bounds;
                            cache.source = Some(shapes_rc.clone());
                        }
                    }
                    if !cache.meshes.is_empty() {
                        let model =
                            current_transform() * Matrix::from_translation(rect_value.min());
                        if let Some(bounds) = cache.bounds {
                            let screen = transform_rect(model, bounds);
                            if !rect_visible(current_clip_logical(), &screen) {
                                return;
                            }
                        }
                        crate::clip::draw_retained_meshes(
                            &cache.meshes,
                            crate::math::model_matrix(model),
                            1.0,
                        );
                        return;
                    }
                }
            }

            let translation = Matrix::from_translation(rect_value.min());
            let transform = paint.transform.get() * translation;
            let matrix = crate::math::model_matrix(transform);
            let gl = unsafe { macroquad::window::get_internal_gl() }.quad_gl;
            gl.push_model_matrix(matrix);
            shapes.get_clone().draw();
            unsafe { macroquad::window::get_internal_gl() }
                .quad_gl
                .pop_model_matrix();
        });
    })
}
