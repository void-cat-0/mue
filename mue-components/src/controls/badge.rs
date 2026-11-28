//! A small status label: [`badge`], modeled on Chakra UI's `Badge`.
//!
//! Badges draw short uppercase text in a [`BadgeVariant`] (solid, subtle,
//! outline) and a [`Palette`]; colors follow the color mode and sizes are in
//! `rem`, so a badge also fits inside a heading or a button.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::Palette;
//!
//! let _new = badge("New").palette(Palette::Green);
//! let _beta = badge("Beta").variant(BadgeVariant::Solid).palette(Palette::Purple);
//! let _draft = badge("Draft").variant(BadgeVariant::Outline);
//! ```

use std::rc::Rc;

use mue_core::prelude::*;
use mue_macroquad::{
    node::{BuildNode, Node, NodeContext, flexbox, text},
    paint::{color::Color, shader::Brush},
    style::{Size, Style, Styleable, StyleableExt},
};

use crate::system::{self, Palette, white_alpha};

/// Visual style of a [`badge`] or a [`tag`](crate::tag).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Filled with the palette color.
    Solid,
    /// A light tint of the palette color.
    #[default]
    Subtle,
    /// A palette-colored border and text.
    Outline,
}

/// Background, text, and border colors of a badge or tag (Chakra's recipe).
pub(crate) fn badge_colors(variant: BadgeVariant, palette: Palette, dark: bool) -> (Color, Color, Color) {
    let clear = Color::TRANSPARENT;
    let gray = palette == Palette::Gray;
    match variant {
        BadgeVariant::Solid => {
            if dark {
                (palette.alpha(500, 0.6), white_alpha(0.8), clear)
            } else {
                (palette.shade(500), Color::WHITE, clear)
            }
        }
        BadgeVariant::Subtle => match (dark, gray) {
            (false, _) => (palette.shade(100), palette.shade(800), clear),
            (true, true) => (white_alpha(0.16), Palette::Gray.shade(200), clear),
            (true, false) => (palette.alpha(200, 0.16), palette.shade(200), clear),
        },
        BadgeVariant::Outline => {
            let color = if dark { palette.alpha(200, 0.8) } else { palette.shade(500) };
            (clear, color, color)
        }
    }
}

/// Builder returned by [`badge`].
pub struct BadgeBuilder {
    label: Prop<Rc<str>>,
    variant: BadgeVariant,
    palette: Palette,
    style: Style,
}

/// A badge showing `label` in uppercase: subtle gray by default.
pub fn badge(label: impl Into<Prop<Rc<str>>>) -> BadgeBuilder {
    BadgeBuilder {
        label: label.into(),
        variant: BadgeVariant::Subtle,
        palette: Palette::Gray,
        style: Style::new(),
    }
}

impl BadgeBuilder {
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }
}

impl Styleable for BadgeBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for BadgeBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            label,
            variant,
            palette,
            style,
        } = self;
        let dark = system::is_dark();
        let colors = dark.map(move |dark| badge_colors(variant, palette, dark));
        let mut root = flexbox()
            .flex_row()
            .items_center()
            .flex_shrink(0.)
            .padding_x(Size::Rem(0.25))
            .radius(system::radius::SM)
            .background(colors.map(|(background, _, _)| Brush::Solid(background)))
            .color(colors.map(|(_, foreground, _)| Brush::Solid(foreground)));
        if variant == BadgeVariant::Outline {
            root = root
                .border((Size::Px(1.),))
                .border_color(colors.map(|(_, _, border)| Brush::Solid(border)));
        }
        let label = label.map(|label| Rc::<str>::from(label.to_uppercase()));
        root.children(
            text(label)
                .font_size(system::font_size::XS)
                .line_height(Size::Rem(1.125))
                .font_weight(cosmic_text::Weight::BOLD)
                .letter_spacing(Size::Rem(0.025))
                .text_wrap(false)
                .color(colors.map(|(_, foreground, _)| Brush::Solid(foreground))),
        )
        .style(style)
        .build(ctx)
    }
}
