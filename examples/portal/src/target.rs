//! Axis-aligned mapping local to the portal example.
//! Resolution and DPI policy belong to the host, not this mapping.

use macroquad::math::Vec2;
use mue_macroquad::math::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetMap {
    layout_rect: Rect,
    target_size: (u32, u32),
}

impl TargetMap {
    pub fn new(layout_rect: Rect, target_size: (u32, u32)) -> Self {
        Self {
            layout_rect,
            target_size,
        }
    }

    pub fn contains(&self, pos: Vec2) -> bool {
        self.layout_rect.w > 0.
            && self.layout_rect.h > 0.
            && pos.x >= self.layout_rect.x
            && pos.x <= self.layout_rect.x + self.layout_rect.w
            && pos.y >= self.layout_rect.y
            && pos.y <= self.layout_rect.y + self.layout_rect.h
    }

    pub fn window_to_target(&self, pos: Vec2) -> Option<Vec2> {
        self.contains(pos)
            .then(|| self.window_to_target_unclamped(pos))
    }

    pub fn window_to_target_unclamped(&self, pos: Vec2) -> Vec2 {
        Vec2::new(
            (pos.x - self.layout_rect.x) / self.layout_rect.w.max(f32::EPSILON)
                * self.target_size.0 as f32,
            (pos.y - self.layout_rect.y) / self.layout_rect.h.max(f32::EPSILON)
                * self.target_size.1 as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ChartInput;

    fn map() -> TargetMap {
        TargetMap::new(Rect::new(10., 20., 100., 50.), (200, 100))
    }

    #[test]
    fn maps_logical_rect_to_actual_target_pixels() {
        let map = map();
        assert_eq!(
            map.window_to_target(Vec2::new(60., 45.)),
            Some(Vec2::new(100., 50.))
        );
        assert_eq!(map.window_to_target(Vec2::new(10., 20.)), Some(Vec2::ZERO));
        assert_eq!(
            map.window_to_target(Vec2::new(110., 70.)),
            Some(Vec2::new(200., 100.))
        );
    }

    #[test]
    fn target_resolution_is_independent_of_layout_and_dpi() {
        let map = TargetMap::new(Rect::new(10., 20., 100., 50.), (320, 180));
        assert_eq!(
            map.window_to_target(Vec2::new(60., 45.)),
            Some(Vec2::new(160., 90.))
        );
    }

    #[test]
    fn rejects_outside_down_and_empty_layout() {
        assert_eq!(map().window_to_target(Vec2::new(5., 5.)), None);
        assert_eq!(map().window_to_target(Vec2::new(200., 200.)), None);
        assert_eq!(
            TargetMap::new(Rect::default(), (200, 100)).window_to_target(Vec2::ZERO),
            None
        );
    }

    #[test]
    fn unclamped_mapping_keeps_outside_move_and_up() {
        let outside = map().window_to_target_unclamped(Vec2::new(5., 5.));
        assert!((outside.x + 10.).abs() < 1e-4);
        assert!((outside.y + 30.).abs() < 1e-4);
        let far = map().window_to_target_unclamped(Vec2::new(210., 120.));
        assert!((far.x - 400.).abs() < 1e-4);
        assert!((far.y - 200.).abs() < 1e-4);
    }

    #[test]
    fn moving_layout_only_changes_mapping() {
        let map = TargetMap::new(Rect::new(50., 60., 100., 50.), (200, 100));
        assert_eq!(
            map.window_to_target(Vec2::new(100., 85.)),
            Some(Vec2::new(100., 50.))
        );
    }

    #[test]
    fn chart_input_receives_target_coordinates() {
        let mut input = ChartInput::new();
        let down = input
            .pointer_down(
                1,
                map().window_to_target(Vec2::new(60., 45.)).unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(down.position, Vec2::new(100., 50.));
        let up = input
            .pointer_up(1, map().window_to_target_unclamped(Vec2::new(5., 5.)), None)
            .unwrap();
        assert!((up.position.x + 10.).abs() < 1e-4);
        assert!((up.position.y + 30.).abs() < 1e-4);
    }
}
