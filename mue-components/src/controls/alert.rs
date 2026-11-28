//! Status messages: [`alert`], [`alert_title`], and [`alert_description`],
//! modeled on Chakra UI's `Alert`.
//!
//! An alert has an [`AlertStatus`] (info, warning, success, error), which
//! picks its color and icon, and an [`AlertVariant`] (subtle, solid,
//! left-accent, top-accent). Pass the title and the description as
//! children; they stack after the icon. Colors follow the color mode; sizes
//! are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let _saved = alert(AlertStatus::Success)
//!     .children(alert_title("Data uploaded to the server"))
//!     .into_node();
//! let _failed = alert(AlertStatus::Error)
//!     .variant(AlertVariant::LeftAccent)
//!     .on_close(|| println!("dismissed"))
//!     .children((
//!         alert_title("Your browser is outdated!"),
//!         alert_description("Your experience may be degraded."),
//!     ))
//!     .into_node();
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
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{Size, Style, Styleable, StyleableExt},
};

use crate::{
    controls::tag::close_button,
    state::{provide_part, try_use_part},
    system::{self, Palette},
};

/// What an [`alert`] reports; it picks the color and the icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertStatus {
    /// Blue, with an "i".
    #[default]
    Info,
    /// Orange, with an "!".
    Warning,
    /// Green, with a check mark.
    Success,
    /// Red, with an "!".
    Error,
}

impl AlertStatus {
    /// The status color (Chakra's mapping).
    pub fn palette(self) -> Palette {
        match self {
            AlertStatus::Info => Palette::Blue,
            AlertStatus::Warning => Palette::Orange,
            AlertStatus::Success => Palette::Green,
            AlertStatus::Error => Palette::Red,
        }
    }
}

/// Visual style of an [`alert`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// A light tint of the status color.
    #[default]
    Subtle,
    /// Filled with the status color.
    Solid,
    /// Subtle, with a thick bar on the left.
    LeftAccent,
    /// Subtle, with a thick bar on top.
    TopAccent,
}

#[derive(Clone)]
struct AlertContext {
    foreground: Prop<Brush>,
}

/// Builder returned by [`alert`].
pub struct AlertBuilder {
    status: AlertStatus,
    variant: AlertVariant,
    palette: Option<Palette>,
    show_icon: bool,
    on_close: Option<Box<dyn FnMut()>>,
    style: Style,
}

/// An alert box as wide as its container, subtle by default. Pass
/// [`alert_title`] and [`alert_description`] (or any content) with
/// `.children(..)`.
///
/// Builder styles apply to the column that holds the children, between the
/// icon and the close button.
pub fn alert(status: AlertStatus) -> AlertBuilder {
    AlertBuilder {
        status,
        variant: AlertVariant::Subtle,
        palette: None,
        show_icon: true,
        on_close: None,
        style: Style::new(),
    }
}

impl AlertBuilder {
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Override the status color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    /// Draw the status icon before the content (default on).
    pub fn show_icon(mut self, show: bool) -> Self {
        self.show_icon = show;
        self
    }

    /// Show a close button at the end that runs `action`.
    pub fn on_close(mut self, action: impl FnMut() + 'static) -> Self {
        self.on_close = Some(Box::new(action));
        self
    }
}

impl Styleable for AlertBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// The painted status icon: a filled circle with a glyph cut out of it.
struct StatusIcon {
    status: AlertStatus,
    color: Prop<Brush>,
    glyph: Prop<Brush>,
    style: Style,
}

impl Styleable for StatusIcon {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for StatusIcon {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            status,
            color,
            glyph,
            style,
        } = self;
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
                let at = |x: f32, y: f32| c + vec2(x * d, y * d);
                let fill = color.get_clone_untracked();
                let mark = glyph.get_clone_untracked();
                let stroke = StrokeStyle::round((d * 0.12).max(1.5));
                paint.draw(|p| {
                    p.fill(&PathData::circle(c, d / 2.), crate::FillRule::NonZero, fill.shader(rect));
                    match status {
                        AlertStatus::Success => p.stroke(
                            &PathData::polyline([at(-0.22, 0.02), at(-0.06, 0.18), at(0.22, -0.14)]),
                            &stroke,
                            mark.shader(rect),
                        ),
                        AlertStatus::Info => {
                            p.fill(&PathData::circle(at(0., -0.22), d * 0.075), crate::FillRule::NonZero, mark.shader(rect));
                            p.stroke(&PathData::line(at(0., -0.04), at(0., 0.24)), &stroke, mark.shader(rect));
                        }
                        AlertStatus::Warning | AlertStatus::Error => {
                            p.stroke(&PathData::line(at(0., -0.24), at(0., 0.04)), &stroke, mark.shader(rect));
                            p.fill(&PathData::circle(at(0., 0.22), d * 0.075), crate::FillRule::NonZero, mark.shader(rect));
                        }
                    }
                });
            });
        })
    }
}

impl BuildNode for AlertBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            status,
            variant,
            palette,
            show_icon,
            on_close,
            style,
        } = self;
        let palette = palette.unwrap_or(status.palette());
        let dark = system::is_dark();
        let solid = variant == AlertVariant::Solid;
        let background = computed(move |_| {
            Brush::Solid(match (solid, dark.get()) {
                (true, false) => palette.shade(500),
                (true, true) => palette.shade(200),
                (false, false) => palette.shade(100),
                (false, true) => palette.alpha(200, 0.16),
            })
        });
        let accent = system::accent(palette);
        let foreground: Prop<Brush> = if solid {
            system::mode(Color::WHITE, Palette::Gray.shade(900))
        } else {
            system::fg()
        };
        let icon_color = if solid { foreground.clone() } else { accent.clone() };
        provide_part(AlertContext {
            foreground: foreground.clone(),
        });

        let mut root = flexbox()
            .flex_row()
            .items_center()
            .width(Size::Percent(100.))
            .padding_x(Size::Rem(1.))
            .padding_y(Size::Rem(0.75))
            .gap(Size::Rem(0.75))
            .radius(system::radius::MD)
            .background(background)
            .color(foreground.clone());
        root = match variant {
            AlertVariant::LeftAccent => root
                .padding_left(Size::Rem(0.75))
                .border_left(Size::Rem(0.25))
                .border_color(accent)
                .radius(Size::Px(0.)),
            AlertVariant::TopAccent => root
                .padding_top(Size::Rem(0.5))
                .border_top(Size::Rem(0.25))
                .border_color(accent)
                .radius(Size::Px(0.)),
            AlertVariant::Subtle | AlertVariant::Solid => root,
        };
        // The caller's title and description stack in a column.
        let content = flexbox()
            .flex_column()
            .flex_grow(1.)
            .flex_shrink(1.)
            .min_width(Size::Px(0.))
            .style(style);
        let mut children: Vec<Node> = Vec::new();
        if show_icon {
            children.push(
                StatusIcon {
                    status,
                    color: icon_color,
                    glyph: background.into(),
                    style: Style::new(),
                }
                .width(Size::Rem(1.25))
                .height(Size::Rem(1.25))
                .flex_shrink(0.)
                .into_node(),
            );
        }
        children.push(content.into_node());
        if let Some(action) = on_close {
            children.push(
                close_button(Size::Rem(1.5), foreground, Prop::Static(false), action)
                    .margin_right(Size::Rem(-0.25))
                    .into_node(),
            );
        }
        root.children(children).build(ctx)
    }
}

fn foreground() -> Prop<Brush> {
    try_use_part::<AlertContext>().map_or_else(system::fg, |alert| alert.foreground)
}

/// Builder returned by [`alert_title`] and [`alert_description`].
pub struct AlertTextBuilder {
    text: Prop<Rc<str>>,
    title: bool,
    style: Style,
}

/// The bold first line of an [`alert`].
///
/// ```
/// use mue_components::prelude::*;
///
/// let _title = alert_title("Update available");
/// ```
pub fn alert_title(title: impl Into<Prop<Rc<str>>>) -> AlertTextBuilder {
    AlertTextBuilder {
        text: title.into(),
        title: true,
        style: Style::new(),
    }
}

/// The body text of an [`alert`], below the title.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _body = alert_description("Restart to finish installing.");
/// ```
pub fn alert_description(description: impl Into<Prop<Rc<str>>>) -> AlertTextBuilder {
    AlertTextBuilder {
        text: description.into(),
        title: false,
        style: Style::new(),
    }
}

impl Styleable for AlertTextBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for AlertTextBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self { text: content, title, style } = self;
        let weight = if title { cosmic_text::Weight::BOLD } else { cosmic_text::Weight::NORMAL };
        text(content)
            .width(Size::Percent(100.))
            .font_size(system::font_size::MD)
            .line_height(Size::Rem(1.5))
            .font_weight(weight)
            .color(foreground())
            .style(style)
            .build(ctx)
    }
}
