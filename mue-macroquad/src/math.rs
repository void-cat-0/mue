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
