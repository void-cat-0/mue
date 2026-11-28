//! Loading placeholders: [`skeleton`], [`skeleton_text`], and
//! [`skeleton_circle`], modeled on Chakra UI's `Skeleton`.
//!
//! A skeleton is a muted shape with a light band sweeping across it. Wrap
//! the real content in it and set [`loaded`](SkeletonBuilder::loaded) once
//! the data is there: the placeholder keeps the content's size while
//! loading, then the content fades in. All skeletons on screen sweep in
//! step. Colors follow the color mode; sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::prelude::*;
//! use mue_macroquad::style::Size;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let ready = signal(false);
//! let _card = stack()
//!     .children((
//!         skeleton_circle(3.),
//!         skeleton_text().lines(3),
//!         skeleton().loaded(ready).children(text("Loaded content")),
//!         skeleton().height(Size::Rem(5.)),
//!     ))
//!     .into_node();
//! # });
//! ```

use mue_core::prelude::*;
use mue_macroquad::{
    math::vec2,
    motion::transition::Transition,
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox},
    paint::{
        color::Color,
        shader::{Brush, ColorStop, LinearGradient},
    },
    style::{Size, Style, Styleable, StyleableExt, values::Display},
};

use crate::system::{self, Palette, white_alpha};

/// Seconds for one sweep across a skeleton.
const PERIOD: f32 = 1.6;

/// The sweeping gradient: a base color with a lighter band at `phase`
/// (0..1 across the shape).
fn shimmer(phase: f32, base: Color, highlight: Color) -> Brush {
    // The gradient spans three widths (from -1 to 2) so the band can enter
    // and leave the shape; `phase` maps to the middle third.
    let center = (phase * 1.6 - 0.3 + 1.) / 3.;
    let half = 0.1;
    Brush::LinearGradient(LinearGradient::new(
        vec2(-1., 0.),
        vec2(2., 0.),
        [
            ColorStop::new(center - half, base),
            ColorStop::new(center, highlight),
            ColorStop::new(center + half, base),
        ],
    ))
}

/// A shimmering brush for one node while `active`; its phase follows the
/// global clock so separate skeletons move together.
fn shimmer_brush(style: Style, active: ReadSignal<bool>) -> (Style, ReadSignal<Brush>) {
    let phase = signal(0_f32);
    let dark = system::is_dark();
    let brush = computed(move |_| {
        if !active.get() {
            return Brush::Solid(Color::TRANSPARENT);
        }
        let (base, highlight) = if dark.get() {
            (white_alpha(0.08), white_alpha(0.2))
        } else {
            (Palette::Gray.shade(200), Palette::Gray.shade(50))
        };
        shimmer(phase.get(), base, highlight)
    });
    let style = style.on_update(move |_| {
        if active.get_untracked() {
            let time = mue_macroquad::app::runtime::get_time();
            phase.set((time % PERIOD) / PERIOD);
        }
    });
    (style, brush)
}

/// Move the caller's box geometry (size, flex, margins, corners) to the
/// placeholder, leaving the rest (children, text styles) for the content.
fn take_box(style: &mut Style) -> Style {
    let mut outer = Style::new();
    macro_rules! carry {
        ($($declared:ident, $take:ident, $set:ident;)*) => {$(
            if let Some(value) = style.$declared() {
                let _ = style.$take();
                outer = outer.$set(value);
            }
        )*};
    }
    carry! {
        declared_width, take_width, width;
        declared_height, take_height, height;
        declared_min_width, take_min_width, min_width;
        declared_min_height, take_min_height, min_height;
        declared_max_width, take_max_width, max_width;
        declared_max_height, take_max_height, max_height;
        declared_flex_grow, take_flex_grow, flex_grow;
        declared_flex_shrink, take_flex_shrink, flex_shrink;
        declared_flex_basis, take_flex_basis, flex_basis;
        declared_align_self, take_align_self, align_self;
        declared_margin_left, take_margin_left, margin_left;
        declared_margin_right, take_margin_right, margin_right;
        declared_margin_top, take_margin_top, margin_top;
        declared_margin_bottom, take_margin_bottom, margin_bottom;
        declared_radius_top_left, take_radius_top_left, radius_top_left;
        declared_radius_top_right, take_radius_top_right, radius_top_right;
        declared_radius_bottom_left, take_radius_bottom_left, radius_bottom_left;
        declared_radius_bottom_right, take_radius_bottom_right, radius_bottom_right;
    }
    outer
}

/// Builder returned by [`skeleton`] and [`skeleton_circle`].
pub struct SkeletonBuilder {
    loaded: Prop<bool>,
    fade: f32,
    circle: Option<f32>,
    style: Style,
}

/// A rectangular placeholder as wide as its container. Without children
/// give it a height; with children it takes their size and shows them once
/// [`loaded`](SkeletonBuilder::loaded).
pub fn skeleton() -> SkeletonBuilder {
    SkeletonBuilder {
        loaded: Prop::Static(false),
        fade: 0.4,
        circle: None,
        style: Style::new(),
    }
}

/// A round placeholder `diameter` rem wide, for avatars.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _avatar = skeleton_circle(2.5);
/// ```
pub fn skeleton_circle(diameter: f32) -> SkeletonBuilder {
    SkeletonBuilder {
        circle: Some(diameter),
        ..skeleton()
    }
}

impl SkeletonBuilder {
    /// Show the children instead of the placeholder (Chakra's `isLoaded`).
    pub fn loaded(mut self, loaded: impl Into<Prop<bool>>) -> Self {
        self.loaded = loaded.into();
        self
    }

    /// Seconds the content takes to fade in once loaded (0.4 by default).
    pub fn fade_duration(mut self, seconds: f32) -> Self {
        self.fade = seconds.max(0.);
        self
    }
}

impl Styleable for SkeletonBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for SkeletonBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            loaded,
            fade,
            circle,
            mut style,
        } = self;
        let loading = computed(move |_| !loaded.get());
        // Sizes go to the placeholder; the children go to the content, which
        // keeps its size but stays invisible while loading.
        let geometry = take_box(&mut style);
        let content = flexbox()
            .flex_column()
            .flex_grow(1.)
            .width(Size::Percent(100.))
            .opacity(loading.map(|loading| if loading { 0. } else { 1. }))
            .transition(Transition::ease_out(fade))
            .style(style);
        let (root, brush) = shimmer_brush(Style::new(), loading);
        let mut root = flexbox()
            .style(root)
            .flex_column()
            .flex_shrink(0.)
            .background(brush)
            .radius(system::radius::SM);
        root = match circle {
            Some(diameter) => root
                .width(Size::Rem(diameter))
                .height(Size::Rem(diameter))
                .radius(system::radius::FULL),
            None => root.width(Size::Percent(100.)),
        };
        root.style(geometry).children(content).build(ctx)
    }
}

/// Builder returned by [`skeleton_text`].
pub struct SkeletonTextBuilder {
    lines: usize,
    line_height: f32,
    spacing: f32,
    loaded: Prop<bool>,
    style: Style,
}

/// Placeholder lines of text: three lines 0.5 rem tall and 0.5 rem apart,
/// the last one shorter. With children it shows them once
/// [`loaded`](SkeletonTextBuilder::loaded).
///
/// ```
/// use mue_components::prelude::*;
///
/// let _paragraph = skeleton_text().lines(4).spacing(0.75);
/// ```
pub fn skeleton_text() -> SkeletonTextBuilder {
    SkeletonTextBuilder {
        lines: 3,
        line_height: 0.5,
        spacing: 0.5,
        loaded: Prop::Static(false),
        style: Style::new(),
    }
}

impl SkeletonTextBuilder {
    /// Number of lines (Chakra's `noOfLines`).
    pub fn lines(mut self, lines: usize) -> Self {
        self.lines = lines.max(1);
        self
    }

    /// Height of each line in `rem` (Chakra's `skeletonHeight`).
    pub fn line_height(mut self, rem: f32) -> Self {
        self.line_height = rem;
        self
    }

    /// Space between lines in `rem`.
    pub fn spacing(mut self, rem: f32) -> Self {
        self.spacing = rem;
        self
    }

    /// Show the children instead of the lines.
    pub fn loaded(mut self, loaded: impl Into<Prop<bool>>) -> Self {
        self.loaded = loaded.into();
        self
    }
}

impl Styleable for SkeletonTextBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for SkeletonTextBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            lines,
            line_height,
            spacing,
            loaded,
            mut style,
        } = self;
        let loading = computed(move |_| !loaded.get());
        let geometry = take_box(&mut style);
        let bars: Vec<Node> = (0..lines)
            .map(|index| {
                let last = index + 1 == lines && lines > 1;
                skeleton()
                    .height(Size::Rem(line_height))
                    .width(Size::Percent(if last { 80. } else { 100. }))
                    .into_node()
            })
            .collect();
        let placeholder = flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .gap(Size::Rem(spacing))
            .display(loading.map(|loading| if loading { Display::Flex } else { Display::None }))
            .children(bars);
        let content = flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .display(loading.map(|loading| if loading { Display::None } else { Display::Flex }))
            .opacity(loading.map(|loading| if loading { 0. } else { 1. }))
            .transition(Transition::ease_out(0.4))
            .style(style);
        flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .style(geometry)
            .children((placeholder, content))
            .build(ctx)
    }
}
