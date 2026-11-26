//! Overlay scrollbars: a thin rounded thumb that appears while the area
//! scrolls, widens with a faint track when the mouse comes near, and fades
//! away when the area is idle.

use mue_macroquad::{
    math::{Rect, Vector},
    paint::{
        Paint,
        color::Color,
        path::{FillRule, PathData},
    },
};

/// Thumb thickness while idle and while the mouse is near.
const THIN: f32 = 4.;
const WIDE: f32 = 8.;
/// Gap between the bar and the viewport edge.
const INSET: f32 = 2.;
const MIN_THUMB: f32 = 32.;
/// Width of the edge band where the mouse widens and grabs the bar.
const HOVER_BAND: f32 = 14.;
/// Seconds the bars stay after the last scroll.
const LINGER: f32 = 0.9;

/// Animated visibility and hover state of an area's two scrollbars.
#[derive(Default)]
pub(super) struct Scrollbars {
    /// Overall opacity, 0 to 1.
    visible: f32,
    /// Seconds since the last scroll.
    idle: f32,
    /// Width expansion per axis (x, y), 0 to 1.
    expand: [f32; 2],
    /// The mouse is in the bar's edge band, per axis.
    near: [bool; 2],
    /// The bar being dragged, if any.
    pub dragging: Option<usize>,
    last_offset: Vector,
}

/// Where the bars are drawn, for one frame.
#[derive(Clone, Copy)]
pub(super) struct BarLayout {
    pub viewport: Rect,
    pub max: Vector,
    pub offset: Vector,
}

impl BarLayout {
    fn shows(&self, axis: usize) -> bool {
        self.max[axis] > 0.5
    }

    /// Track and thumb for `axis` at `thickness`.
    pub fn geometry(&self, axis: usize, thickness: f32) -> Option<(Rect, Rect)> {
        if !self.shows(axis) {
            return None;
        }
        let v = self.viewport;
        // Leave the corner free when both bars show.
        let reserve = if self.shows(1 - axis) {
            WIDE + INSET
        } else {
            0.
        };
        let length = v.size()[axis] - 2. * INSET - reserve;
        if length <= MIN_THUMB {
            return None;
        }
        let extent = v.size()[axis] + self.max[axis];
        let size = (length * v.size()[axis] / extent).clamp(MIN_THUMB, length);
        let start = (length - size) * (self.offset[axis] / self.max[axis]).clamp(0., 1.);
        Some(if axis == 0 {
            let y = v.bottom() - INSET - thickness;
            (
                Rect::new(v.x + INSET, y, length, thickness),
                Rect::new(v.x + INSET + start, y, size, thickness),
            )
        } else {
            let x = v.right() - INSET - thickness;
            (
                Rect::new(x, v.y + INSET, thickness, length),
                Rect::new(x, v.y + INSET + start, thickness, size),
            )
        })
    }

    /// The edge band along `axis`'s bar that the mouse can grab.
    pub fn band(&self, axis: usize) -> Option<Rect> {
        let (track, _) = self.geometry(axis, WIDE)?;
        Some(if axis == 0 {
            Rect::new(
                track.x,
                self.viewport.bottom() - HOVER_BAND,
                track.w,
                HOVER_BAND,
            )
        } else {
            Rect::new(
                self.viewport.right() - HOVER_BAND,
                track.y,
                HOVER_BAND,
                track.h,
            )
        })
    }

    /// Track and thumb as drawn and hit-tested (always the wide size).
    pub fn grab_geometry(&self, axis: usize) -> Option<(Rect, Rect)> {
        self.geometry(axis, WIDE)
    }
}

impl Scrollbars {
    /// Which bar's band contains `position`, if any.
    pub fn band_at(&self, layout: &BarLayout, position: Vector, touch: bool) -> Option<usize> {
        // Touches only grab a bar that is already showing; the mouse can grab
        // it from the edge band at any time.
        if touch && self.visible < 0.5 {
            return None;
        }
        (0..2).find(|&axis| {
            layout
                .band(axis)
                .is_some_and(|band| band.contains(&position))
        })
    }

    /// Whether a showing bar's band covers `position`, so it takes the press
    /// instead of the content beneath.
    pub fn blocks(&self, layout: &BarLayout, position: Vector) -> bool {
        self.visible > 0.3 && self.band_at(layout, position, false).is_some()
    }

    /// Track the mouse over the area (`None` when it leaves).
    pub fn hover(&mut self, layout: &BarLayout, position: Option<Vector>) {
        for (axis, near) in self.near.iter_mut().enumerate() {
            *near = position.is_some_and(|position| {
                layout
                    .band(axis)
                    .is_some_and(|band| band.contains(&position))
            });
        }
    }

    /// Advance the fade and widening animations by `dt` seconds.
    pub fn advance(&mut self, layout: &BarLayout, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        if layout.offset != self.last_offset {
            self.last_offset = layout.offset;
            self.idle = 0.;
        } else {
            self.idle += dt;
        }
        let active = self.idle < LINGER || self.dragging.is_some() || self.near.contains(&true);
        let (target, rate) = if active { (1., 14.) } else { (0., 5.) };
        self.visible = approach(self.visible, target, rate, dt);
        for axis in 0..2 {
            let wide = self.near[axis] || self.dragging == Some(axis);
            self.expand[axis] = approach(self.expand[axis], if wide { 1. } else { 0. }, 12., dt);
        }
    }

    pub fn draw(&self, paint: &Paint, layout: &BarLayout) {
        if self.visible <= 0.01 {
            return;
        }
        paint.draw(|p| {
            for axis in 0..2 {
                let expand = self.expand[axis];
                let thickness = THIN + (WIDE - THIN) * expand;
                let Some((track, thumb)) = layout.geometry(axis, thickness) else {
                    continue;
                };
                let radius = thickness / 2.;
                if expand > 0.01 {
                    p.fill(
                        &PathData::rounded_rect(track, radius),
                        FillRule::NonZero,
                        Color::new(0.5, 0.5, 0.55, 0.16 * expand * self.visible),
                    );
                }
                let strength = if self.dragging == Some(axis) {
                    0.85
                } else {
                    0.55 + 0.2 * expand
                };
                p.fill(
                    &PathData::rounded_rect(thumb, radius),
                    FillRule::NonZero,
                    Color::new(0.62, 0.64, 0.7, strength * self.visible),
                );
            }
        });
    }
}

/// Move `value` toward `target` exponentially at `rate` per second.
fn approach(value: f32, target: f32, rate: f32, dt: f32) -> f32 {
    let next = value + (target - value) * (1. - (-rate * dt).exp());
    if (next - target).abs() < 0.005 {
        target
    } else {
        next
    }
}
