//! Vector paths: geometry constructors, SVG path data, and fitting to a box.
//!
//! [`PathData`] is an immutable, cheaply cloned path in its own coordinate
//! space. Draw it with the [`path`](crate::node::path()) node, or tessellate it
//! yourself with [`ShapesBuilder::fill`](crate::paint::ShapesBuilder::fill) and
//! [`ShapesBuilder::stroke`](crate::paint::ShapesBuilder::stroke).
//!
//! ```
//! use mue_macroquad::{math::Rect, path::PathData};
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
