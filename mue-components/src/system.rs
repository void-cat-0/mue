//! The design system the controls share: a Chakra-style color palette,
//! semantic colors that follow the color mode, and control sizes in `rem`.
//!
//! Controls take a [`Palette`] (Chakra's `colorScheme`) and a
//! [`ControlSize`]. Sizes are in `rem`, so controls follow the root font size:
//! set a larger `font-size` on the App root (for example on a phone or for a
//! zoom setting) and every control grows with the text.
//!
//! Semantic colors ([`fg`], [`bg`], [`border`], ...) are reactive brushes
//! that switch between light and dark values with
//! [`use_color_mode`](crate::use_color_mode).
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{self, Palette, ControlSize};
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let _panel = flexbox().style(style! {
//!     padding: 1rem;
//!     border-radius: {system::radius::LG};
//!     background: {system::bg_subtle()};
//!     color: {system::fg()};
//! });
//! let _accent = Palette::Teal.shade(500);
//! let _height = ControlSize::Md.height();
//! # });
//! ```

use mue_core::prelude::*;
use mue_macroquad::{
    paint::{color::Color, shader::Brush},
    style::{FontSize, Size},
};

use crate::{ColorScheme, use_color_mode};

/// A color family with ten shades, 50 (lightest) to 900 (darkest), with
/// Chakra UI's values. Controls use it as their accent (`colorScheme`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Palette {
    Gray,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    #[default]
    Blue,
    Cyan,
    Purple,
    Pink,
}

const SHADES: [u16; 10] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900];

impl Palette {
    pub const ALL: [Palette; 10] = [
        Palette::Gray,
        Palette::Red,
        Palette::Orange,
        Palette::Yellow,
        Palette::Green,
        Palette::Teal,
        Palette::Blue,
        Palette::Cyan,
        Palette::Purple,
        Palette::Pink,
    ];

    fn table(self) -> [u32; 10] {
        match self {
            Palette::Gray => [
                0xF7FAFC, 0xEDF2F7, 0xE2E8F0, 0xCBD5E0, 0xA0AEC0, 0x718096, 0x4A5568, 0x2D3748,
                0x1A202C, 0x171923,
            ],
            Palette::Red => [
                0xFFF5F5, 0xFED7D7, 0xFEB2B2, 0xFC8181, 0xF56565, 0xE53E3E, 0xC53030, 0x9B2C2C,
                0x822727, 0x63171B,
            ],
            Palette::Orange => [
                0xFFFAF0, 0xFEEBC8, 0xFBD38D, 0xF6AD55, 0xED8936, 0xDD6B20, 0xC05621, 0x9C4221,
                0x7B341E, 0x652B19,
            ],
            Palette::Yellow => [
                0xFFFFF0, 0xFEFCBF, 0xFAF089, 0xF6E05E, 0xECC94B, 0xD69E2E, 0xB7791F, 0x975A16,
                0x744210, 0x5F370E,
            ],
            Palette::Green => [
                0xF0FFF4, 0xC6F6D5, 0x9AE6B4, 0x68D391, 0x48BB78, 0x38A169, 0x2F855A, 0x276749,
                0x22543D, 0x1C4532,
            ],
            Palette::Teal => [
                0xE6FFFA, 0xB2F5EA, 0x81E6D9, 0x4FD1C5, 0x38B2AC, 0x319795, 0x2C7A7B, 0x285E61,
                0x234E52, 0x1D4044,
            ],
            Palette::Blue => [
                0xEBF8FF, 0xBEE3F8, 0x90CDF4, 0x63B3ED, 0x4299E1, 0x3182CE, 0x2B6CB0, 0x2C5282,
                0x2A4365, 0x1A365D,
            ],
            Palette::Cyan => [
                0xEDFDFD, 0xC4F1F9, 0x9DECF9, 0x76E4F7, 0x0BC5EA, 0x00B5D8, 0x00A3C4, 0x0987A0,
                0x086F83, 0x065666,
            ],
            Palette::Purple => [
                0xFAF5FF, 0xE9D8FD, 0xD6BCFA, 0xB794F4, 0x9F7AEA, 0x805AD5, 0x6B46C1, 0x553C9A,
                0x44337A, 0x322659,
            ],
            Palette::Pink => [
                0xFFF5F7, 0xFED7E2, 0xFBB6CE, 0xF687B3, 0xED64A6, 0xD53F8C, 0xB83280, 0x97266D,
                0x702459, 0x521B41,
            ],
        }
    }

    /// One shade: 50, 100, 200, ... 900. Other values pick the nearest shade.
    pub fn shade(self, shade: u16) -> Color {
        let index = SHADES
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| (**s as i32 - shade as i32).abs())
            .map_or(5, |(index, _)| index);
        hex(self.table()[index])
    }

    /// A shade with alpha (0..1), like Chakra's `blue.200/16` dark-mode tints.
    pub fn alpha(self, shade: u16, alpha: f32) -> Color {
        with_alpha(self.shade(shade), alpha)
    }
}

/// An opaque color from `0xRRGGBB`.
pub const fn hex(rgb: u32) -> Color {
    Color::from_rgba(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        255,
    )
}

/// The same color with alpha `alpha` (0..1).
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

/// White with alpha, Chakra's `whiteAlpha` scale.
pub fn white_alpha(alpha: f32) -> Color {
    Color::new(1., 1., 1., alpha.clamp(0., 1.))
}

/// Black with alpha, Chakra's `blackAlpha` scale.
pub fn black_alpha(alpha: f32) -> Color {
    Color::new(0., 0., 0., alpha.clamp(0., 1.))
}

/// Whether the dark color mode is in effect. Tracked.
pub fn is_dark() -> ReadSignal<bool> {
    use_color_mode().scheme.map(|scheme| scheme == ColorScheme::Dark)
}

/// A color that is `light` in the light mode and `dark` in the dark mode.
pub fn mode(light: Color, dark: Color) -> Prop<Brush> {
    let dark_mode = is_dark();
    computed(move |_| Brush::Solid(if dark_mode.get() { dark } else { light })).into()
}

/// Pick a light or dark color for the current mode inside a reactive
/// closure (tracked).
pub fn pick(light: Color, dark: Color) -> Color {
    if is_dark().get() { dark } else { light }
}

/// Text color.
pub fn fg() -> Prop<Brush> {
    mode(Palette::Gray.shade(800), white_alpha(0.92))
}

/// Secondary text: hints, captions, placeholders.
pub fn fg_muted() -> Prop<Brush> {
    mode(Palette::Gray.shade(600), Palette::Gray.shade(400))
}

/// Page background.
pub fn bg() -> Prop<Brush> {
    mode(hex(0xFFFFFF), Palette::Gray.shade(800))
}

/// Raised surfaces: cards, popovers, menus.
pub fn bg_panel() -> Prop<Brush> {
    mode(hex(0xFFFFFF), Palette::Gray.shade(700))
}

/// Subtle fills: filled inputs, tracks, hovered rows.
pub fn bg_subtle() -> Prop<Brush> {
    mode(Palette::Gray.shade(100), white_alpha(0.06))
}

/// Stronger fills: unchecked tracks, pressed rows.
pub fn bg_muted() -> Prop<Brush> {
    mode(Palette::Gray.shade(200), white_alpha(0.16))
}

/// Borders and dividers.
pub fn border() -> Prop<Brush> {
    mode(Palette::Gray.shade(200), white_alpha(0.16))
}

/// Borders of hovered fields.
pub fn border_emphasized() -> Prop<Brush> {
    mode(Palette::Gray.shade(300), white_alpha(0.24))
}

/// Focus rings and focused field borders.
pub fn focus_ring() -> Prop<Brush> {
    mode(Palette::Blue.shade(500), Palette::Blue.shade(300))
}

/// Error borders and messages.
pub fn error() -> Prop<Brush> {
    mode(Palette::Red.shade(500), Palette::Red.shade(300))
}

/// The palette's solid accent (Chakra: 500 light, 200 dark).
pub fn accent(palette: Palette) -> Prop<Brush> {
    mode(palette.shade(500), palette.shade(200))
}

/// Text on top of [`accent`].
pub fn on_accent(palette: Palette) -> Prop<Brush> {
    let light = match palette {
        Palette::Yellow | Palette::Cyan => Palette::Gray.shade(800),
        _ => hex(0xFFFFFF),
    };
    mode(light, Palette::Gray.shade(800))
}

/// Corner radii (Chakra's `radii`), in `rem`.
pub mod radius {
    use mue_macroquad::style::Size;

    pub const SM: Size = Size::Rem(0.125);
    pub const BASE: Size = Size::Rem(0.25);
    pub const MD: Size = Size::Rem(0.375);
    pub const LG: Size = Size::Rem(0.5);
    pub const XL: Size = Size::Rem(0.75);
    pub const XXL: Size = Size::Rem(1.);
    pub const FULL: Size = Size::Px(9999.);
}

/// The spacing scale (Chakra's `space`): `space(4.)` is 1rem.
pub const fn space(units: f32) -> Size {
    Size::Rem(units * 0.25)
}

/// Font sizes (Chakra's `fontSizes`), in `rem`.
pub mod font_size {
    use mue_macroquad::style::FontSize;

    pub const XS: FontSize = FontSize::Rem(0.75);
    pub const SM: FontSize = FontSize::Rem(0.875);
    pub const MD: FontSize = FontSize::Rem(1.);
    pub const LG: FontSize = FontSize::Rem(1.125);
    pub const XL: FontSize = FontSize::Rem(1.25);
    pub const XXL: FontSize = FontSize::Rem(1.5);
    pub const XXXL: FontSize = FontSize::Rem(1.875);
    pub const XXXXL: FontSize = FontSize::Rem(2.25);
}

/// The size of a control (Chakra's `size` prop).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ControlSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
}

impl ControlSize {
    fn rem(self, values: [f32; 4]) -> f32 {
        values[self as usize]
    }

    /// Height of buttons, inputs, and selects: 1.5, 2, 2.5, 3 rem.
    pub fn height(self) -> Size {
        Size::Rem(self.rem([1.5, 2., 2.5, 3.]))
    }

    /// Height in `rem` units.
    pub fn height_rem(self) -> f32 {
        self.rem([1.5, 2., 2.5, 3.])
    }

    /// Text size: xs, sm, md, lg.
    pub fn font_size(self) -> FontSize {
        FontSize::Rem(self.rem([0.75, 0.875, 1., 1.125]))
    }

    /// Line height matching the font size.
    pub fn line_height(self) -> Size {
        Size::Rem(self.rem([1., 1.25, 1.5, 1.75]))
    }

    /// Horizontal padding of buttons.
    pub fn button_padding(self) -> Size {
        Size::Rem(self.rem([0.5, 0.75, 1., 1.5]))
    }

    /// Horizontal padding of fields.
    pub fn field_padding(self) -> Size {
        Size::Rem(self.rem([0.5, 0.75, 1., 1.]))
    }

    /// Corner radius of buttons and fields.
    pub fn radius(self) -> Size {
        match self {
            ControlSize::Xs | ControlSize::Sm => radius::SM,
            ControlSize::Md | ControlSize::Lg => radius::MD,
        }
    }

    /// Box size of checkboxes and radios: 0.75, 0.875, 1, 1.25 rem.
    pub fn indicator(self) -> f32 {
        self.rem([0.75, 0.875, 1., 1.25])
    }

    /// Gap between an indicator or icon and its label.
    pub fn gap(self) -> Size {
        Size::Rem(self.rem([0.25, 0.375, 0.5, 0.5]))
    }
}
