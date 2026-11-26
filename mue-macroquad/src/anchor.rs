//! Position floating content next to an anchor (popovers, menus, tooltips).
//!
//! [`anchored`] is an absolutely positioned container that places itself
//! beside an anchor rectangle on a chosen side, flips to the opposite side when
//! it would overflow the boundary, and shifts along the side to stay inside.
//! The anchor can be a node (through a [`NodeHandle`]), a rectangle, or a
//! point, all in window logical pixels. Children can read the final placement
//! and an arrow offset with [`use_anchor`].
//!
//! Put anchored content in a [`teleport`](crate::layer::teleport) so it draws
//! above everything and is not clipped by scroll areas.
//!
//! ```
//! use mue_macroquad::{
//!     anchor::{Placement, anchored},
//!     layer::{Layer, teleport},
//!     node::{NodeHandle, flexbox},
//!     style::{Styleable, StyleableExt},
//! };
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let trigger = NodeHandle::new();
//! let _button = flexbox().node_handle(trigger).width(80.).height(32.);
//! let _menu = teleport(Layer::Popover, move || {
//!     anchored(trigger)
//!         .placement(Placement::BottomStart)
//!         .offset(6.)
//!         .children(flexbox().width(160.).height(120.))
//! });
//! # });
//! ```

use mue_core::{
    effect::computed,
    prop::{Prop, PropValue},
    scope::{inject, provide},
    signal::{Access, ReadSignal, Signal, signal},
};

use crate::{
    layout::use_layout,
    math::{Rect, Vector, vec2},
    node::{BuildNode, Node, NodeContext, NodeHandle, NodeRef},
    paint::use_background,
    style::{Size, Style, Styleable, StyleableExt},
};

/// The anchor side and alignment of floating content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    Top,
    TopStart,
    TopEnd,
    #[default]
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}
impl PropValue for Placement {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Start,
    Center,
    End,
}

impl Placement {
    fn parts(self) -> (Side, Align) {
        use Placement::*;
        match self {
            Top => (Side::Top, Align::Center),
            TopStart => (Side::Top, Align::Start),
            TopEnd => (Side::Top, Align::End),
            Bottom => (Side::Bottom, Align::Center),
            BottomStart => (Side::Bottom, Align::Start),
            BottomEnd => (Side::Bottom, Align::End),
            Left => (Side::Left, Align::Center),
            LeftStart => (Side::Left, Align::Start),
            LeftEnd => (Side::Left, Align::End),
            Right => (Side::Right, Align::Center),
            RightStart => (Side::Right, Align::Start),
            RightEnd => (Side::Right, Align::End),
        }
    }

    fn from_parts(side: Side, align: Align) -> Self {
        use Placement::*;
        match (side, align) {
            (Side::Top, Align::Center) => Top,
            (Side::Top, Align::Start) => TopStart,
            (Side::Top, Align::End) => TopEnd,
            (Side::Bottom, Align::Center) => Bottom,
            (Side::Bottom, Align::Start) => BottomStart,
            (Side::Bottom, Align::End) => BottomEnd,
            (Side::Left, Align::Center) => Left,
            (Side::Left, Align::Start) => LeftStart,
            (Side::Left, Align::End) => LeftEnd,
            (Side::Right, Align::Center) => Right,
            (Side::Right, Align::Start) => RightStart,
            (Side::Right, Align::End) => RightEnd,
        }
    }

    /// The placement on the opposite side with the same alignment.
    pub fn flipped(self) -> Self {
        let (side, align) = self.parts();
        let side = match side {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        };
        Self::from_parts(side, align)
    }

    /// Whether the content sits above or below the anchor.
    pub fn is_vertical(self) -> bool {
        matches!(self.parts().0, Side::Top | Side::Bottom)
    }
}

/// What the floating content is attached to, in window logical pixels.
#[derive(Clone)]
pub enum AnchorTarget {
    Node(NodeHandle),
    NodeRef(NodeRef),
    Rect(Prop<Rect>),
    Point(Prop<Vector>),
}

impl From<NodeHandle> for AnchorTarget {
    fn from(handle: NodeHandle) -> Self {
        Self::Node(handle)
    }
}
impl From<NodeRef> for AnchorTarget {
    fn from(node: NodeRef) -> Self {
        Self::NodeRef(node)
    }
}
impl From<Rect> for AnchorTarget {
    fn from(rect: Rect) -> Self {
        Self::Rect(Prop::Static(rect))
    }
}
impl From<Vector> for AnchorTarget {
    fn from(point: Vector) -> Self {
        Self::Point(Prop::Static(point))
    }
}

impl AnchorTarget {
    fn rect(&self) -> Option<Rect> {
        match self {
            Self::Node(handle) => handle.rect(),
            Self::NodeRef(node) => node.rect()?.try_get_untracked(),
            Self::Rect(rect) => rect.try_get_untracked(),
            Self::Point(point) => point
                .try_get_untracked()
                .map(|p| Rect::new(p.x, p.y, 0., 0.)),
        }
    }
}

/// Placement results for the anchored content's children.
#[derive(Clone, Copy)]
pub struct AnchorState {
    /// The placement after flipping.
    pub placement: ReadSignal<Placement>,
    /// Offset of the anchor's center from the content's leading edge along the
    /// anchored side (x for top/bottom, y for left/right), clamped inside the
    /// content. Use it to position an arrow.
    pub arrow: ReadSignal<f32>,
    /// The anchor rectangle used for the last placement.
    pub anchor: ReadSignal<Rect>,
}

/// The nearest [`anchored`] ancestor's placement state.
pub fn use_anchor() -> Option<AnchorState> {
    inject::<AnchorState>()
}

/// Builder returned by [`anchored`].
pub struct AnchoredBuilder {
    target: AnchorTarget,
    placement: Prop<Placement>,
    offset: f32,
    flip: bool,
    shift: bool,
    padding: f32,
    boundary: Option<Prop<Rect>>,
    match_width: bool,
    style: Style,
}

/// Absolutely positioned content attached to `target`.
pub fn anchored(target: impl Into<AnchorTarget>) -> AnchoredBuilder {
    AnchoredBuilder {
        target: target.into(),
        placement: Prop::Static(Placement::Bottom),
        offset: 0.,
        flip: true,
        shift: true,
        padding: 8.,
        boundary: None,
        match_width: false,
        style: Style::new(),
    }
}

impl AnchoredBuilder {
    pub fn placement(mut self, placement: impl Into<Prop<Placement>>) -> Self {
        self.placement = placement.into();
        self
    }

    /// Gap between the anchor and the content.
    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    /// Move to the opposite side when the preferred side overflows (default on).
    pub fn flip(mut self, flip: bool) -> Self {
        self.flip = flip;
        self
    }

    /// Slide along the side to stay inside the boundary (default on).
    pub fn shift(mut self, shift: bool) -> Self {
        self.shift = shift;
        self
    }

    /// Minimum distance from the boundary edges (default 8).
    pub fn boundary_padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// The rectangle to stay inside. Defaults to the nearest layer host's
    /// bounds, or the window.
    pub fn boundary(mut self, boundary: impl Into<Prop<Rect>>) -> Self {
        self.boundary = Some(boundary.into());
        self
    }

    /// Make the content at least as wide as the anchor (for selects).
    pub fn match_width(mut self, match_width: bool) -> Self {
        self.match_width = match_width;
        self
    }
}

impl Styleable for AnchoredBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Where content of `size` goes for `placement`, before shifting.
fn place(anchor: Rect, size: Vector, placement: Placement, offset: f32) -> Vector {
    let (side, align) = placement.parts();
    let along = |start: f32, length: f32, content: f32| match align {
        Align::Start => start,
        Align::Center => start + (length - content) / 2.,
        Align::End => start + length - content,
    };
    match side {
        Side::Top => vec2(
            along(anchor.x, anchor.w, size.x),
            anchor.y - offset - size.y,
        ),
        Side::Bottom => vec2(along(anchor.x, anchor.w, size.x), anchor.bottom() + offset),
        Side::Left => vec2(
            anchor.x - offset - size.x,
            along(anchor.y, anchor.h, size.y),
        ),
        Side::Right => vec2(anchor.right() + offset, along(anchor.y, anchor.h, size.y)),
    }
}

fn overflow(position: Vector, size: Vector, bounds: Rect) -> f32 {
    let left = (bounds.x - position.x).max(0.);
    let top = (bounds.y - position.y).max(0.);
    let right = (position.x + size.x - bounds.right()).max(0.);
    let bottom = (position.y + size.y - bounds.bottom()).max(0.);
    left + top + right + bottom
}

/// Compute the final placement and top-left position.
pub(crate) fn resolve(
    anchor: Rect,
    size: Vector,
    preferred: Placement,
    offset: f32,
    flip: bool,
    shift: bool,
    bounds: Rect,
) -> (Placement, Vector) {
    let mut placement = preferred;
    let mut position = place(anchor, size, placement, offset);
    if flip && overflow(position, size, bounds) > 0. {
        let flipped = preferred.flipped();
        let alternative = place(anchor, size, flipped, offset);
        if overflow(alternative, size, bounds) < overflow(position, size, bounds) {
            placement = flipped;
            position = alternative;
        }
    }
    if shift {
        let clamp = |value: f32, min: f32, max: f32| {
            if max < min {
                min
            } else {
                value.clamp(min, max)
            }
        };
        if placement.is_vertical() {
            position.x = clamp(position.x, bounds.x, bounds.right() - size.x);
        } else {
            position.y = clamp(position.y, bounds.y, bounds.bottom() - size.y);
        }
    }
    (placement, position)
}

impl BuildNode for AnchoredBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            target,
            placement,
            offset,
            flip,
            shift,
            padding,
            boundary,
            match_width,
            style,
        } = self;
        let inset: Signal<Vector> = signal(Vector::ZERO);
        let min_width: Signal<f32> = signal(0.);
        let actual: Signal<Placement> = signal(placement.get_untracked());
        let arrow: Signal<f32> = signal(0.);
        let anchor_rect: Signal<Rect> = signal(Rect::default());
        provide(AnchorState {
            placement: *actual,
            arrow: *arrow,
            anchor: *anchor_rect,
        });
        ctx.push_style(style);
        let mut defaults = Style::new()
            .absolute()
            .inset_left(computed(move |_| Size::Px(inset.get().x)))
            .inset_top(computed(move |_| Size::Px(inset.get().y)));
        if match_width {
            defaults = defaults.min_width(computed(move |_| Size::Px(min_width.get())));
        }
        ctx.finish(defaults, move |style| {
            let layout = use_layout(style);
            let rect = layout.rect;
            use_background(style, rect);
            let viewport = crate::viewport::read();
            let host = crate::layer::LayerHost::current();
            // Layout hooks may update layout inputs; preparation repeats until
            // the position is stable.
            style.on_layout.append(move |_| {
                let Some(anchor) = target.rect() else {
                    return;
                };
                let own = rect.get_untracked();
                let bounds = boundary
                    .as_ref()
                    .and_then(|bounds| bounds.try_get_untracked())
                    .or_else(|| host.as_ref().and_then(|host| host.rect()))
                    .filter(|bounds| bounds.w > 0. && bounds.h > 0.)
                    .unwrap_or_else(|| {
                        let viewport = viewport.get_untracked();
                        Rect::new(0., 0., viewport.width, viewport.height)
                    })
                    .outset(-padding);
                let (resolved, position) = resolve(
                    anchor,
                    own.size(),
                    placement.get_untracked(),
                    offset,
                    flip,
                    shift,
                    bounds,
                );
                // Convert the desired window position into the parent's inset.
                let current = inset.get_untracked();
                let next = current + (position - own.min());
                if (next - current).length() > 0.01 {
                    inset.set(next);
                }
                if match_width && (min_width.get_untracked() - anchor.w).abs() > 0.01 {
                    min_width.set(anchor.w);
                }
                if actual.get_untracked() != resolved {
                    actual.set(resolved);
                }
                let arrow_offset = if resolved.is_vertical() {
                    (anchor.center().x - position.x).clamp(0., own.w)
                } else {
                    (anchor.center().y - position.y).clamp(0., own.h)
                };
                if (arrow.get_untracked() - arrow_offset).abs() > 0.01 {
                    arrow.set(arrow_offset);
                }
                if anchor_rect.get_untracked() != anchor {
                    anchor_rect.set(anchor);
                }
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Rect = Rect {
        x: 0.,
        y: 0.,
        w: 400.,
        h: 300.,
    };

    #[test]
    fn places_on_the_preferred_side_with_alignment() {
        let anchor = Rect::new(100., 100., 80., 20.);
        let size = vec2(120., 60.);
        let (p, pos) = resolve(anchor, size, Placement::BottomStart, 4., true, true, BOUNDS);
        assert_eq!(p, Placement::BottomStart);
        assert_eq!(pos, vec2(100., 124.));
        let (_, pos) = resolve(anchor, size, Placement::Top, 4., true, true, BOUNDS);
        assert_eq!(pos, vec2(80., 36.));
        let (_, pos) = resolve(anchor, size, Placement::RightEnd, 0., true, true, BOUNDS);
        assert_eq!(pos, vec2(180., 60.));
    }

    #[test]
    fn flips_when_the_preferred_side_overflows() {
        let anchor = Rect::new(100., 260., 80., 20.);
        let (p, pos) = resolve(
            anchor,
            vec2(120., 60.),
            Placement::Bottom,
            4.,
            true,
            true,
            BOUNDS,
        );
        assert_eq!(p, Placement::Top);
        assert_eq!(pos.y, 196.);
        let (p, _) = resolve(
            anchor,
            vec2(120., 60.),
            Placement::Bottom,
            4.,
            false,
            true,
            BOUNDS,
        );
        assert_eq!(p, Placement::Bottom, "flip disabled");
    }

    #[test]
    fn shifts_along_the_side_to_stay_inside() {
        let anchor = Rect::new(370., 100., 20., 20.);
        let (_, pos) = resolve(
            anchor,
            vec2(120., 60.),
            Placement::Bottom,
            0.,
            true,
            true,
            BOUNDS,
        );
        assert_eq!(pos.x, 280.);
        let (_, pos) = resolve(
            anchor,
            vec2(120., 60.),
            Placement::Bottom,
            0.,
            true,
            false,
            BOUNDS,
        );
        assert_eq!(pos.x, 320.);
    }
}
