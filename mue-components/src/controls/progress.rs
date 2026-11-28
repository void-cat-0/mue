//! Progress indicators: the linear [`progress`] bar and the ring-shaped
//! [`circular_progress`], modeled on Chakra UI's `Progress` and
//! `CircularProgress`.
//!
//! Both show a value between `min` and `max` (0 and 100 by default) and
//! glide to a new value, or run an endless animation while
//! `indeterminate`. Colors follow the [`Palette`] and the color mode; sizes
//! are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{ControlSize, Palette};
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let uploaded = signal(30_f32);
//! let _bar = progress().value(uploaded).has_stripe(true).is_animated(true).into_node();
//! let _busy = progress().indeterminate(true).size(ControlSize::Xs).into_node();
//! let _ring = circular_progress()
//!     .value(uploaded)
//!     .palette(Palette::Green)
//!     .label(text(uploaded.map(|v| std::rc::Rc::<str>::from(format!("{v:.0}%")))))
//!     .into_node();
//! # });
//! ```

use std::f32::consts::PI;

use mue_core::prelude::*;
use mue_macroquad::{
    layout::use_layout,
    math::{Vector, vec2},
    motion::transition::{Transition, animated},
    node::{BuildNode, IntoNode, Node, NodeContext, flexbox},
    paint::{
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{Size, Style, Styleable, StyleableExt, root_font_size},
};

use crate::system::{self, ControlSize, Palette, white_alpha};

/// The track behind the filled part.
fn track_color() -> Prop<Brush> {
    system::mode(Palette::Gray.shade(100), white_alpha(0.3))
}

/// The fraction (0..1) of `value` between `min` and `max`.
fn fraction(value: Prop<f32>, min: f32, max: f32) -> ReadSignal<f32> {
    let span = (max - min).abs().max(f32::EPSILON);
    let low = min.min(max);
    computed(move |_| ((value.get() - low) / span).clamp(0., 1.))
}

/// Builder returned by [`progress`].
pub struct ProgressBuilder {
    value: Prop<f32>,
    min: f32,
    max: f32,
    indeterminate: Prop<bool>,
    has_stripe: Prop<bool>,
    is_animated: Prop<bool>,
    palette: Palette,
    size: ControlSize,
    style: Style,
}

/// A horizontal progress bar as wide as its container: blue, medium (0.75
/// rem tall), at 0 of 100.
///
/// The filled part glides to each new [`value`](ProgressBuilder::value).
/// [`has_stripe`](ProgressBuilder::has_stripe) draws diagonal stripes on it
/// and [`is_animated`](ProgressBuilder::is_animated) moves them;
/// [`indeterminate`](ProgressBuilder::indeterminate) replaces the value with
/// a segment that slides across the track. All four accept reactive values.
pub fn progress() -> ProgressBuilder {
    ProgressBuilder {
        value: Prop::Static(0.),
        min: 0.,
        max: 100.,
        indeterminate: Prop::Static(false),
        has_stripe: Prop::Static(false),
        is_animated: Prop::Static(false),
        palette: Palette::Blue,
        size: ControlSize::Md,
        style: Style::new(),
    }
}

impl ProgressBuilder {
    pub fn value(mut self, value: impl Into<Prop<f32>>) -> Self {
        self.value = value.into();
        self
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// Show activity without a known value.
    pub fn indeterminate(mut self, indeterminate: impl Into<Prop<bool>>) -> Self {
        self.indeterminate = indeterminate.into();
        self
    }

    /// Draw diagonal stripes on the filled part (Chakra's `hasStripe`).
    pub fn has_stripe(mut self, stripe: impl Into<Prop<bool>>) -> Self {
        self.has_stripe = stripe.into();
        self
    }

    /// Move the stripes (Chakra's `isAnimated`); implies nothing without
    /// [`has_stripe`](Self::has_stripe).
    pub fn is_animated(mut self, animated: impl Into<Prop<bool>>) -> Self {
        self.is_animated = animated.into();
        self
    }

    /// The fill color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Bar height: 0.25, 0.5, 0.75, or 1 rem for xs to lg.
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styleable for ProgressBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Paints diagonal stripes over its own rect, moving while `moving`.
struct Stripes {
    shown: Prop<bool>,
    moving: Prop<bool>,
    style: Style,
}

impl Styleable for Stripes {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for Stripes {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            shown,
            moving,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let rect = use_layout(style).rect;
            let paint = use_paint(style);
            let rem = root_font_size();
            let mut shift = 0_f32;
            style.on_update.append(move |dt| {
                if moving.get_untracked() {
                    shift += dt;
                }
            });
            style.on_render.append(move |_| {
                let rect = rect.get_untracked();
                if !shown.get_untracked() || rect.w <= 0. || rect.h <= 0. {
                    return;
                }
                // Chakra: 45° white stripes, one per rem, moving one period a second.
                let period = rem.get_untracked().max(4.);
                let offset = (shift % 1.) * period;
                let h = rect.h;
                let brush = Brush::Solid(white_alpha(0.15));
                paint.draw(|p| {
                    let mut x = rect.x - h - period + offset;
                    while x < rect.right() {
                        let stripe = PathData::polygon([
                            vec2(x, rect.bottom()),
                            vec2(x + period / 2., rect.bottom()),
                            vec2(x + period / 2. + h, rect.y),
                            vec2(x + h, rect.y),
                        ]);
                        p.fill(&stripe, crate::FillRule::NonZero, brush.shader(rect));
                        x += period;
                    }
                });
            });
        })
    }
}

impl BuildNode for ProgressBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            value,
            min,
            max,
            indeterminate,
            has_stripe,
            is_animated,
            palette,
            size,
            style,
        } = self;
        let height = [0.25, 0.5, 0.75, 1.][size as usize];
        let fraction = fraction(value, min, max);
        // Phase of the indeterminate segment, 0..1 once a second.
        let phase = signal(0_f32);
        let left = computed(move |_| {
            if indeterminate.get() {
                Size::Percent(-40. + 140. * phase.get())
            } else {
                Size::Percent(0.)
            }
        });
        let width = computed(move |_| {
            if indeterminate.get() {
                Size::Percent(40.)
            } else {
                Size::Percent(fraction.get() * 100.)
            }
        });
        let glide = indeterminate.map(|indeterminate| {
            if indeterminate { Transition::Instant } else { Transition::ease_out(0.3) }
        });
        let fill = system::accent(palette);
        let indicator = flexbox()
            .absolute()
            .inset_top(Size::Px(0.))
            .inset_left(left)
            .height(Size::Percent(100.))
            .width(width)
            .background(fill)
            .clip_radius(Size::Rem(height / 2.))
            .transition(glide)
            .children(
                Stripes {
                    shown: computed(move |_| has_stripe.get() && !indeterminate.get()).into(),
                    moving: is_animated,
                    style: Style::new(),
                }
                .width(Size::Percent(100.))
                .height(Size::Percent(100.)),
            );
        flexbox()
            .relative()
            .width(Size::Percent(100.))
            .height(Size::Rem(height))
            .flex_shrink(0.)
            .background(track_color())
            .radius(Size::Rem(height / 2.))
            .clip_radius(Size::Rem(height / 2.))
            .on_update(move |dt| {
                if indeterminate.get_untracked() {
                    phase.set((phase.get_untracked() + dt) % 1.);
                }
            })
            .children(indicator)
            .style(style)
            .build(ctx)
    }
}

/// Builder returned by [`circular_progress`].
pub struct CircularProgressBuilder {
    value: Prop<f32>,
    min: f32,
    max: f32,
    indeterminate: Prop<bool>,
    palette: Palette,
    diameter: Size,
    thickness: f32,
    color: Option<Prop<Brush>>,
    track: Option<Prop<Brush>>,
    label: Option<Node>,
    style: Style,
}

/// A ring that fills clockwise from the top: blue, 3 rem wide, at 0 of 100.
///
/// The arc glides to each new [`value`](CircularProgressBuilder::value);
/// while [`indeterminate`](CircularProgressBuilder::indeterminate) a short
/// arc spins and breathes instead. A [`label`](CircularProgressBuilder::label)
/// is centered inside the ring (Chakra's `CircularProgressLabel`).
///
/// ```
/// use mue_components::prelude::*;
/// use mue_macroquad::style::Size;
///
/// let _loading = circular_progress().indeterminate(true).diameter(Size::Rem(2.)).thickness(0.15);
/// ```
pub fn circular_progress() -> CircularProgressBuilder {
    CircularProgressBuilder {
        value: Prop::Static(0.),
        min: 0.,
        max: 100.,
        indeterminate: Prop::Static(false),
        palette: Palette::Blue,
        diameter: Size::Rem(3.),
        thickness: 0.1,
        color: None,
        track: None,
        label: None,
        style: Style::new(),
    }
}

impl CircularProgressBuilder {
    pub fn value(mut self, value: impl Into<Prop<f32>>) -> Self {
        self.value = value.into();
        self
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// Spin without a known value.
    pub fn indeterminate(mut self, indeterminate: impl Into<Prop<bool>>) -> Self {
        self.indeterminate = indeterminate.into();
        self
    }

    /// The arc color (Chakra's `color`), from a palette.
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Width and height of the ring (Chakra's `size`); 3 rem by default.
    pub fn diameter(mut self, diameter: impl Into<Size>) -> Self {
        self.diameter = diameter.into();
        self
    }

    /// Stroke width as a fraction of the diameter (0.1 by default, like
    /// Chakra's `thickness` of 10).
    pub fn thickness(mut self, fraction: f32) -> Self {
        self.thickness = fraction.clamp(0.02, 0.5);
        self
    }

    /// The arc color, overriding the palette.
    pub fn color(mut self, color: impl Into<Prop<Brush>>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// The color of the full ring behind the arc (Chakra's `trackColor`).
    pub fn track(mut self, color: impl Into<Prop<Brush>>) -> Self {
        self.track = Some(color.into());
        self
    }

    /// Content centered in the ring, such as the value in percent.
    pub fn label(mut self, label: impl IntoNode) -> Self {
        self.label = Some(label.into_node());
        self
    }
}

impl Styleable for CircularProgressBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// The painted ring of a [`circular_progress`].
struct Ring {
    fraction: Prop<f32>,
    indeterminate: Prop<bool>,
    thickness: f32,
    color: Prop<Brush>,
    track: Prop<Brush>,
    style: Style,
}

impl Styleable for Ring {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for Ring {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            fraction,
            indeterminate,
            thickness,
            color,
            track,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let rect = use_layout(style).rect;
            let paint = use_paint(style);
            let mut time = 0_f32;
            style.on_update.append(move |dt| time += dt);
            style.on_render.append(move |_| {
                let rect = rect.get_untracked();
                let d = rect.w.min(rect.h);
                if d <= 0. {
                    return;
                }
                let width = (d * thickness).max(1.);
                let radius = (d - width) / 2.;
                let center = rect.center();
                let (start, sweep) = if indeterminate.get_untracked() {
                    // Chakra's spin: a turn every 2 s while the arc breathes.
                    let start = (time * PI) % (2. * PI) - PI / 2.;
                    (start, PI * (0.6 + 0.5 * (time * 2.).sin()))
                } else {
                    (-PI / 2., 2. * PI * fraction.get_untracked().clamp(0., 1.))
                };
                let track = track.get_clone_untracked();
                let brush = color.get_clone_untracked();
                paint.draw(|p| {
                    p.stroke(
                        &PathData::circle(center, radius),
                        &StrokeStyle::new(width),
                        track.shader(rect),
                    );
                    if sweep > 0.001 {
                        let steps = ((sweep / (2. * PI)) * 64.).ceil().max(2.) as usize;
                        let arc = PathData::polyline((0..=steps).map(|i| {
                            let angle = start + sweep * i as f32 / steps as f32;
                            center + Vector::new(angle.cos(), angle.sin()) * radius
                        }));
                        p.stroke(&arc, &StrokeStyle::round(width), brush.shader(rect));
                    }
                });
            });
        })
    }
}

impl BuildNode for CircularProgressBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            value,
            min,
            max,
            indeterminate,
            palette,
            diameter,
            thickness,
            color,
            track,
            label,
            style,
        } = self;
        let shown = animated(fraction(value, min, max), Transition::ease_out(0.4));
        let ring = Ring {
            fraction: shown,
            indeterminate,
            thickness,
            color: color.unwrap_or_else(|| system::accent(palette)),
            track: track.unwrap_or_else(track_color),
            style: Style::new(),
        }
        .absolute()
        .inset((Size::Px(0.),));
        let mut children: Vec<Node> = vec![ring.into_node()];
        if let Some(label) = label {
            children.push(
                flexbox()
                    .items_center()
                    .justify_center()
                    .font_size(system::font_size::XS)
                    .line_height(Size::Rem(1.))
                    .color(system::fg())
                    .children(label)
                    .into_node(),
            );
        }
        flexbox()
            .relative()
            .items_center()
            .justify_center()
            .flex_shrink(0.)
            .width(diameter)
            .height(diameter)
            .children(children)
            .style(style)
            .build(ctx)
    }
}
