//! Vector paths: geometry constructors, SVG path data, and fitting to a box.
//!
//! [`PathData`] is an immutable, cheaply cloned path in its own coordinate
//! space. Draw it with the [`path`](crate::node::path()) node, or tessellate it
//! yourself with [`ShapesBuilder::fill`](crate::paint::ShapesBuilder::fill) and
//! [`ShapesBuilder::stroke`](crate::paint::ShapesBuilder::stroke).
//!
//! ```
//! use mue_macroquad::{math::Rect, paint::path::PathData};
//!
//! let check = PathData::parse("M4 12 l5 5 L20 6").unwrap();
//! assert_eq!(check.bounds(), Rect::new(4., 6., 16., 11.));
//! let pill = PathData::rounded_rect(Rect::new(0., 0., 80., 24.), 12.);
//! assert!(!pill.is_empty());
//! ```

use std::{fmt, rc::Rc};

pub use lyon;
use lyon::{
    geom::{Angle, ArcFlags, point, vector},
    path::{Path, builder::SvgPathBuilder},
};
use mue_core::prop::PropValue;

use crate::math::{CornerRadii, Matrix, Rect, Vector, vec2};

/// An immutable vector path with cached bounds.
///
/// Clones share the same geometry. Two values compare equal only when they
/// share it, so replacing a property with a newly parsed path always repaints.
#[derive(Clone)]
pub struct PathData {
    path: Rc<Path>,
    bounds: Rect,
}

impl PropValue for PathData {}

impl PartialEq for PathData {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.path, &other.path)
    }
}

impl fmt::Debug for PathData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathData")
            .field("bounds", &self.bounds)
            .finish_non_exhaustive()
    }
}

impl Default for PathData {
    fn default() -> Self {
        Self::new(Path::new())
    }
}

impl From<Path> for PathData {
    fn from(path: Path) -> Self {
        Self::new(path)
    }
}

impl PathData {
    /// Wrap a lyon path, computing its exact bounds.
    pub fn new(path: Path) -> Self {
        let bounds = if path.iter().next().is_none() {
            Rect::default()
        } else {
            let b = lyon::algorithms::aabb::bounding_box(path.iter());
            Rect::new(b.min.x, b.min.y, b.width(), b.height())
        };
        Self {
            path: Rc::new(path),
            bounds,
        }
    }

    /// Parse SVG path data (the `d` attribute), for example `"M0 0 L10 10 Z"`.
    ///
    /// All commands are supported: `M L H V C S Q T A Z` and their relative
    /// lowercase forms, with implicit repetition and compact number syntax.
    pub fn parse(data: &str) -> Result<Self, PathParseError> {
        parse_svg_path(data).map(Self::new)
    }

    /// An axis-aligned rectangle.
    pub fn rect(rect: Rect) -> Self {
        Self::rounded_rect(rect, CornerRadii::ZERO)
    }

    /// A rectangle with per-corner radii. Radii that overlap are scaled down.
    pub fn rounded_rect(rect: Rect, radii: impl Into<CornerRadii>) -> Self {
        let mut builder = Path::builder();
        add_rounded_rect(
            &mut builder,
            rect,
            radii.into(),
            lyon::path::Winding::Positive,
        );
        Self::new(builder.build())
    }

    /// A circle.
    pub fn circle(center: Vector, radius: f32) -> Self {
        Self::ellipse(center, vec2(radius, radius))
    }

    /// An axis-aligned ellipse.
    pub fn ellipse(center: Vector, radii: Vector) -> Self {
        let mut builder = Path::builder();
        builder.add_ellipse(
            point(center.x, center.y),
            vector(radii.x.max(0.), radii.y.max(0.)),
            Angle::zero(),
            lyon::path::Winding::Positive,
        );
        Self::new(builder.build())
    }

    /// One open segment, useful for strokes.
    pub fn line(from: Vector, to: Vector) -> Self {
        Self::polyline([from, to])
    }

    /// Connected open segments through `points`.
    pub fn polyline(points: impl IntoIterator<Item = Vector>) -> Self {
        Self::poly(points, false)
    }

    /// A closed polygon through `points`.
    pub fn polygon(points: impl IntoIterator<Item = Vector>) -> Self {
        Self::poly(points, true)
    }

    /// A regular polygon with `sides` corners (at least 3) inscribed in a
    /// circle of `radius`, the first corner at `rotation` radians (0 points
    /// right; -π/2 points up).
    pub fn regular_polygon(center: Vector, radius: f32, sides: u32, rotation: f32) -> Self {
        let sides = sides.max(3);
        Self::polygon((0..sides).map(|i| {
            let angle = rotation + std::f32::consts::TAU * i as f32 / sides as f32;
            center + vec2(angle.cos(), angle.sin()) * radius
        }))
    }

    /// A star with `points` tips (at least 2) on a circle of `outer` radius
    /// and inner corners on `inner`, the first tip at `rotation` radians.
    pub fn star(center: Vector, outer: f32, inner: f32, points: u32, rotation: f32) -> Self {
        let points = points.max(2);
        Self::polygon((0..points * 2).map(|i| {
            let angle = rotation + std::f32::consts::PI * i as f32 / points as f32;
            let radius = if i % 2 == 0 { outer } else { inner };
            center + vec2(angle.cos(), angle.sin()) * radius
        }))
    }

    /// An open elliptical arc from `start` through `sweep` radians
    /// (clockwise on screen for positive sweeps); stroke it.
    pub fn arc(center: Vector, radii: Vector, start: f32, sweep: f32) -> Self {
        let steps = arc_steps(radii, sweep);
        Self::polyline((0..=steps).map(|i| {
            let angle = start + sweep * i as f32 / steps as f32;
            center + vec2(angle.cos() * radii.x, angle.sin() * radii.y)
        }))
    }

    /// A pie slice from `start` through `sweep` radians; with `inner_ratio`
    /// above 0 the center is cut out (a ring segment, 0..1 of the radii).
    /// A full turn with `inner_ratio` 0 is an ellipse.
    pub fn sector(center: Vector, radii: Vector, inner_ratio: f32, start: f32, sweep: f32) -> Self {
        let inner_ratio = inner_ratio.clamp(0., 1.);
        let steps = arc_steps(radii, sweep);
        let at = |angle: f32, scale: f32| {
            center + vec2(angle.cos() * radii.x, angle.sin() * radii.y) * scale
        };
        let outer = (0..=steps).map(|i| at(start + sweep * i as f32 / steps as f32, 1.));
        if inner_ratio <= 0. {
            return Self::polygon(std::iter::once(center).chain(outer));
        }
        let inner = (0..=steps)
            .rev()
            .map(|i| at(start + sweep * i as f32 / steps as f32, inner_ratio));
        Self::polygon(outer.chain(inner))
    }

    /// The part of the path between `start` and `end`, as fractions (0..1)
    /// of its length: animate `end` from 0 to 1 to draw a stroke on.
    pub fn trimmed(&self, start: f32, end: f32) -> Self {
        let (start, end) = (start.clamp(0., 1.), end.clamp(0., 1.));
        if start <= 0. && end >= 1. {
            return self.clone();
        }
        if end <= start {
            return Self::default();
        }
        use lyon::algorithms::measure::{PathMeasurements, SampleType};
        let measurements = PathMeasurements::from_path(&self.path, 0.05);
        let path: &Path = &self.path;
        let mut sampler = measurements.create_sampler(path, SampleType::Normalized);
        let mut builder = Path::builder();
        sampler.split_range(start..end, &mut builder);
        Self::new(builder.build())
    }

    fn poly(points: impl IntoIterator<Item = Vector>, close: bool) -> Self {
        let mut builder = Path::builder();
        let mut points = points.into_iter();
        if let Some(first) = points.next() {
            builder.begin(point(first.x, first.y));
            for p in points {
                builder.line_to(point(p.x, p.y));
            }
            builder.end(close);
        }
        Self::new(builder.build())
    }

    /// Exact bounds of the path geometry, before stroking.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn is_empty(&self) -> bool {
        self.path.iter().next().is_none()
    }

    /// The underlying lyon path.
    pub fn lyon(&self) -> &Path {
        &self.path
    }

    /// Iterate path events for lyon tessellators or algorithms.
    pub fn iter(&self) -> lyon::path::Iter<'_> {
        self.path.iter()
    }

    /// Apply the affine part of `matrix`, returning a new path.
    pub fn transformed(&self, matrix: Matrix) -> Self {
        if matrix == Matrix::IDENTITY {
            return self.clone();
        }
        Self::new(
            self.path
                .as_ref()
                .clone()
                .transformed(&lyon_transform(matrix)),
        )
    }
}

pub(crate) fn lyon_transform(matrix: Matrix) -> lyon::geom::euclid::default::Transform2D<f32> {
    let c = matrix.to_cols_array();
    lyon::geom::euclid::default::Transform2D::new(c[0], c[1], c[3], c[4], c[6], c[7])
}

pub(crate) fn add_rounded_rect(
    builder: &mut lyon::path::path::Builder,
    rect: Rect,
    radii: CornerRadii,
    winding: lyon::path::Winding,
) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    let radii = radii.fit(rect);
    builder.add_rounded_rectangle(
        &lyon::geom::Box2D::from_origin_and_size(
            point(rect.x, rect.y),
            lyon::geom::size(rect.w, rect.h),
        ),
        &lyon::path::builder::BorderRadii {
            top_left: radii.top_left,
            top_right: radii.top_right,
            bottom_left: radii.bottom_left,
            bottom_right: radii.bottom_right,
        },
        winding,
    );
}

/// How the fill decides which regions are inside a self-intersecting path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}
impl PropValue for FillRule {}

impl From<FillRule> for lyon::tessellation::FillRule {
    fn from(rule: FillRule) -> Self {
        match rule {
            FillRule::NonZero => Self::NonZero,
            FillRule::EvenOdd => Self::EvenOdd,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke geometry, in the path's own units before fitting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
}
impl PropValue for StrokeStyle {}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.,
        }
    }
}

impl StrokeStyle {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }

    /// Round caps and joins, the usual choice for icons.
    pub fn round(width: f32) -> Self {
        Self {
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
            ..Self::default()
        }
    }

    pub(crate) fn options(&self, tolerance: f32) -> lyon::tessellation::StrokeOptions {
        use lyon::tessellation::{LineCap as Cap, LineJoin as Join, StrokeOptions};
        StrokeOptions::tolerance(tolerance)
            .with_line_width(self.width.max(0.))
            .with_line_cap(match self.cap {
                LineCap::Butt => Cap::Butt,
                LineCap::Round => Cap::Round,
                LineCap::Square => Cap::Square,
            })
            .with_line_join(match self.join {
                LineJoin::Miter => Join::Miter,
                LineJoin::Round => Join::Round,
                LineJoin::Bevel => Join::Bevel,
            })
            .with_miter_limit(self.miter_limit.max(1.))
    }
}

/// How a view box maps into the node's content box. Fitted content is centered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathFit {
    /// Scale uniformly so the whole view box is visible (SVG `xMidYMid meet`).
    #[default]
    Contain,
    /// Scale uniformly to cover the box (SVG `xMidYMid slice`).
    Cover,
    /// Stretch each axis independently (SVG `none`).
    Fill,
    /// Keep the original scale, centered.
    None,
}
impl PropValue for PathFit {}

impl PathFit {
    /// The matrix mapping `view_box` coordinates into `target`.
    pub fn matrix(self, view_box: Rect, target: Rect) -> Matrix {
        if view_box.w <= 0. || view_box.h <= 0. {
            return Matrix::from_translation(target.min() - view_box.min());
        }
        let sx = target.w / view_box.w;
        let sy = target.h / view_box.h;
        let (sx, sy) = match self {
            Self::Contain => (sx.min(sy), sx.min(sy)),
            Self::Cover => (sx.max(sy), sx.max(sy)),
            Self::Fill => (sx, sy),
            Self::None => (1., 1.),
        };
        let scaled = vec2(view_box.w * sx, view_box.h * sy);
        let offset = target.min() + (target.size() - scaled) / 2.;
        Matrix::from_translation(offset)
            * Matrix::from_scale(vec2(sx, sy))
            * Matrix::from_translation(-view_box.min())
    }
}

/// The SVG path data could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid SVG path data at byte {offset}: {message}")]
pub struct PathParseError {
    pub offset: usize,
    pub message: &'static str,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Lexer<'_> {
    fn error(&self, message: &'static str) -> PathParseError {
        PathParseError {
            offset: self.pos,
            message,
        }
    }

    fn skip_separators(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_whitespace() || *c == b',')
        {
            self.pos += 1;
        }
    }

    fn peek_command(&mut self) -> Option<u8> {
        self.skip_separators();
        self.bytes
            .get(self.pos)
            .copied()
            .filter(|c| c.is_ascii_alphabetic() && !matches!(c, b'e' | b'E'))
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        self.bytes
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Result<f32, PathParseError> {
        self.skip_separators();
        let start = self.pos;
        let b = self.bytes;
        if matches!(b.get(self.pos), Some(b'-' | b'+')) {
            self.pos += 1;
        }
        let mut digits = false;
        while b.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
            digits = true;
        }
        if b.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            while b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
                digits = true;
            }
        }
        if !digits {
            self.pos = start;
            return Err(self.error("expected a number"));
        }
        if matches!(b.get(self.pos), Some(b'e' | b'E')) {
            let mark = self.pos;
            self.pos += 1;
            if matches!(b.get(self.pos), Some(b'-' | b'+')) {
                self.pos += 1;
            }
            if b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                while b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
            } else {
                self.pos = mark;
            }
        }
        std::str::from_utf8(&b[start..self.pos])
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
            .filter(|value| value.is_finite())
            .ok_or(PathParseError {
                offset: start,
                message: "invalid number",
            })
    }

    /// Arc flags may be written without separators, as in `a1 1 0 00 5 5`.
    fn flag(&mut self) -> Result<bool, PathParseError> {
        self.skip_separators();
        match self.bytes.get(self.pos) {
            Some(b'0') => {
                self.pos += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.pos += 1;
                Ok(true)
            }
            _ => Err(self.error("expected an arc flag (0 or 1)")),
        }
    }

    fn pair(&mut self) -> Result<lyon::math::Point, PathParseError> {
        Ok(point(self.number()?, self.number()?))
    }
}

fn parse_svg_path(data: &str) -> Result<Path, PathParseError> {
    let mut lexer = Lexer {
        bytes: data.as_bytes(),
        pos: 0,
    };
    let mut builder = Path::svg_builder();
    let mut command: Option<u8> = None;
    let mut started = false;
    loop {
        if let Some(next) = lexer.peek_command() {
            lexer.pos += 1;
            command = Some(next);
        } else if lexer.pos >= lexer.bytes.len() {
            break;
        } else if !lexer.at_number() {
            return Err(lexer.error("unexpected character"));
        }
        let Some(current) = command else {
            return Err(lexer.error("path data must start with a command"));
        };
        if !started && !matches!(current, b'M' | b'm') {
            return Err(lexer.error("path data must start with M or m"));
        }
        let relative = current.is_ascii_lowercase();
        let v = |p: lyon::math::Point| p.to_vector();
        match current.to_ascii_uppercase() {
            b'M' => {
                let to = lexer.pair()?;
                if relative && started {
                    builder.relative_move_to(v(to));
                } else {
                    builder.move_to(to);
                }
                started = true;
                // Extra coordinate pairs after a move are implicit line-tos.
                command = Some(if relative { b'l' } else { b'L' });
            }
            b'Z' => {
                builder.close();
                // Z takes no arguments; a following number is an error.
                command = None;
                if lexer.peek_command().is_none() && lexer.at_number() {
                    return Err(lexer.error("expected a command after Z"));
                }
                continue;
            }
            b'L' => {
                let to = lexer.pair()?;
                if relative {
                    builder.relative_line_to(v(to));
                } else {
                    builder.line_to(to);
                }
            }
            b'H' => {
                let x = lexer.number()?;
                if relative {
                    builder.relative_horizontal_line_to(x);
                } else {
                    builder.horizontal_line_to(x);
                }
            }
            b'V' => {
                let y = lexer.number()?;
                if relative {
                    builder.relative_vertical_line_to(y);
                } else {
                    builder.vertical_line_to(y);
                }
            }
            b'C' => {
                let (c1, c2, to) = (lexer.pair()?, lexer.pair()?, lexer.pair()?);
                if relative {
                    builder.relative_cubic_bezier_to(v(c1), v(c2), v(to));
                } else {
                    builder.cubic_bezier_to(c1, c2, to);
                }
            }
            b'S' => {
                let (c2, to) = (lexer.pair()?, lexer.pair()?);
                if relative {
                    builder.smooth_relative_cubic_bezier_to(v(c2), v(to));
                } else {
                    builder.smooth_cubic_bezier_to(c2, to);
                }
            }
            b'Q' => {
                let (c, to) = (lexer.pair()?, lexer.pair()?);
                if relative {
                    builder.relative_quadratic_bezier_to(v(c), v(to));
                } else {
                    builder.quadratic_bezier_to(c, to);
                }
            }
            b'T' => {
                let to = lexer.pair()?;
                if relative {
                    builder.smooth_relative_quadratic_bezier_to(v(to));
                } else {
                    builder.smooth_quadratic_bezier_to(to);
                }
            }
            b'A' => {
                let radii = vector(lexer.number()?.abs(), lexer.number()?.abs());
                let rotation = Angle::degrees(lexer.number()?);
                let flags = ArcFlags {
                    large_arc: lexer.flag()?,
                    sweep: lexer.flag()?,
                };
                let to = lexer.pair()?;
                if relative {
                    builder.relative_arc_to(radii, rotation, flags, v(to));
                } else {
                    builder.arc_to(radii, rotation, flags, to);
                }
            }
            _ => return Err(lexer.error("unsupported path command")),
        }
    }
    Ok(builder.build())
}

/// The shape of a rounded corner, as in CSS `corner-shape`.
///
/// Every corner keeps its `border-radius` box; the shape decides the curve
/// inside it. The curve is the superellipse `|x|^n + |y|^n = 1` with
/// `n = 2^K`, as in CSS `superellipse(K)`: `Round` is K = 1, `Squircle`
/// K = 2, `Bevel` K = 0 (a straight cut), `Scoop` K = -1 (concave),
/// `Square` K = infinity and `Notch` K = -infinity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CornerShape {
    #[default]
    Round,
    Squircle,
    Bevel,
    Scoop,
    Notch,
    Square,
    /// `superellipse(K)`; positive K bulges toward the corner, negative K is
    /// concave.
    Superellipse(f32),
}
impl PropValue for CornerShape {}

impl CornerShape {
    /// The superellipse exponent `2^K`; infinite for `Square`, zero for `Notch`.
    pub fn exponent(self) -> f32 {
        match self {
            Self::Round => 2.,
            Self::Squircle => 4.,
            Self::Bevel => 1.,
            Self::Scoop => 0.5,
            Self::Notch => 0.,
            Self::Square => f32::INFINITY,
            Self::Superellipse(k) => 2f32.powf(k),
        }
    }
}

/// Append the closed outline of `rect` with `radii` and `shape` to `out`,
/// clockwise on screen (y down) from the top-left corner. Curves are
/// subdivided finely enough for the given device-pixel `scale`.
pub(crate) fn corner_outline(
    rect: Rect,
    radii: CornerRadii,
    shape: CornerShape,
    scale: f32,
    out: &mut Vec<Vector>,
) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    let radii = radii.fit(rect);
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
    // Corner: radius, center, axis toward the first point, axis toward the second.
    let corners = [
        (radii.top_left, vec2(x0, y0), vec2(-1., 0.), vec2(0., -1.)),
        (radii.top_right, vec2(x1, y0), vec2(0., -1.), vec2(1., 0.)),
        (radii.bottom_right, vec2(x1, y1), vec2(1., 0.), vec2(0., 1.)),
        (radii.bottom_left, vec2(x0, y1), vec2(0., 1.), vec2(-1., 0.)),
    ];
    let n = shape.exponent();
    let start = out.len();
    let push = |point: Vector, out: &mut Vec<Vector>| {
        if out.len() == start
            || out
                .last()
                .is_none_or(|last| last.distance_squared(point) > 1e-8)
        {
            out.push(point);
        }
    };
    for (radius, corner, a, b) in corners {
        if radius <= 0. {
            push(corner, out);
            continue;
        }
        let center = corner - (a + b) * radius;
        if n <= 0. {
            push(center + a * radius, out);
            push(center, out);
            push(center + b * radius, out);
            continue;
        }
        if !n.is_finite() || n > 64. {
            push(center + a * radius, out);
            push(corner, out);
            push(center + b * radius, out);
            continue;
        }
        let segments = if (n - 1.).abs() < 1e-3 {
            1
        } else {
            corner_segments(radius, scale)
        };
        let power = 2. / n;
        for i in 0..=segments {
            let t = i as f32 / segments as f32 * std::f32::consts::FRAC_PI_2;
            let (s, c) = t.sin_cos();
            let point = center
                + a * (radius * c.max(0.).powf(power))
                + b * (radius * s.max(0.).powf(power));
            push(point, out);
        }
    }
    // Drop a closing duplicate of the first point.
    if out.len() > start + 1 && out[start].distance_squared(*out.last().unwrap()) <= 1e-8 {
        out.pop();
    }
}

/// Line segments for one curved corner of `radius` at a device-pixel `scale`.
pub(crate) fn corner_segments(radius: f32, scale: f32) -> usize {
    ((radius * scale.max(0.25)).sqrt() * 2.5)
        .ceil()
        .clamp(4., 64.) as usize
}

/// Twice the signed area of a closed polygon (positive when clockwise on
/// screen with y down).
pub(crate) fn signed_area(points: &[Vector]) -> f32 {
    let mut area = 0.;
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        area += p.x * q.y - q.x * p.y;
    }
    area
}

/// Even-odd point-in-polygon test.
pub(crate) fn polygon_contains(points: &[Vector], point: Vector) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Segments for a smooth arc: about one per 4 logical pixels of length.
fn arc_steps(radii: Vector, sweep: f32) -> u32 {
    let length = sweep.abs() * radii.x.max(radii.y).max(0.);
    ((length / 4.).ceil() as u32).clamp(4, 256)
}
