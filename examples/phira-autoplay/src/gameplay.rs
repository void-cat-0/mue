//! Stateless gameplay art. All motion, including hit effects, is sampled from chart time.
use macroquad::prelude::{
    Color, DrawTextureParams, Vec2, draw_circle, draw_circle_lines, draw_line, draw_texture_ex,
    draw_triangle, vec2,
};
use mue_macroquad::{math::Rect, paint::with_clip};

use super::{
    chart::{Chart, Kind, Note, Pose},
    skin::{Skin, Sprite},
};

const INK: Color = Color::new(0.025, 0.06, 0.12, 1.);
const WHITE: Color = Color::new(0.97, 0.99, 1., 1.);
const GOLD: Color = Color::new(1., 0.84, 0.43, 1.);
const TAP: Color = Color::new(0.22, 0.8, 1., 1.);
const DRAG: Color = Color::new(1., 0.73, 0.24, 1.);
const HOLD: Color = Color::new(0.21, 0.92, 0.82, 1.);
const FLICK: Color = Color::new(1., 0.36, 0.64, 1.);
const HIT_LIFETIME: f64 = 0.40;
const HOLD_LIFETIME: f64 = 0.24;
const HOLD_INTERVAL: f64 = 0.15;

fn tint(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// Coordinates aligned with a judge line. Positive distance is on the note's approach side.
#[derive(Clone, Copy)]
struct Frame {
    origin: Vec2,
    right: Vec2,
    away: Vec2,
    angle: f32,
    below: bool,
}

impl Frame {
    fn line(field: Rect, x: f32, y: f32, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            origin: vec2(field.x + (0.5 + x) * field.w, field.y + (0.5 + y) * field.h),
            right: vec2(cos, sin),
            away: vec2(sin, -cos),
            angle,
            below: false,
        }
    }

    fn note(self, note: &Note, field: Rect) -> Self {
        let mut frame = self;
        if !note.above {
            frame.away = -frame.away;
            frame.below = true;
        }
        frame.origin = frame.at(note.x * field.w, note.y_offset as f32 * field.h);
        frame
    }

    fn at(self, x: f32, distance: f32) -> Vec2 {
        self.origin + self.right * x + self.away * distance
    }

    fn sprite(self, sprite: Sprite<'_>, distance: f32, size: Vec2, alpha: f32, reversed: bool) {
        let corner = self.at(0., distance) - size * 0.5;
        draw_texture_ex(
            sprite.texture,
            corner.x,
            corner.y,
            Color::new(1., 1., 1., alpha),
            DrawTextureParams {
                dest_size: Some(size),
                source: Some(sprite.source),
                rotation: self.angle,
                flip_y: self.below ^ reversed,
                ..Default::default()
            },
        );
    }

    fn polygon(self, points: &[(f32, f32)], color: Color) {
        let first = self.at(points[0].0, points[0].1);
        for pair in points[1..].windows(2) {
            draw_triangle(
                first,
                self.at(pair[0].0, pair[0].1),
                self.at(pair[1].0, pair[1].1),
                color,
            );
        }
    }

    fn rect(self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], color);
    }

    fn bar(self, y: f32, w: f32, h: f32, bevel: f32, color: Color) {
        let b = bevel.min(w).min(h);
        self.polygon(
            &[
                (-w + b, y - h),
                (w - b, y - h),
                (w, y - h + b),
                (w, y + h - b),
                (w - b, y + h),
                (-w + b, y + h),
                (-w, y + h - b),
                (-w, y - h + b),
            ],
            color,
        );
    }
}

fn stroke(from: Vec2, to: Vec2, width: f32, color: Color) {
    draw_line(from.x, from.y, to.x, to.y, width, color);
}

// Cheap rejection only; Mue's clip handles the exact field boundary.
fn in_view(field: Rect, from: Vec2, to: Vec2, margin: f32) -> bool {
    from.x.min(to.x) - margin <= field.right()
        && from.x.max(to.x) + margin >= field.x
        && from.y.min(to.y) - margin <= field.bottom()
        && from.y.max(to.y) + margin >= field.y
}

/// Draw gameplay into the current Mue paint pass (window or Scene picture).
/// `time` is offset-adjusted chart time in seconds, not application frame time.
pub fn draw(
    chart: &Chart,
    skin: Option<&Skin>,
    time: f64,
    field: Rect,
    line_length: f32,
    negative_holds: bool,
    opacity: f32,
) {
    if field.w <= 0. || field.h <= 0. || opacity <= 0. {
        return;
    }
    let opacity = opacity.min(1.);
    let lines: Vec<_> = chart
        .lines
        .iter()
        .map(|line| {
            let pose = line.sample(time);
            (Frame::line(field, pose.x, pose.y, pose.angle), pose)
        })
        .collect();

    with_clip(field, || {
        for &(frame, ref pose) in &lines {
            if pose.opacity > 0. && line_length > 0. {
                // Chart metadata uses a field two units wide, with length on each side.
                let half = line_length * field.w * 0.5;
                let thickness = (field.w * 0.0035).max(1.);
                let alpha = pose.opacity.min(1.) * opacity;
                let a = frame.at(-half, 0.);
                let b = frame.at(half, 0.);
                stroke(a, b, thickness * 2.4, tint(GOLD, 0.17 * alpha));
                stroke(a, b, thickness, tint(GOLD, alpha));
                stroke(a, b, thickness * 0.38, tint(WHITE, 0.82 * alpha));
            }
        }
        // Long ribbons first, then point notes so neither gets buried under a crossing hold.
        // Zero opacity hides just the line stroke; negative opacity also hides its notes.
        for holds in [true, false] {
            for (line, &(frame, ref pose)) in chart.lines.iter().zip(&lines) {
                if pose.opacity < 0. {
                    continue;
                }
                for note in &line.notes {
                    if matches!(note.kind, Kind::Hold) == holds {
                        draw_note(
                            note,
                            skin,
                            frame,
                            pose,
                            line.cover,
                            time,
                            field,
                            negative_holds,
                            opacity,
                        );
                    }
                }
            }
        }
        draw_effects(chart, time, field, opacity);
    });
}

fn hold_span(head: f64, tail: f64, active: bool, negative_holds: bool) -> (f64, f64) {
    let head = if active { 0. } else { head };
    (head, if negative_holds { tail } else { tail.max(head) })
}

#[allow(clippy::too_many_arguments)]
fn draw_note(
    note: &Note,
    skin: Option<&Skin>,
    frame: Frame,
    pose: &Pose,
    cover: bool,
    time: f64,
    field: Rect,
    negative_holds: bool,
    opacity: f32,
) {
    if note.alpha <= 0.
        || note.size <= 0.
        || time >= note.end
        || note.time - time > note.visible_time
    {
        return;
    }

    let active = time >= note.time;
    let head = (note.height - pose.height) * note.speed;
    // Cover hides approaching notes whose scroll position has crossed their own side of the line.
    if cover && !active && head < -0.0005 {
        return;
    }
    let frame = frame.note(note, field);
    let alpha = note.alpha.min(1.) * opacity;
    let w = field.w * 0.0525 * note.size;
    let h = field.h * 0.0105 * note.size;
    if matches!(note.kind, Kind::Hold) {
        let tail = (note.tail_height - pose.height) * note.speed;
        let (head, tail) = hold_span(head, tail, active, negative_holds);
        let head = head as f32 * field.h;
        let tail = tail as f32 * field.h;
        if in_view(field, frame.at(0., head), frame.at(0., tail), w + h * 2.) {
            if let Some(skin) = skin {
                draw_skin_hold(
                    skin,
                    note.highlight,
                    frame,
                    head,
                    tail,
                    w * 2.,
                    alpha,
                    active,
                );
            } else {
                draw_hold(
                    frame,
                    head,
                    tail,
                    w,
                    h,
                    alpha,
                    note.highlight,
                    active && !note.fake,
                );
            }
        }
    } else {
        let y = head as f32 * field.h;
        let center = frame.at(0., y);
        if in_view(field, center, center, w + h * 2.) {
            if let Some(sprite) = skin.and_then(|skin| skin.point(note.kind, note.highlight)) {
                let width = w * 2.;
                frame.sprite(sprite, y, vec2(width, sprite.height(width)), alpha, false);
            } else {
                draw_point(frame, y, w, h, &note.kind, alpha, note.highlight);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_skin_hold(
    skin: &Skin,
    highlighted: bool,
    frame: Frame,
    head: f32,
    tail: f32,
    width: f32,
    alpha: f32,
    active: bool,
) {
    let [head_sprite, body, tail_sprite] = skin.hold(highlighted);
    let length = (tail - head).abs();
    let reversed = tail < head;
    let direction = if reversed { -1. } else { 1. };
    if length > 0. {
        frame.sprite(
            body,
            (head + tail) * 0.5,
            vec2(width, length),
            alpha,
            reversed,
        );
    }
    let cap = |sprite: Sprite<'_>, distance, side: f32| {
        let height = sprite.height(width);
        if height > 0. {
            let center = if skin.compact {
                distance
            } else {
                distance + side * direction * height * 0.5
            };
            frame.sprite(sprite, center, vec2(width, height), alpha, reversed);
        }
    };
    if !active || skin.keep_head {
        cap(head_sprite, head, -1.);
    }
    cap(tail_sprite, tail, 1.);
}

fn draw_point(frame: Frame, y: f32, w: f32, h: f32, kind: &Kind, alpha: f32, highlight: bool) {
    let (w, h, color) = match kind {
        Kind::Tap => (w, h, TAP),
        Kind::Drag => (w * 0.88, h * 0.66, DRAG),
        Kind::Flick => (w, h * 1.38, FLICK),
        Kind::Hold => return,
    };
    frame.bar(y, w + h * 0.55, h * 1.43, h, tint(color, alpha * 0.19));
    if highlight {
        frame.bar(y, w + h * 0.65, h * 1.29, h, tint(GOLD, alpha));
    }
    frame.bar(y, w, h, h * 0.61, tint(INK, alpha));
    frame.bar(y, w - h * 0.25, h * 0.8, h * 0.43, tint(color, alpha));

    match kind {
        Kind::Tap => {
            frame.rect(
                -w * 0.79,
                y + h * 0.30,
                w * 1.58,
                h * 0.27,
                tint(WHITE, alpha * 0.92),
            );
        }
        Kind::Drag => {
            for x in [-w * 0.57, w * 0.57] {
                frame.polygon(
                    &[(x - h, y), (x, y + h * 0.55), (x + h, y), (x, y - h * 0.55)],
                    tint(WHITE, alpha * 0.92),
                );
            }
        }
        Kind::Flick => {
            for x in [-w * 0.36, w * 0.36] {
                for sign in [-1., 1.] {
                    frame.polygon(
                        &[
                            (x + sign * w * 0.31, y - h * 0.43),
                            (x, y + h * 0.54),
                            (x, y + h * 0.08),
                            (x + sign * w * 0.17, y - h * 0.43),
                        ],
                        tint(WHITE, alpha * 0.96),
                    );
                }
            }
        }
        Kind::Hold => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hold(
    frame: Frame,
    head: f32,
    tail: f32,
    w: f32,
    h: f32,
    alpha: f32,
    highlight: bool,
    active: bool,
) {
    let color = if active { GOLD } else { HOLD };
    let from = head.min(tail);
    let length = (tail - head).abs();
    if length > 0. {
        let w = w * 0.79;
        frame.rect(-w, from, w * 2., length, tint(INK, alpha * 0.68));
        frame.rect(-w * 0.87, from, w * 1.74, length, tint(color, alpha * 0.46));
        frame.rect(-w * 0.20, from, w * 0.40, length, tint(color, alpha * 0.47));
        for x in [-w * 0.83, w * 0.74] {
            frame.rect(
                x,
                from,
                w * 0.09,
                length,
                tint(if highlight { GOLD } else { WHITE }, alpha * 0.78),
            );
        }
        frame.bar(tail, w, h * 0.51, h * 0.42, tint(color, alpha));
    }
    frame.bar(
        head,
        w + h * 0.45,
        h * 1.25,
        h * 0.85,
        tint(color, alpha * 0.21),
    );
    frame.bar(
        head,
        w,
        h,
        h * 0.65,
        tint(if highlight { GOLD } else { INK }, alpha),
    );
    frame.bar(head, w - h * 0.35, h * 0.72, h * 0.56, tint(color, alpha));
    frame.rect(
        -w * 0.63,
        head - h * 0.13,
        w * 1.26,
        h * 0.29,
        tint(WHITE, alpha * 0.9),
    );
}

fn draw_effects(chart: &Chart, time: f64, field: Rect, opacity: f32) {
    // Reconstruct sustain pulses from their scheduled times, including a short decay after release.
    for line in &chart.lines {
        for (index, note) in line.notes.iter().enumerate() {
            if !matches!(note.kind, Kind::Hold)
                || note.fake
                || time < note.time + HOLD_INTERVAL
                || time >= note.end + HOLD_LIFETIME
            {
                continue;
            }
            let first = ((time - HOLD_LIFETIME - note.time) / HOLD_INTERVAL)
                .ceil()
                .max(1.) as usize;
            let last = ((time.min(note.end) - note.time) / HOLD_INTERVAL).floor() as usize;
            for tick in first..=last {
                let at = note.time + tick as f64 * HOLD_INTERVAL;
                if at < note.end {
                    let pose = line.sample(at);
                    if pose.opacity >= 0. {
                        let frame =
                            Frame::line(field, pose.x, pose.y, pose.angle).note(note, field);
                        burst(
                            frame,
                            field,
                            (time - at) / HOLD_LIFETIME,
                            opacity,
                            index + tick,
                            true,
                        );
                    }
                }
            }
        }
    }
    let hits = &chart.hits;
    let start = hits.partition_point(|hit| hit.time <= time - HIT_LIFETIME);
    let end = hits.partition_point(|hit| hit.time <= time);
    for hit in &hits[start..end] {
        let line = &chart.lines[hit.line];
        let pose = line.sample(hit.time);
        if pose.opacity >= 0. {
            let frame =
                Frame::line(field, pose.x, pose.y, pose.angle).note(&line.notes[hit.note], field);
            let seed = hit.line.wrapping_mul(23).wrapping_add(hit.note);
            burst(
                frame,
                field,
                (time - hit.time) / HIT_LIFETIME,
                opacity,
                seed,
                false,
            );
        }
    }
}

fn burst(frame: Frame, field: Rect, age: f64, opacity: f32, seed: usize, sustain: bool) {
    let unit = field.h * if sustain { 0.68 } else { 1. };
    if !in_view(field, frame.origin, frame.origin, unit * 0.12) {
        return;
    }
    let p = (age as f32).clamp(0., 1.);
    let travel = 1. - (1. - p).powi(2);
    let fade = (1. - p).powi(2) * opacity;
    let pos = frame.origin;
    let radius = unit * (0.012 + 0.056 * travel);
    draw_circle_lines(
        pos.x,
        pos.y,
        radius,
        unit * (0.0014 + 0.0020 * (1. - p)),
        tint(GOLD, fade * 0.92),
    );
    if !sustain {
        draw_circle_lines(
            pos.x,
            pos.y,
            radius * 0.74,
            unit * 0.0015,
            tint(WHITE, fade * 0.68),
        );
    }
    let flash = (1. - 5. * p).max(0.) * opacity;
    draw_circle(
        pos.x,
        pos.y,
        unit * (0.008 + 0.011 * travel),
        tint(WHITE, flash * 0.88),
    );

    let phase = (seed % 37) as f32 * 0.17;
    for spark in 0..if sustain { 4 } else { 8 } {
        let (sin, cos) = (phase + spark as f32 * 2.4).sin_cos();
        let reach = unit * (0.016 + travel * (0.048 + (spark % 3) as f32 * 0.015));
        let center = frame.at(cos * reach, sin * reach);
        let s = unit * (0.0027 + (spark % 2) as f32 * 0.002) * (1. - 0.55 * p);
        let d = frame.right * s;
        let n = frame.away * s;
        let color = tint(if spark % 3 == 0 { WHITE } else { GOLD }, fade * 0.92);
        draw_triangle(center - d, center + n, center + d, color);
        draw_triangle(center - d, center + d, center - n, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_normalized_positions_rotate_clockwise() {
        let field = Rect::new(20., 40., 200., 100.);
        let frame = Frame::line(field, -0.25, 0.25, std::f32::consts::FRAC_PI_2);
        assert_eq!(frame.origin, vec2(70., 115.));
        assert!(frame.at(15., 10.).abs_diff_eq(vec2(80., 130.), 0.0001));
        assert!(frame.at(15., -10.).abs_diff_eq(vec2(60., 130.), 0.0001));
    }

    #[test]
    fn hold_head_pins_and_the_tail_can_reverse() {
        assert_eq!(hold_span(0.6, 0.8, false, true), (0.6, 0.8));
        assert_eq!(hold_span(-0.2, 0.4, true, true), (0., 0.4));
        assert_eq!(hold_span(0.2, -0.4, true, true), (0., -0.4));
        assert_eq!(hold_span(0.2, -0.4, true, false), (0., 0.));
        assert_eq!(hold_span(0.2, -0.4, false, false), (0.2, 0.2));
    }

    #[test]
    fn a_hold_can_cross_the_field_with_both_caps_outside() {
        let field = Rect::new(20., 40., 200., 100.);
        assert!(in_view(field, vec2(50., 0.), vec2(50., 200.), 0.));
        assert!(!in_view(field, vec2(0., 0.), vec2(5., 200.), 8.));
    }
}
