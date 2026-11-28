//! A loading spinner, like Chakra's `Spinner`.

use std::f32::consts::PI;

use mue_core::prelude::*;
use mue_macroquad::{
    layout::use_layout,
    math::Vector,
    node::{BuildNode, Node, NodeContext},
    paint::{
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{Size, Style, Styleable},
};

use crate::system::{self, ControlSize};

/// Builder returned by [`spinner`].
pub struct SpinnerBuilder {
    size: ControlSize,
    color: Option<Prop<Brush>>,
    track: Option<Prop<Brush>>,
    thickness: f32,
    speed: f32,
    style: Style,
}

/// A spinning arc that shows work in progress.
///
/// The spinner is 0.75, 1, 1.5, or 2 rem wide for the four
/// [`ControlSize`]s and draws in the inherited text color unless
/// [`color`](SpinnerBuilder::color) is set.
///
/// ```
/// use mue_components::{prelude::*, system::{ControlSize, Palette}};
///
/// let _busy = spinner().size(ControlSize::Lg).color(Palette::Teal.shade(400));
/// ```
pub fn spinner() -> SpinnerBuilder {
    SpinnerBuilder {
        size: ControlSize::Md,
        color: None,
        track: None,
        thickness: 0.12,
        speed: 1.4,
        style: Style::new(),
    }
}

impl SpinnerBuilder {
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Arc color; the inherited text color by default.
    pub fn color(mut self, color: impl Into<Prop<Brush>>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Color of the full circle behind the arc (Chakra's `emptyColor`).
    pub fn track(mut self, color: impl Into<Prop<Brush>>) -> Self {
        self.track = Some(color.into());
        self
    }

    /// Stroke width as a fraction of the diameter (0.12 by default).
    pub fn thickness(mut self, fraction: f32) -> Self {
        self.thickness = fraction.clamp(0.02, 0.5);
        self
    }

    /// Turns per second.
    pub fn speed(mut self, turns_per_second: f32) -> Self {
        self.speed = turns_per_second;
        self
    }
}

impl Styleable for SpinnerBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for SpinnerBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            size,
            color,
            track,
            thickness,
            speed,
            style,
        } = self;
        let diameter = Size::Rem([0.75, 1., 1.5, 2.][size as usize]);
        ctx.push_style(style);
        ctx.finish(
            Style::new().width(diameter).height(diameter).flex_shrink(0.),
            move |style| {
                let color = color
                    .or_else(mue_macroquad::inherit::inherited_foreground)
                    .unwrap_or_else(system::fg);
                let rect = use_layout(style).rect;
                let paint = use_paint(style);
                style.on_render.append(move |_| {
                    let rect = rect.get_untracked();
                    let d = rect.w.min(rect.h);
                    if d <= 0. {
                        return;
                    }
                    let width = (d * thickness).max(1.);
                    let radius = (d - width) / 2.;
                    let center = rect.center();
                    let time = mue_macroquad::app::runtime::get_time();
                    // The arc spins and breathes between a quarter and three
                    // quarters of a turn.
                    let start = (time * speed * 2. * PI) % (2. * PI);
                    let sweep = PI * (1. + 0.5 * (time * 2.1).sin());
                    let steps = 32;
                    let points = (0..=steps).map(|i| {
                        let angle = start + sweep * i as f32 / steps as f32;
                        center + Vector::new(angle.cos(), angle.sin()) * radius
                    });
                    let arc = PathData::polyline(points);
                    let stroke = StrokeStyle::round(width);
                    let brush = color.get_clone();
                    let track = track.as_ref().map(|track| track.get_clone());
                    paint.draw(|p| {
                        if let Some(track) = track {
                            p.stroke(
                                &PathData::circle(center, radius),
                                &StrokeStyle::new(width),
                                track.shader(rect),
                            );
                        }
                        p.stroke(&arc, &stroke, brush.shader(rect));
                    });
                });
            },
        )
    }
}
