use macroquad::{
    math::{Mat4, Vec4Swizzles, vec4},
    miniquad as mq,
    window::get_internal_gl,
};
use mue_core::signal::Access;

use crate::{
    math::{Rect, vec2},
    paint::Paint,
};

#[derive(Clone, Copy)]
pub(super) struct ShadowProjection {
    pub projection: Mat4,
    pub model: Mat4,
    pub viewport: Rect,
    /// Nonempty overlap of the target, viewport and scissor in QuadGl top-left pixels.
    pub clip: Option<Rect>,
}

impl ShadowProjection {
    pub fn for_draw(paint: &Paint) -> Option<Self> {
        // A Paint outside node mounting can transform the submitted vertices
        // on the CPU. Don't assume those vertices equal the layout rectangle.
        if paint.transform.get() != crate::math::Matrix::IDENTITY {
            return None;
        }
        let gl = unsafe { get_internal_gl() };
        let (width, height) = if let Some(pass) = gl.quad_gl.get_active_render_pass() {
            let [texture] = gl.quad_context.render_pass_color_attachments(pass) else {
                return None;
            };
            gl.quad_context.texture_size(*texture)
        } else {
            // QuadGl converts both the viewport and scissor using this size.
            let (width, height) = mq::window::screen_size();
            (width as u32, height as u32)
        };
        let (width, height) = (width as i32, height as i32);
        let raw_viewport = gl.quad_gl.get_viewport_raw();
        // GL may silently clamp unusual viewport dimensions. Keep its
        // analytic fallback rather than assume the requested size was used.
        if raw_viewport.is_some_and(|(_, _, w, h)| w > width || h > height)
            && matches!(gl.quad_context.info().backend, mq::Backend::OpenGl)
        {
            return None;
        }
        Self::new(
            gl.quad_gl.get_projection_matrix(),
            gl.quad_gl.get_model_matrix(),
            (width, height),
            raw_viewport,
            gl.quad_gl.get_scissor(),
        )
    }

    fn new(
        projection: Mat4,
        model: Mat4,
        (width, height): (i32, i32),
        raw_viewport: Option<(i32, i32, i32, i32)>,
        scissor: Option<(i32, i32, i32, i32)>,
    ) -> Option<Self> {
        if width <= 0 || height <= 0 || !projection.is_finite() || !model.is_finite() {
            return None;
        }
        // A negative GL viewport/scissor is invalid, not an empty region.
        let (vx, vy, vw, vh) = raw_viewport.unwrap_or((0, 0, width, height));
        if vw < 0 || vh < 0 {
            return None;
        }
        let [vx, vy, vw, vh] = [vx, vy, vw, vh].map(i64::from);
        let viewport_y = i64::from(height) - vy - vh;
        let viewport = Rect::new(vx as f32, viewport_y as f32, vw as f32, vh as f32);

        let (x, y, w, h) = scissor.unwrap_or((0, 0, width, height));
        if w < 0 || h < 0 {
            return None;
        }
        // QuadGl lowers a top-left scissor using this i32 expression. Don't
        // infer a clip if the submitted coordinates cannot be represented.
        height.checked_sub(y.checked_add(h)?)?;
        let [x, y, w, h] = [x, y, w, h].map(i64::from);
        // Intersect framebuffer, viewport and scissor in top-left pixels
        // before conversion to f32 so narrow overlaps cannot round away.
        let left = vx.max(x).max(0);
        let right = (vx + vw).min(x + w).min(i64::from(width));
        let top = viewport_y.max(y).max(0);
        let bottom = (viewport_y + vh).min(y + h).min(i64::from(height));
        let clip = (right > left && bottom > top).then(|| {
            Rect::new(
                left as f32,
                top as f32,
                (right - left) as f32,
                (bottom - top) as f32,
            )
        });
        Some(Self {
            projection,
            model,
            viewport,
            clip,
        })
    }

    pub fn outside(self, rect: Rect) -> bool {
        let Some(clip) = self.clip else {
            return true;
        };
        let mut min = vec2(f32::INFINITY, f32::INFINITY);
        let mut max = vec2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        // Mirror UI_VERTEX: Projection * (Model * vertex). Keep two pixels of
        // slack for rasterization/rounding and fail open for invalid inputs or
        // projections that can cross the homogeneous near plane.
        let abs_xform = |matrix: Mat4| {
            Mat4::from_cols(
                matrix.x_axis.abs(),
                matrix.y_axis.abs(),
                matrix.z_axis.abs(),
                matrix.w_axis.abs(),
            )
        };
        let abs_model = abs_xform(self.model);
        let abs_projection = abs_xform(self.projection);
        for point in [
            rect.min(),
            vec2(rect.right(), rect.y),
            rect.max(),
            vec2(rect.x, rect.bottom()),
        ] {
            let vertex = vec4(point.x, point.y, 0., 1.);
            let p = self.projection * (self.model * vertex);
            let magnitude = abs_projection * (abs_model * vertex.abs());
            let error = magnitude * (16. * f32::EPSILON);
            if !p.is_finite() || !error.is_finite() || p.w <= error.w.max(f32::EPSILON) {
                return false;
            }
            let ndc = p.xy() / p.w;
            // Bound error in the perspective divide, including cancellation in
            // either transform. Keep very large or ill-conditioned cases safe.
            let ndc_error = (error.xy() + ndc.abs() * error.w) / (p.w - error.w);
            let half = self.viewport.size() * 0.5;
            let screen = self.viewport.min() + vec2(1. + ndc.x, 1. - ndc.y) * half;
            let slack = vec2(ndc_error.x, ndc_error.y) * half
                + vec2(2., 2.)
                + (screen.abs() + self.viewport.min().abs() + half) * (8. * f32::EPSILON);
            if !screen.is_finite() || !slack.is_finite() {
                return false;
            }
            min = min.min(screen - slack);
            max = max.max(screen + slack);
        }
        max.x < clip.x || min.x > clip.right() || max.y < clip.y || min.y > clip.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(
        viewport: Option<(i32, i32, i32, i32)>,
        clip: Option<(i32, i32, i32, i32)>,
    ) -> ShadowProjection {
        ShadowProjection::new(
            Mat4::orthographic_rh_gl(0., 200., 160., 0., -1., 1.),
            Mat4::IDENTITY,
            (200, 160),
            viewport,
            clip,
        )
        .unwrap()
    }

    #[test]
    fn culls_all_edges_but_keeps_a_visible_halo_and_touching_edges() {
        let projection = project(None, None);
        for rect in [
            Rect::new(-48., 30., 30., 60.),
            Rect::new(205., 30., 20., 60.),
            Rect::new(30., -50., 40., 40.),
            Rect::new(30., 165., 40., 40.),
        ] {
            assert!(projection.outside(rect), "{rect:?}");
        }
        assert!(!projection.outside(Rect::new(-20., 15., 28., 8.)));
        assert!(!projection.outside(Rect::new(200., 25., 5., 5.)));
    }

    #[test]
    fn viewport_scissor_and_host_model_use_the_same_pixel_coordinates() {
        let mut projection = project(Some((10, 30, 140, 100)), Some((50, 52, 25, 25)));
        assert_eq!(projection.viewport, Rect::new(10., 30., 140., 100.));
        let rect = Rect::new(0., 0., 30., 30.);
        assert!(projection.outside(rect));
        projection.model = Mat4::from_translation(macroquad::math::vec3(75., 42., 0.))
            * Mat4::from_rotation_z(0.25);
        assert!(!projection.outside(rect));
        assert!(project(None, Some((0, 0, 0, 100))).outside(rect));
    }

    #[test]
    fn framebuffer_bounds_apply_even_with_an_explicit_scissor() {
        let partial = project(Some((180, -20, 100, 120)), Some((190, 80, 100, 100)));
        assert_eq!(partial.viewport, Rect::new(180., 60., 100., 120.));
        assert_eq!(partial.clip, Some(Rect::new(190., 80., 10., 80.)));

        let outside = project(Some((200, 0, 100, 160)), Some((200, 0, 90, 160)));
        assert!(outside.outside(Rect::new(10., 20., 30., 40.)));
    }

    #[test]
    fn scissor_intersection_preserves_a_one_pixel_sliver() {
        let projection = project(None, Some((-1_000_000_000, 0, 1_000_000_001, 100)));
        assert_eq!(projection.clip, Some(Rect::new(0., 0., 1., 100.)));
        assert!(!projection.outside(Rect::new(-5., 8., 7., 10.)));
    }

    #[test]
    fn uncertain_or_overflowing_coordinates_are_not_culled() {
        for scissor in [(0, 0, -1, 80), (0, i32::MAX, 1, 1), (0, i32::MIN, 1, 1)] {
            assert!(
                ShadowProjection::new(
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    (200, 160),
                    None,
                    Some(scissor),
                )
                .is_none()
            );
        }
        let mut projection = project(None, None);
        projection.model.w_axis.w = -1.;
        assert!(!projection.outside(Rect::new(2000., 2000., 10., 10.)));
        assert!(!project(None, None).outside(Rect::new(f32::INFINITY, 50., 40., 40.)));
    }
}
