use mue_macroquad::{
    event::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId, PointerType},
    gesture::Gesture,
    math::Vector,
    node::{BuildNode, Node, NodeContext, flexbox::use_container},
    style::{Style, Styleable, StyleableExt},
};

/// A container that turns a rightward swipe from its left edge into a back
/// press ([`dispatch_back`](mue_macroquad::back::dispatch_back)), like iOS
/// navigation. Wrap a page (usually the app root) in it.
///
/// The swipe must start within [`BackSwipeBuilder::edge`] of the left edge,
/// move mostly horizontally, and travel [`BackSwipeBuilder::distance`]. It
/// claims the touch only once it is clearly a back swipe, so taps and
/// vertical scrolling near the edge still work.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _page = back_swipe().children(text("Swipe from the left edge to go back"));
/// ```
pub fn back_swipe() -> BackSwipeBuilder {
    BackSwipeBuilder {
        edge: 24.,
        distance: 80.,
        mouse: false,
        style: Style::new(),
    }
}

pub struct BackSwipeBuilder {
    edge: f32,
    distance: f32,
    mouse: bool,
    style: Style,
}

impl BackSwipeBuilder {
    /// Width of the edge zone where a swipe may start (logical pixels).
    pub fn edge(mut self, edge: f32) -> Self {
        self.edge = edge.max(1.);
        self
    }

    /// Horizontal travel that triggers back on release.
    pub fn distance(mut self, distance: f32) -> Self {
        self.distance = distance.max(1.);
        self
    }

    /// Also accept mouse drags (for testing on desktop).
    pub fn mouse(mut self, mouse: bool) -> Self {
        self.mouse = mouse;
        self
    }
}

impl Styleable for BackSwipeBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for BackSwipeBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            edge,
            distance,
            mouse,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new().flex_column(), move |style| {
            use_container(style);
            // The layout rect exists once the container is set up.
            let rect = mue_macroquad::node::use_rect();
            mue_macroquad::node::add_gesture(EdgeSwipe {
                rect,
                edge,
                distance,
                mouse,
                active: None,
            });
        })
    }
}

struct Swipe {
    pointer: PointerId,
    origin: Vector,
    claimed: bool,
}

struct EdgeSwipe {
    rect: mue_core::signal::ReadSignal<mue_macroquad::math::Rect>,
    edge: f32,
    distance: f32,
    mouse: bool,
    active: Option<Swipe>,
}

/// Movement before the swipe claims the pointer.
const CLAIM_SLOP: f32 = 12.;

impl Gesture for EdgeSwipe {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        use mue_core::signal::Access;
        match event.action() {
            PointerAction::Down => {
                let left = self.rect.get_untracked().x;
                let allowed = event.pointer_type() == PointerType::Touch || self.mouse;
                if allowed && self.active.is_none() && event.position().x - left <= self.edge {
                    self.active = Some(Swipe {
                        pointer: event.pointer_id(),
                        origin: event.position(),
                        claimed: false,
                    });
                } else {
                    token.dismiss();
                }
            }
            PointerAction::Move => {
                let Some(swipe) = self.active.as_mut() else {
                    return;
                };
                if swipe.pointer != event.pointer_id() || swipe.claimed {
                    return;
                }
                let delta = event.position() - swipe.origin;
                if delta.y.abs() > CLAIM_SLOP && delta.y.abs() > delta.x {
                    // Vertical: leave it to scrolling.
                    self.active = None;
                    token.dismiss();
                } else if delta.x > CLAIM_SLOP && delta.x > delta.y.abs() * 1.5 {
                    if token.claim() {
                        swipe.claimed = true;
                    } else {
                        self.active = None;
                        token.dismiss();
                    }
                }
            }
            PointerAction::Up => {
                if let Some(swipe) = self.active.take()
                    && swipe.claimed
                    && event.position().x - swipe.origin.x >= self.distance
                {
                    mue_macroquad::back::dispatch_back();
                }
                token.dismiss();
            }
            PointerAction::Cancel => {
                self.active = None;
            }
        }
    }

    fn on_rejected(&mut self, pointer: PointerId) {
        if self
            .active
            .as_ref()
            .is_some_and(|swipe| swipe.pointer == pointer)
        {
            self.active = None;
        }
    }
}
