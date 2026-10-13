use mue_core::prop::PropValue;

/// An sRGB color with channels in the 0..=1 range.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl PropValue for Color {}

impl Color {
    pub const TRANSPARENT: Self = Self::new(0., 0., 0., 0.);
    pub const WHITE: Self = Self::new(1., 1., 1., 1.);
    pub const BLACK: Self = Self::new(0., 0., 0., 1.);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.,
            g as f32 / 255.,
            b as f32 / 255.,
            a as f32 / 255.,
        )
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }
}

impl From<macroquad::color::Color> for Color {
    fn from(value: macroquad::color::Color) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

impl From<Color> for macroquad::color::Color {
    fn from(value: Color) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}
