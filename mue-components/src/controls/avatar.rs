//! A round user picture: [`avatar`], modeled on Chakra UI's `Avatar` and
//! `AvatarBadge`.
//!
//! An avatar shows an image when one is given, otherwise the initials of the
//! name on a color picked from the name (the same name always gets the same
//! color), otherwise a generic person. It comes in [`AvatarSize`]s from
//! `Xs` to `Xxl` (in `rem`) and can show a status dot in a corner.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::Palette;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let _dan = avatar().name("Dan Abramov").into_node();
//! let _kent = avatar().name("Kent Dodds").size(AvatarSize::Lg).badge(Palette::Green).into_node();
//! let _guest = avatar().size(AvatarSize::Sm).show_border(true).into_node();
//! # });
//! ```

use std::rc::Rc;

use mue_core::prelude::*;
use mue_macroquad::{
    layout::use_layout,
    math::vec2,
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox, text},
    paint::{
        color::Color,
        path::PathData,
        shader::{Brush, SharedTexture},
        use_paint,
    },
    style::{Size, Style, Styleable, StyleableExt},
};

use crate::{
    graphics::image,
    system::{self, Palette},
};

/// The diameter of an [`avatar`] (Chakra's sizes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AvatarSize {
    /// 1.5 rem.
    Xs,
    /// 2 rem.
    Sm,
    /// 3 rem.
    #[default]
    Md,
    /// 4 rem.
    Lg,
    /// 6 rem.
    Xl,
    /// 8 rem.
    Xxl,
}

impl AvatarSize {
    /// The diameter in `rem`.
    pub fn rem(self) -> f32 {
        [1.5, 2., 3., 4., 6., 8.][self as usize]
    }
}

/// Up to two initials: the first letters of the first and the last word.
///
/// ```
/// use mue_components::initials;
///
/// assert_eq!(initials("Dan Abramov"), "DA");
/// assert_eq!(initials("ada lovelace byron"), "AB");
/// assert_eq!(initials("Prince"), "P");
/// assert_eq!(initials("  "), "");
/// ```
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = |word: &str| word.chars().next();
    let letters: Vec<char> = match words.as_slice() {
        [] => Vec::new(),
        [only] => first(only).into_iter().collect(),
        [head, .., tail] => first(head).into_iter().chain(first(tail)).collect(),
    };
    letters.into_iter().flat_map(char::to_uppercase).collect()
}

/// A palette chosen from `name`, the same for the same name (never gray).
///
/// ```
/// use mue_components::{name_palette, system::Palette};
///
/// assert_eq!(name_palette("Kent Dodds"), name_palette("Kent Dodds"));
/// assert_ne!(name_palette("Kent Dodds"), Palette::Gray);
/// ```
pub fn name_palette(name: &str) -> Palette {
    // FNV-1a: stable across runs and platforms.
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let colors = &Palette::ALL[1..];
    colors[hash as usize % colors.len()]
}

/// Builder returned by [`avatar`].
pub struct AvatarBuilder {
    name: Option<Prop<Rc<str>>>,
    src: Option<Prop<SharedTexture>>,
    size: AvatarSize,
    badge: Option<Prop<Brush>>,
    show_border: bool,
    style: Style,
}

/// A medium avatar with a generic person until [`name`](AvatarBuilder::name)
/// or [`src`](AvatarBuilder::src) is set.
pub fn avatar() -> AvatarBuilder {
    AvatarBuilder {
        name: None,
        src: None,
        size: AvatarSize::Md,
        badge: None,
        show_border: false,
        style: Style::new(),
    }
}

impl AvatarBuilder {
    /// The person's name: its initials and color show without an image.
    pub fn name(mut self, name: impl Into<Prop<Rc<str>>>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The picture, cropped to the circle.
    pub fn src(mut self, texture: impl Into<Prop<SharedTexture>>) -> Self {
        self.src = Some(texture.into());
        self
    }

    pub fn size(mut self, size: AvatarSize) -> Self {
        self.size = size;
        self
    }

    /// A status dot at the bottom right in the palette's color
    /// (Chakra's `AvatarBadge`).
    pub fn badge(self, palette: Palette) -> Self {
        self.badge_color(system::mode(palette.shade(500), palette.shade(300)))
    }

    /// A status dot in any color.
    pub fn badge_color(mut self, color: impl Into<Prop<Brush>>) -> Self {
        self.badge = Some(color.into());
        self
    }

    /// A ring in the page color around the avatar, for stacked avatars.
    pub fn show_border(mut self, show: bool) -> Self {
        self.show_border = show;
        self
    }
}

impl Styleable for AvatarBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// A painted head and shoulders, clipped by the avatar's circle.
struct Person {
    color: Prop<Brush>,
    style: Style,
}

impl Styleable for Person {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for Person {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { color, style } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let rect = use_layout(style).rect;
            let paint = use_paint(style);
            style.on_render.append(move |_| {
                let rect = rect.get_untracked();
                let d = rect.w.min(rect.h);
                if d <= 0. {
                    return;
                }
                let c = rect.center();
                let brush = color.get_clone_untracked();
                paint.draw(|p| {
                    p.fill(
                        &PathData::circle(c + vec2(0., -d * 0.12), d * 0.2),
                        crate::FillRule::NonZero,
                        brush.shader(rect),
                    );
                    p.fill(
                        &PathData::ellipse(c + vec2(0., d * 0.46), vec2(d * 0.36, d * 0.3)),
                        crate::FillRule::NonZero,
                        brush.shader(rect),
                    );
                });
            });
        })
    }
}

impl BuildNode for AvatarBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            name,
            src,
            size,
            badge,
            show_border,
            style,
        } = self;
        let diameter = size.rem();
        let dark = system::is_dark();
        let name = name.unwrap_or_else(|| Prop::Static(Rc::from("")));
        let palette = name.map(|name| (!name.trim().is_empty()).then(|| name_palette(&name)));
        let background = computed(move |_| {
            Brush::Solid(match palette.get() {
                Some(palette) => palette.shade(if dark.get() { 300 } else { 500 }),
                None => Palette::Gray.shade(400),
            })
        });
        let foreground = computed(move |_| {
            let light_text = match palette.get() {
                Some(Palette::Yellow | Palette::Cyan) => false,
                Some(_) => !dark.get(),
                None => true,
            };
            Brush::Solid(if light_text { Color::WHITE } else { Palette::Gray.shade(800) })
        });
        let letters = name.map(|name| Rc::<str>::from(initials(&name)));
        let has_name = palette.map(|palette| palette.is_some());

        let mut face: Vec<Node> = vec![
            text(letters)
                .font_size(mue_macroquad::style::FontSize::Rem(diameter * 0.4))
                .line_height(Size::Rem(diameter * 0.5))
                .font_weight(cosmic_text::Weight::MEDIUM)
                .text_wrap(false)
                .color(foreground)
                .display(has_name.map(|shown| {
                    if shown { mue_macroquad::style::values::Display::Flex } else { mue_macroquad::style::values::Display::None }
                }))
                .into_node(),
            Person {
                color: system::white_alpha(0.9).into(),
                style: Style::new(),
            }
            .absolute()
            .inset((Size::Px(0.),))
            .opacity(has_name.map(|shown| if shown { 0. } else { 1. }))
            .into_node(),
        ];
        if let Some(src) = src {
            face.push(
                image(src)
                    .absolute()
                    .inset((Size::Px(0.),))
                    .width(Size::Percent(100.))
                    .height(Size::Percent(100.))
                    .object_cover()
                    .into_node(),
            );
        }
        let circle = flexbox()
            .relative()
            .items_center()
            .justify_center()
            .width(Size::Percent(100.))
            .height(Size::Percent(100.))
            .radius(system::radius::FULL)
            .clip_radius(Size::Rem(diameter / 2.))
            .background(background)
            .children(face);
        let mut children: Vec<Node> = vec![circle.into_node()];
        if let Some(color) = badge {
            let dot = diameter * 0.3;
            children.push(
                flexbox()
                    .absolute()
                    .inset_right(Size::Rem(diameter * 0.02))
                    .inset_bottom(Size::Rem(diameter * 0.02))
                    .width(Size::Rem(dot))
                    .height(Size::Rem(dot))
                    .radius(system::radius::FULL)
                    .border((Size::Rem((dot * 0.15).max(0.125)),))
                    .border_color(system::bg())
                    .background(color)
                    .into_node(),
            );
        }
        let mut root = flexbox()
            .relative()
            .flex_shrink(0.)
            .width(Size::Rem(diameter))
            .height(Size::Rem(diameter))
            .radius(system::radius::FULL);
        if show_border {
            root = root.outline(Size::Px(2.), system::bg(), Size::Px(0.));
        }
        root.children(children).style(style).build(ctx)
    }
}
