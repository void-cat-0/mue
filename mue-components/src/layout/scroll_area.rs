use mue_macroquad::node::flexbox::flexbox;
use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use taffy::Overflow;

use mue_macroquad::{
    input::gesture::Gesture,
    input::pointer::{
        ButtonState, ClaimToken, PointerAction, PointerEvent, PointerId, PointerType,
    },
    layout::use_layout,
    math::{Rect, Vector},
    node::{BuildNode, IntoChildren, IntoNode, Node, NodeContext, NodeHandle, NodeRef},
    paint::color::Color,
    paint::path::{FillRule, PathData},
    paint::{use_background, use_paint},
    style::{Style, Styleable, StyleableExt},
};

thread_local! {
    /// Mounted scroll areas, so only the innermost one reveals a field.
    static SCROLL_AREAS: std::cell::RefCell<Vec<NodeHandle>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Space kept between a revealed caret and the keyboard or viewport edge.
const REVEAL_MARGIN: f32 = 16.;

type Action = Rc<RefCell<dyn FnMut()>>;

/// Pull-to-refresh configuration.
#[derive(Clone)]
struct Refresh {
    refreshing: Signal<bool>,
    action: Action,
    /// Pull distance that triggers a refresh on release.
    threshold: f32,
    /// Height the content stays pulled down while refreshing.
    hold: f32,
}

/// Load-more configuration and its edge-trigger state.
struct ReachEnd {
    threshold: f32,
    action: Action,
    armed: bool,
    last_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxis {
    fn mask(self, value: Vector) -> Vector {
        match self {
            Self::Vertical => Vector::new(0., value.y),
            Self::Horizontal => Vector::new(value.x, 0.),
            Self::Both => value,
        }
    }
}

/// Input and physics options, in logical pixels and seconds.
#[derive(Clone, Copy, Debug)]
pub struct ScrollOptions {
    pub axis: ScrollAxis,
    pub touch: bool,
    /// Drag content with the primary mouse button. Off by default.
    pub mouse_drag: bool,
    pub wheel: bool,
    /// Logical pixels per Macroquad wheel unit. Trackpads share this input stream.
    pub wheel_speed: f32,
    /// Distance before a drag competes with child taps.
    pub drag_threshold: f32,
    /// Lock a two-axis touch drag to its initial dominant axis.
    pub axis_lock: bool,
    pub momentum: bool,
    /// Exponential velocity decay per second; larger values stop sooner.
    pub friction: f32,
    /// Release velocity limit, in pixels per second.
    pub max_velocity: f32,
    /// Allow an ancestor to scroll when this area cannot move in the requested direction.
    pub chain: bool,
    /// Show and enable draggable overlay scrollbars when content overflows.
    pub scrollbars: bool,
}

impl Default for ScrollOptions {
    fn default() -> Self {
        Self {
            axis: ScrollAxis::Vertical,
            touch: true,
            mouse_drag: false,
            wheel: true,
            wheel_speed: 40.,
            drag_threshold: 8.,
            axis_lock: true,
            momentum: true,
            friction: 8.,
            max_velocity: 6000.,
            chain: true,
            scrollbars: true,
        }
    }
}

/// A clipped, interactive scrolling viewport. Give it a bounded height (and a
/// bounded width for horizontal scrolling). Content uses a column by default;
/// customize that inner column with [`ScrollAreaBuilder::content_style`].
///
/// Touch drags claim the gesture arena only after the threshold. A second contact
/// cancels scrolling so pinch gestures can take over before the scroll claims the
/// gesture. Bounds are hard stops without elastic overscroll; wheel momentum is
/// left to the operating system. See [`ScrollOptions`] for axis, chaining, drag,
/// momentum, and scrollbar settings.
///
/// When a focused text field inside the area gains focus, its caret moves,
/// or a soft keyboard appears, the area scrolls so the caret (or the focused
/// node) stays visible above the keyboard. Only the innermost scroll area
/// containing the field does this. Put forms in a scroll area so they can be
/// revealed on platforms whose keyboard covers the window (iOS, web).
pub fn scroll_area() -> ScrollAreaBuilder {
    ScrollAreaBuilder {
        handle: None,
        style: Style::new(),
        content_style: Style::new(),
        options: ScrollOptions::default(),
        offset: None,
        refresh: None,
        reach_end: None,
    }
}

/// Imperative scrolling (Vue's template ref with `defineExpose`): bind with
/// [`ScrollAreaBuilder::handle`], then scroll from code.
///
/// ```
/// use mue_components::prelude::*;
/// use mue_macroquad::math::Vector;
///
/// # let owner = mue_core::scope::OwnedScope::new();
/// # owner.run(|| {
/// let list = ScrollAreaHandle::new();
/// let _area = scroll_area().handle(list.clone());
/// let _to_top = button("Top").on_activate(move || list.scroll_to_top(true));
/// # });
/// ```
#[derive(Clone, Default)]
pub struct ScrollAreaHandle(Rc<RefCell<std::rc::Weak<RefCell<ScrollState>>>>);

impl ScrollAreaHandle {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> Option<Rc<RefCell<ScrollState>>> {
        self.0.borrow().upgrade()
    }

    /// The current offset (zero when unmounted).
    pub fn offset(&self) -> Vector {
        self.state()
            .map_or(Vector::ZERO, |state| state.borrow().offset.get_untracked())
    }

    /// The largest offset on each axis.
    pub fn max_offset(&self) -> Vector {
        self.state()
            .map_or(Vector::ZERO, |state| state.borrow().max)
    }

    /// Scroll to `offset` (clamped), easing over 0.3 s when `animated`.
    pub fn scroll_to(&self, offset: Vector, animated: bool) {
        if let Some(state) = self.state() {
            state.borrow_mut().scroll_to(offset, animated);
        }
    }

    pub fn scroll_by(&self, delta: Vector, animated: bool) {
        self.scroll_to(self.offset() + delta, animated);
    }

    pub fn scroll_to_top(&self, animated: bool) {
        self.scroll_to(Vector::new(self.offset().x, 0.), animated);
    }

    pub fn scroll_to_end(&self, animated: bool) {
        let max = self.max_offset();
        self.scroll_to(Vector::new(self.offset().x, max.y), animated);
    }

    /// Scroll the least amount that makes `rect` (window coordinates, such as
    /// a node's [`rect`](mue_macroquad::node::NodeRef::rect)) visible.
    pub fn scroll_into_view(&self, rect: Rect, animated: bool) {
        let Some(state) = self.state() else {
            return;
        };
        let (viewport, offset) = {
            let state = state.borrow();
            (state.viewport, state.offset.get_untracked())
        };
        let mut target = offset;
        if rect.bottom() > viewport.bottom() {
            target.y += rect.bottom() - viewport.bottom();
        }
        if rect.y < viewport.y {
            target.y -= viewport.y - rect.y;
        }
        if rect.right() > viewport.right() {
            target.x += rect.right() - viewport.right();
        }
        if rect.x < viewport.x {
            target.x -= viewport.x - rect.x;
        }
        self.scroll_to(target, animated);
    }
}

pub struct ScrollAreaBuilder {
    handle: Option<ScrollAreaHandle>,
    style: Style,
    content_style: Style,
    options: ScrollOptions,
    offset: Option<Signal<Vector>>,
    refresh: Option<Refresh>,
    reach_end: Option<(f32, Action)>,
}

pub type ScrollArea = ScrollAreaBuilder;

impl ScrollAreaBuilder {
    pub fn axis(mut self, axis: ScrollAxis) -> Self {
        self.options.axis = axis;
        self
    }

    pub fn options(mut self, options: ScrollOptions) -> Self {
        self.options = options;
        self
    }

    /// Share a writable offset to observe or programmatically scroll the viewport.
    /// The viewport clamps it after layout, including when content shrinks.
    pub fn offset(mut self, offset: Signal<Vector>) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn content_style(mut self, style: Style) -> Self {
        self.content_style = style;
        self
    }

    /// Bind a handle for scrolling from code.
    pub fn handle(mut self, handle: ScrollAreaHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Pull to refresh (MB7): dragging down past the top shows an indicator;
    /// releasing past 64 px sets `refreshing` and calls `action`. The content
    /// stays pulled down with a spinning indicator until the caller sets
    /// `refreshing` back to false. Touch only unless `mouse_drag` is enabled.
    pub fn on_refresh(mut self, refreshing: Signal<bool>, action: impl FnMut() + 'static) -> Self {
        self.refresh = Some(Refresh {
            refreshing,
            action: Rc::new(RefCell::new(action)),
            threshold: 64.,
            hold: 48.,
        });
        self
    }

    /// Load more (MB7): call `action` when the end of the content comes within
    /// `threshold` logical pixels of the viewport. It fires once per approach,
    /// and again after the content grows or the user scrolls away; content
    /// shorter than the viewport counts as reaching the end, so a list keeps
    /// loading until it fills the area.
    pub fn on_reach_end(mut self, threshold: f32, action: impl FnMut() + 'static) -> Self {
        self.reach_end = Some((threshold.max(0.), Rc::new(RefCell::new(action))));
        self
    }
}

impl Styleable for ScrollAreaBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ScrollAreaBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            handle,
            style,
            mut content_style,
            options,
            offset,
            refresh,
            reach_end,
        } = self;
        assert!(options.wheel_speed.is_finite() && options.wheel_speed >= 0.);
        assert!(options.drag_threshold.is_finite() && options.drag_threshold >= 0.);
        assert!(options.friction.is_finite() && options.friction > 0.);
        assert!(options.max_velocity.is_finite() && options.max_velocity >= 0.);
        let node = NodeHandle::new();
        ctx.push_style(style.node_handle(node));
        SCROLL_AREAS.with(|areas| {
            let mut areas = areas.borrow_mut();
            areas.retain(|area| area.get_untracked().is_some());
            areas.push(node);
        });
        let defaults = Style::new()
            .flex_column()
            .min_width(0.)
            .min_height(0.)
            .overflow_x(Overflow::Scroll)
            .overflow_y(Overflow::Scroll);
        ctx.finish(defaults, move |style| {
            let offset = offset.unwrap_or_else(|| signal(Vector::ZERO));
            let clip_rect = signal(Rect::default());
            let pull = signal(0_f32);
            let state = Rc::new(RefCell::new(ScrollState {
                offset,
                options,
                max: Vector::ZERO,
                viewport: Rect::default(),
                velocity: Vector::ZERO,
                drag: None,
                last_offset: offset.get(),
                pull,
                refresh: refresh.clone(),
                end: reach_end.map(|(threshold, action)| ReachEnd {
                    threshold,
                    action,
                    armed: true,
                    last_max: 0.,
                }),
                animation: None,
            }));
            if let Some(handle) = &handle {
                *handle.0.borrow_mut() = Rc::downgrade(&state);
            }
            let source = mue_macroquad::node::take_children();
            content_style.provide_defaults(Style::new().flex_none().min_width(0.).min_height(0.));
            if options.axis == ScrollAxis::Vertical {
                content_style.provide_defaults(Style::new().w_full().flex_column());
            } else if options.axis == ScrollAxis::Horizontal {
                content_style.provide_defaults(Style::new().h_full().flex_row());
            } else {
                content_style.provide_defaults(Style::new().flex_column());
            }
            let mut content = flexbox().children(source);
            content.style_mut().provide_defaults(content_style);
            let content = content.into_node();
            let content_id = content.layout_id().unwrap();
            mue_macroquad::node::set_children(content.into_children());
            mue_macroquad::node::set_clip(*clip_rect);
            // Pulling to refresh moves the content down past the top.
            mue_macroquad::node::set_child_offset(computed(move |_| {
                offset.get() - Vector::new(0., pull.get())
            }));
            let layout = use_layout(style);
            let rect = layout.rect;
            let layout_id = layout.layout_id;
            use_background(style, rect);
            style.on_layout.append({
                let state = state.clone();
                move |_| {
                    let (viewport, extent) = mue_macroquad::layout::with_taffy(|taffy| {
                        let outer = taffy.layout(layout_id).unwrap();
                        let content = taffy.layout(content_id).unwrap();
                        let rect = rect.get();
                        let viewport = Rect::new(
                            rect.x + outer.border.left + outer.padding.left,
                            rect.y + outer.border.top + outer.padding.top,
                            (rect.w
                                - outer.border.left
                                - outer.border.right
                                - outer.padding.left
                                - outer.padding.right)
                                .max(0.),
                            (rect.h
                                - outer.border.top
                                - outer.border.bottom
                                - outer.padding.top
                                - outer.padding.bottom)
                                .max(0.),
                        );
                        let extent = Vector::new(
                            content.size.width.max(content.content_size.width),
                            content.size.height.max(content.content_size.height),
                        );
                        (viewport, extent)
                    });
                    let mut state = state.borrow_mut();
                    state.viewport = viewport;
                    clip_rect.set(viewport);
                    state.max = options
                        .axis
                        .mask((extent - viewport.size()).max(Vector::ZERO));
                    state.correct_offset();
                }
            });
            // Reveal requests: focus changes, keyboard inset changes, and caret
            // moves within the focused field (not caret moves from scrolling).
            let reveal = std::rc::Rc::new(std::cell::Cell::new(false));
            {
                let reveal = reveal.clone();
                let focused = mue_macroquad::input::focus::focused_signal();
                let inset = mue_macroquad::input::text_input::keyboard_inset();
                let caret = mue_macroquad::input::text_input::caret_rect();
                let relative = computed(move |_| {
                    let caret = caret.get()?;
                    let origin = focused.get()?.rect()?.get_untracked();
                    Some(((caret.x - origin.x).round(), (caret.y - origin.y).round()))
                });
                watch_effect(move || {
                    let _ = (focused.get(), inset.get(), relative.get());
                    reveal.set(true);
                });
            }
            style.on_update.append({
                let state = state.clone();
                let reveal = reveal.clone();
                move |dt| {
                    if reveal.replace(false) {
                        reveal_focused(node, &mut state.borrow_mut());
                    }
                    // Run callbacks after releasing the state.
                    let action = state.borrow_mut().advance(*dt);
                    if let Some(action) = action {
                        (action.borrow_mut())();
                    }
                }
            });
            style.on_wheel_event.append({
                let state = state.clone();
                move |event| {
                    if !options.wheel {
                        return;
                    }
                    let raw = event.remaining_delta();
                    let shift = macroquad::input::is_key_down(macroquad::input::KeyCode::LeftShift)
                        || macroquad::input::is_key_down(macroquad::input::KeyCode::RightShift);
                    let swap = options.axis == ScrollAxis::Horizontal
                        || (shift && options.axis == ScrollAxis::Both);
                    let delta = if swap && raw.x == 0. {
                        Vector::new(raw.y, 0.)
                    } else {
                        raw
                    };
                    let mut state = state.borrow_mut();
                    state.velocity = Vector::ZERO;
                    let moved = state.scroll_by(-options.axis.mask(delta) * options.wheel_speed);
                    if options.wheel_speed > 0. {
                        let mut consumed = -moved / options.wheel_speed;
                        if swap && raw.x == 0. {
                            consumed = Vector::new(0., consumed.x);
                        }
                        event.consume_delta(if options.chain { consumed } else { raw });
                    }
                }
            });
            let paint = use_paint(style);
            mue_macroquad::node::on_after_children({
                let state = state.clone();
                move |_| {
                    let state = state.borrow();
                    if let Some(refresh) = &state.refresh {
                        draw_refresh_indicator(&paint, &state, refresh);
                    }
                    if !options.scrollbars {
                        return;
                    }
                    paint.draw(|p| {
                        for axis in 0..2 {
                            if let Some((track, thumb)) = state.bar(axis) {
                                p.fill(
                                    &PathData::rounded_rect(track, 3.),
                                    FillRule::NonZero,
                                    Color::from_rgba(128, 128, 128, 40),
                                );
                                p.fill(
                                    &PathData::rounded_rect(thumb, 3.),
                                    FillRule::NonZero,
                                    Color::from_rgba(128, 128, 128, 180),
                                );
                            }
                        }
                    });
                }
            });
            mue_macroquad::node::add_gesture(ScrollGesture {
                state: state.clone(),
            });
            // Scrollbar input should not activate the content beneath it.
            mue_macroquad::input::hit_test::set_hit_test_fn(Rc::new(move |node, pos, result| {
                let state = state.borrow();
                if !state.viewport.contains(&pos) {
                    return;
                }
                let on_bar = (0..2).any(|axis| {
                    state
                        .bar(axis)
                        .is_some_and(|(track, _)| track.contains(&pos))
                });
                drop(state);
                if !on_bar {
                    let children = node.children();
                    for child in children.get_clone().iter().rev() {
                        mue_macroquad::input::hit_test::hit_test_into(*child, pos, result);
                    }
                }
                result.hit_self(node);
            }));
        })
    }
}

/// Scroll so the focused node's caret (or the node) is visible above the
/// soft keyboard, if this is the innermost scroll area containing it.
fn reveal_focused(area: NodeHandle, state: &mut ScrollState) {
    let (Some(area), Some(focused)) =
        (area.get_untracked(), mue_macroquad::input::focus::focused())
    else {
        return;
    };
    if focused == area || !area.contains(focused) {
        return;
    }
    let inner = SCROLL_AREAS.with(|areas| {
        areas
            .borrow()
            .iter()
            .filter_map(NodeHandle::get_untracked)
            .any(|other: NodeRef| other != area && area.contains(other) && other.contains(focused))
    });
    if inner {
        return;
    }
    let target = mue_macroquad::input::text_input::caret_rect()
        .get_untracked()
        .or_else(|| focused.rect().map(|rect| rect.get_untracked()));
    let Some(target) = target else {
        return;
    };
    let window = mue_macroquad::app::viewport::use_viewport().get_untracked();
    let inset = mue_macroquad::input::text_input::keyboard_inset().get_untracked();
    let viewport = state.viewport;
    let bottom = viewport.bottom().min(window.height - inset) - REVEAL_MARGIN;
    let top = viewport.y + REVEAL_MARGIN.min(viewport.h / 4.);
    let mut delta = Vector::ZERO;
    if target.bottom() > bottom {
        delta.y = target.bottom() - bottom;
    } else if target.y < top {
        delta.y = target.y - top;
    }
    if state.options.axis != ScrollAxis::Vertical {
        let right = viewport.right() - REVEAL_MARGIN;
        if target.right() > right {
            delta.x = target.right() - right;
        } else if target.x < viewport.x {
            delta.x = target.x - viewport.x;
        }
    }
    if delta != Vector::ZERO {
        state.velocity = Vector::ZERO;
        state.scroll_by(delta);
    }
}

/// Draw the pull-to-refresh indicator: an arc that grows with the pull and
/// spins while refreshing.
fn draw_refresh_indicator(
    paint: &mue_macroquad::paint::Paint,
    state: &ScrollState,
    refresh: &Refresh,
) {
    use std::f32::consts::PI;
    let pull = state.pull.get_untracked();
    let refreshing = refresh.refreshing.get_untracked();
    if pull < 0.5 && !refreshing {
        return;
    }
    let radius = 9.;
    let center = Vector::new(
        state.viewport.center().x,
        state.viewport.y + (pull / 2.).max(radius + 6.).min(refresh.hold / 2. + 8.),
    );
    let (start, sweep) = if refreshing {
        let spin = (mue_macroquad::app::runtime::get_time() * 5.) % (2. * PI);
        (spin, 1.5 * PI)
    } else {
        let progress = (pull / refresh.threshold).clamp(0., 1.);
        (-PI / 2., progress * 1.75 * PI)
    };
    let end = start + sweep.max(0.05);
    let point = |angle: f32| center + Vector::new(angle.cos(), angle.sin()) * radius;
    let (a, b) = (point(start), point(end));
    let arc = format!(
        "M{} {} A{radius} {radius} 0 {} 1 {} {}",
        a.x,
        a.y,
        u8::from(sweep > PI),
        b.x,
        b.y
    );
    let Ok(arc) = PathData::parse(&arc) else {
        return;
    };
    let alpha = if refreshing {
        1.
    } else {
        (pull / refresh.threshold).clamp(0.3, 1.)
    };
    paint.draw(|p| {
        p.fill(
            &PathData::circle(center, radius + 7.),
            FillRule::NonZero,
            Color::new(0.16, 0.19, 0.27, alpha),
        );
        p.stroke(
            &arc,
            &mue_macroquad::paint::path::StrokeStyle::round(2.5),
            Color::new(0.55, 0.72, 1., alpha),
        );
    });
}

struct ScrollState {
    offset: Signal<Vector>,
    options: ScrollOptions,
    max: Vector,
    viewport: Rect,
    velocity: Vector,
    drag: Option<Drag>,
    last_offset: Vector,
    /// Pull-to-refresh distance past the top.
    pull: Signal<f32>,
    refresh: Option<Refresh>,
    end: Option<ReachEnd>,
    /// A programmatic scroll in progress: from, to, elapsed fraction.
    animation: Option<(Vector, Vector, f32)>,
}

impl ScrollState {
    fn correct_offset(&mut self) {
        // External navigation cancels an old fling. This also runs before the
        // time step so an observer of prepared geometry can stop momentum.
        if self.offset.get() != self.last_offset {
            self.velocity = Vector::ZERO;
        }
        self.set_offset(self.offset.get());
        self.last_offset = self.offset.get();
    }

    /// Advance momentum and pull springs; returns a load-more action to run.
    fn advance(&mut self, dt: f32) -> Option<Action> {
        self.advance_pull(dt);
        self.advance_motion(dt);
        self.check_end()
    }

    fn advance_pull(&mut self, dt: f32) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        if self.drag.is_some() {
            return;
        }
        let target = if refresh.refreshing.get_untracked() {
            refresh.hold
        } else {
            0.
        };
        let pull = self.pull.get_untracked();
        if (pull - target).abs() < 0.5 {
            if pull != target {
                self.pull.set(target);
            }
            return;
        }
        let rate = 1. - (-14. * dt.clamp(0., 0.05)).exp();
        self.pull.set(pull + (target - pull) * rate);
    }

    fn check_end(&mut self) -> Option<Action> {
        let offset = self.offset.get_untracked();
        let end = self.end.as_mut()?;
        if self.max.y > end.last_max + 0.5 {
            // New content: arm again.
            end.armed = true;
        }
        end.last_max = self.max.y;
        let near = self.max.y - offset.y <= end.threshold;
        if !near {
            end.armed = true;
            None
        } else if end.armed {
            end.armed = false;
            Some(end.action.clone())
        } else {
            None
        }
    }

    fn scroll_to(&mut self, target: Vector, animated: bool) {
        let target = self.options.axis.mask(target).clamp(Vector::ZERO, self.max);
        self.velocity = Vector::ZERO;
        if animated {
            self.animation = Some((self.offset.get_untracked(), target, 0.));
        } else {
            self.animation = None;
            self.set_offset(target);
            self.last_offset = self.offset.get_untracked();
        }
    }

    fn advance_motion(&mut self, dt: f32) {
        if let Some((from, to, t)) = self.animation {
            if self.drag.is_some() {
                // A touch takes over from the animation.
                self.animation = None;
            } else {
                let t = (t + dt.clamp(0., 0.05) / 0.3).min(1.);
                let eased = 1. - (1. - t) * (1. - t) * (1. - t);
                self.set_offset(from + (to - from) * eased);
                self.last_offset = self.offset.get_untracked();
                self.animation = (t < 1.).then_some((from, to, t));
                return;
            }
        }
        self.correct_offset();
        if self.drag.is_none() && self.options.momentum {
            let dt = dt.clamp(0., 0.05);
            let decay = (-self.options.friction * dt).exp();
            let delta = self.velocity * (1. - decay) / self.options.friction;
            let moved = self.scroll_by(delta);
            if (moved.x - delta.x).abs() > 0.01 {
                self.velocity.x = 0.;
            }
            if (moved.y - delta.y).abs() > 0.01 {
                self.velocity.y = 0.;
            }
            self.velocity *= decay;
            if self.velocity.length() < 5. {
                self.velocity = Vector::ZERO;
            }
        }
    }

    fn set_offset(&self, value: Vector) {
        let value = Vector::new(
            if value.x.is_finite() { value.x } else { 0. },
            if value.y.is_finite() { value.y } else { 0. },
        );
        self.offset
            .set(self.options.axis.mask(value).clamp(Vector::ZERO, self.max));
    }

    fn scroll_by(&mut self, delta: Vector) -> Vector {
        let old = self.offset.get();
        self.set_offset(old + delta);
        self.last_offset = self.offset.get();
        self.last_offset - old
    }

    fn bar(&self, axis: usize) -> Option<(Rect, Rect)> {
        if !self.options.scrollbars || self.max[axis] <= 0. {
            return None;
        }
        let v = self.viewport;
        let length = (v.size()[axis] - 12.).max(0.);
        if length <= 0. {
            return None;
        }
        let size = (length * v.size()[axis] / (v.size()[axis] + self.max[axis]))
            .max(24.)
            .min(length);
        let start = (length - size) * self.offset.get()[axis] / self.max[axis];
        Some(if axis == 0 {
            (
                Rect::new(v.x + 3., v.bottom() - 9., length, 6.),
                Rect::new(v.x + 3. + start, v.bottom() - 9., size, 6.),
            )
        } else {
            (
                Rect::new(v.right() - 9., v.y + 3., 6., length),
                Rect::new(v.right() - 9., v.y + 3. + start, 6., size),
            )
        })
    }
}

struct Drag {
    id: PointerId,
    origin: Vector,
    previous: Vector,
    time: f32,
    claimed: bool,
    axis: ScrollAxis,
    bar: Option<usize>,
}

struct ScrollGesture {
    state: Rc<RefCell<ScrollState>>,
}

impl Gesture for ScrollGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        let mut state = self.state.borrow_mut();
        let options = state.options;
        if let Some(drag) = &state.drag {
            if drag.id != event.pointer_id() {
                // A second finger stops the fling/drag; it is not treated as another pan.
                state.drag = None;
                state.velocity = Vector::ZERO;
                token.dismiss();
                return;
            }
        } else if event.action() == PointerAction::Down {
            let bar = (0..2).find(|&axis| {
                state
                    .bar(axis)
                    .is_some_and(|(track, _)| track.contains(&event.position()))
            });
            let allowed = match event.pointer_type() {
                PointerType::Touch => options.touch,
                PointerType::Mouse => {
                    event.button_state().contains(ButtonState::PRIMARY)
                        && (options.mouse_drag || bar.is_some())
                }
            };
            if !allowed {
                token.dismiss();
                return;
            }
            state.velocity = Vector::ZERO;
            let claimed = bar.is_some() && token.claim();
            if bar.is_some() && !claimed {
                token.dismiss();
                return;
            }
            if let Some(axis) = bar {
                let (track, thumb) = state.bar(axis).unwrap();
                if !thumb.contains(&event.position()) {
                    let mut offset = state.offset.get();
                    let fraction =
                        (event.position()[axis] - track.min()[axis] - thumb.size()[axis] / 2.)
                            / (track.size()[axis] - thumb.size()[axis]).max(1.);
                    offset[axis] = fraction * state.max[axis];
                    state.set_offset(offset);
                }
            }
            state.drag = Some(Drag {
                id: event.pointer_id(),
                origin: event.position(),
                previous: event.position(),
                time: event.time(),
                claimed,
                axis: options.axis,
                bar,
            });
            return;
        } else {
            token.dismiss();
            return;
        }

        let mut drag = state.drag.take().unwrap();
        match event.action() {
            PointerAction::Move => {
                let total = event.position() - drag.origin;
                if !drag.claimed {
                    if total.length() <= options.drag_threshold {
                        state.drag = Some(drag);
                        return;
                    }
                    let dominant = if total.x.abs() > total.y.abs() {
                        ScrollAxis::Horizontal
                    } else {
                        ScrollAxis::Vertical
                    };
                    if options.axis != ScrollAxis::Both && options.axis != dominant {
                        token.dismiss();
                        return;
                    }
                    if options.axis_lock {
                        drag.axis = dominant;
                    }
                    let delta = -drag.axis.mask(total);
                    let current = state.offset.get();
                    let can_move = (current + delta).clamp(Vector::ZERO, state.max) != current;
                    let can_pull = state.refresh.is_some()
                        && drag.axis != ScrollAxis::Horizontal
                        && current.y <= 0.
                        && total.y > 0.;
                    if (!can_move && !can_pull && options.chain) || !token.claim() {
                        token.dismiss();
                        return;
                    }
                    drag.claimed = true;
                }
                let delta = event.position() - drag.previous;
                let dt = (event.time() - drag.time).clamp(0.001, 0.1);
                if let Some(axis) = drag.bar {
                    if let Some((track, thumb)) = state.bar(axis) {
                        let mut movement = Vector::ZERO;
                        movement[axis] = delta[axis] * state.max[axis]
                            / (track.size()[axis] - thumb.size()[axis]).max(1.);
                        state.scroll_by(movement);
                    }
                } else if state.refresh.is_some()
                    && drag.axis != ScrollAxis::Horizontal
                    && (state.pull.get_untracked() > 0.
                        || (state.offset.get().y <= 0. && delta.y > 0.))
                {
                    // Past the top: pull with resistance instead of scrolling.
                    let pull = (state.pull.get_untracked() + delta.y * 0.5).max(0.);
                    state.pull.set(pull);
                    state.velocity = Vector::ZERO;
                } else {
                    let moved = state.scroll_by(-drag.axis.mask(delta));
                    state.velocity = (state.velocity * 0.25 + moved / dt * 0.75)
                        .clamp_length_max(options.max_velocity);
                }
                drag.previous = event.position();
                drag.time = event.time();
                state.drag = Some(drag);
            }
            PointerAction::Up | PointerAction::Cancel => {
                let trigger = event.action() == PointerAction::Up
                    && state.refresh.as_ref().is_some_and(|refresh| {
                        !refresh.refreshing.get_untracked()
                            && state.pull.get_untracked() >= refresh.threshold
                    });
                if trigger && let Some(refresh) = state.refresh.clone() {
                    refresh.refreshing.set(true);
                    drop(state);
                    (refresh.action.borrow_mut())();
                    token.dismiss();
                    return;
                }
                if event.action() == PointerAction::Cancel
                    || !options.momentum
                    || drag.bar.is_some()
                    || event.time() - drag.time > 0.1
                {
                    state.velocity = Vector::ZERO;
                }
                token.dismiss();
            }
            PointerAction::Down => state.drag = Some(drag),
        }
    }

    fn on_rejected(&mut self, pointer: PointerId) {
        let mut state = self.state.borrow_mut();
        if state.drag.as_ref().is_some_and(|drag| drag.id == pointer) {
            state.drag = None;
            state.velocity = Vector::ZERO;
        }
    }
}
