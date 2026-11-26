//! The pull-to-refresh indicator.

use mue_core::prelude::*;
use mue_macroquad::{
    math::Vector,
    paint::color::Color,
    paint::path::{FillRule, PathData},
};

use super::state::{Refresh, ScrollState};

/// Draw the pull-to-refresh indicator: an arc that grows with the pull and
/// spins while refreshing.
pub(super) fn draw_refresh_indicator(
    paint: &mue_macroquad::paint::Paint,
    state: &ScrollState,
    refresh: &Refresh,
) {
    use std::f32::consts::PI;
    let pull = state.pull.get_untracked();
    let refreshing = refresh.refreshing.get_untracked();
    if pull < 0.5 && !refreshing {
        return;
    }
    let radius = 9.;
    let center = Vector::new(
        state.viewport.center().x,
        state.viewport.y + (pull / 2.).max(radius + 6.).min(refresh.hold / 2. + 8.),
    );
    let (start, sweep) = if refreshing {
        let spin = (mue_macroquad::app::runtime::get_time() * 5.) % (2. * PI);
        (spin, 1.5 * PI)
    } else {
        let progress = (pull / refresh.threshold).clamp(0., 1.);
        (-PI / 2., progress * 1.75 * PI)
    };
    let end = start + sweep.max(0.05);
    let point = |angle: f32| center + Vector::new(angle.cos(), angle.sin()) * radius;
    let (a, b) = (point(start), point(end));
    let arc = format!(
        "M{} {} A{radius} {radius} 0 {} 1 {} {}",
        a.x,
        a.y,
        u8::from(sweep > PI),
        b.x,
        b.y
    );
    let Ok(arc) = PathData::parse(&arc) else {
        return;
    };
    let alpha = if refreshing {
        1.
    } else {
        (pull / refresh.threshold).clamp(0.3, 1.)
    };
    paint.draw(|p| {
        p.fill(
            &PathData::circle(center, radius + 7.),
            FillRule::NonZero,
            Color::new(0.16, 0.19, 0.27, alpha),
        );
        p.stroke(
            &arc,
            &mue_macroquad::paint::path::StrokeStyle::round(2.5),
            Color::new(0.55, 0.72, 1., alpha),
        );
    });
}
