pub use glam::vec2;

pub type Vector = glam::Vec2;
pub type Matrix = glam::Mat3;

/// Project a canvas point through a homography, including perspective division.
pub fn project_point(matrix: Matrix, point: Vector) -> Vector {
    let point = matrix * point.extend(1.);
    point.truncate() / point.z
}

/// Embed a canvas homography without discarding its homogeneous coordinate.
pub(crate) fn model_matrix(matrix: Matrix) -> macroquad::math::Mat4 {
    let c = matrix.to_cols_array();
    macroquad::math::Mat4::from_cols_array(&[
        c[0], c[1], 0., c[2], c[3], c[4], 0., c[5], 0., 0., 1., 0., c[6], c[7], 0., c[8],
    ])
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn min(&self) -> Vector {
        vec2(self.x, self.y)
    }

    pub fn max(&self) -> Vector {
        vec2(self.x + self.w, self.y + self.h)
    }

    pub fn size(&self) -> Vector {
        vec2(self.w, self.h)
    }

    pub fn center(&self) -> Vector {
        vec2(self.x + self.w / 2., self.y + self.h / 2.)
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        if x1 <= x2 && y1 <= y2 {
            Some(Rect::new(x1, y1, x2 - x1, y2 - y1))
        } else {
            None
        }
    }

    /// Smallest rectangle containing both rectangles.
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, point: &Vector) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }
}

impl From<macroquad::math::Rect> for Rect {
    fn from(rect: macroquad::math::Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }
    }
}

impl From<Rect> for macroquad::math::Rect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }
    }
}

impl Rect {
    /// Grow every side by `amount`; negative values shrink the rectangle.
    pub fn outset(&self, amount: f32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            (self.w + 2. * amount).max(0.),
            (self.h + 2. * amount).max(0.),
        )
    }

    /// Shrink each side by the matching edge, clamping the size at zero.
    pub fn deflate(&self, edges: Edges) -> Rect {
        Rect::new(
            self.x + edges.left,
            self.y + edges.top,
            (self.w - edges.left - edges.right).max(0.),
            (self.h - edges.top - edges.bottom).max(0.),
        )
    }
}

/// Resolved per-side lengths in logical pixels, such as border or padding widths.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Self = Self::all(0.);

    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.top <= 0. && self.right <= 0. && self.bottom <= 0. && self.left <= 0.
    }
}

impl From<taffy::Rect<f32>> for Edges {
    fn from(rect: taffy::Rect<f32>) -> Self {
        Self {
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            left: rect.left,
        }
    }
}

/// Resolved corner radii in logical pixels, clockwise from the top-left corner.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub const ZERO: Self = Self::all(0.);

    pub const fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.top_left <= 0.
            && self.top_right <= 0.
            && self.bottom_right <= 0.
            && self.bottom_left <= 0.
    }

    /// The common radius when all four corners match.
    pub fn uniform(&self) -> Option<f32> {
        (self.top_left == self.top_right
            && self.top_left == self.bottom_right
            && self.top_left == self.bottom_left)
            .then_some(self.top_left)
    }

    pub fn max(&self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }

    /// Clamp negative radii to zero, then scale all radii down uniformly when
    /// adjacent corners would overlap along a side (CSS corner-overlap rule).
    pub fn fit(&self, rect: Rect) -> Self {
        let r = Self {
            top_left: self.top_left.max(0.),
            top_right: self.top_right.max(0.),
            bottom_right: self.bottom_right.max(0.),
            bottom_left: self.bottom_left.max(0.),
        };
        let ratio = |side: f32, a: f32, b: f32| {
            if a + b > side && a + b > 0. {
                (side.max(0.)) / (a + b)
            } else {
                1.
            }
        };
        let scale = ratio(rect.w, r.top_left, r.top_right)
            .min(ratio(rect.w, r.bottom_left, r.bottom_right))
            .min(ratio(rect.h, r.top_left, r.bottom_left))
            .min(ratio(rect.h, r.top_right, r.bottom_right));
        if scale < 1. { r.scale(scale) } else { r }
    }

    pub fn scale(&self, factor: f32) -> Self {
        Self {
            top_left: self.top_left * factor,
            top_right: self.top_right * factor,
            bottom_right: self.bottom_right * factor,
            bottom_left: self.bottom_left * factor,
        }
    }

    /// Radii for a rectangle grown by `amount` on every side. Square corners stay square.
    pub fn outset(&self, amount: f32) -> Self {
        let grow = |radius: f32| {
            if radius > 0. {
                (radius + amount).max(0.)
            } else {
                0.
            }
        };
        Self {
            top_left: grow(self.top_left),
            top_right: grow(self.top_right),
            bottom_right: grow(self.bottom_right),
            bottom_left: grow(self.bottom_left),
        }
    }

    /// Inner radii of a border with the given widths (each corner shrinks by
    /// its wider adjacent edge).
    pub fn inset(&self, edges: Edges) -> Self {
        Self {
            top_left: (self.top_left - edges.top.max(edges.left)).max(0.),
            top_right: (self.top_right - edges.top.max(edges.right)).max(0.),
            bottom_right: (self.bottom_right - edges.bottom.max(edges.right)).max(0.),
            bottom_left: (self.bottom_left - edges.bottom.max(edges.left)).max(0.),
        }
    }
}

impl From<f32> for CornerRadii {
    fn from(radius: f32) -> Self {
        Self::all(radius)
    }
}

impl mue_core::prop::PropValue for Rect {}
impl mue_core::prop::PropValue for Edges {}
impl mue_core::prop::PropValue for CornerRadii {}
