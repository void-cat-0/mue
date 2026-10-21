use mue_core::prelude::Access;

use crate::{
    layout::use_layout,
    math::Matrix,
    paint::{Shapes, use_paint},
    style::Style,
};
use std::rc::Rc;

/// Draw prebuilt shapes in local coordinates within a node's layout and lifetime.
#[mue_macros::node]
pub fn shape(style: &mut Style, shapes: Rc<Shapes>) {
    let rect = use_layout(style).rect;
    let paint = use_paint(style);
    style.on_render.append(move |_| {
        let translation = Matrix::from_translation(rect.get().min());
        let transform = paint.transform.get() * translation;
        let matrix = crate::math::model_matrix(transform);
        let gl = unsafe { macroquad::window::get_internal_gl() }.quad_gl;
        gl.push_model_matrix(matrix);
        shapes.get_clone().draw();
        unsafe { macroquad::window::get_internal_gl() }
            .quad_gl
            .pop_model_matrix();
    });
}
