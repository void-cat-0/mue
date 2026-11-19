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
