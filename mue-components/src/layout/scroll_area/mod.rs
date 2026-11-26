//! The scroll area: a clipped viewport with touch, wheel, and mouse
//! scrolling, momentum, overlay scrollbars, pull to refresh, load more, and
//! focus reveal.

mod gesture;
mod refresh;
mod scrollbar;
mod state;

use mue_macroquad::node::flexbox::flexbox;
use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use taffy::Overflow;

use mue_macroquad::{
    layout::use_layout,
    math::{Rect, Vector},
    node::{BuildNode, IntoChildren, IntoNode, Node, NodeContext, NodeHandle, NodeRef},
    paint::{use_background, use_paint},
    style::{Style, Styleable, StyleableExt},
};

thread_local! {
    /// Mounted scroll areas, so only the innermost one reveals a field.
    static SCROLL_AREAS: std::cell::RefCell<Vec<NodeHandle>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Space kept between a revealed caret and the keyboard or viewport edge.
const REVEAL_MARGIN: f32 = 16.;

use gesture::ScrollGesture;
use refresh::draw_refresh_indicator;
use scrollbar::Scrollbars;
use state::{Action, ReachEnd, Refresh, ScrollState};

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
    /// Overlay scrollbars when content overflows: a thin thumb that shows
    /// while scrolling and fades out when idle, widens with a track when the
    /// mouse comes near the edge, and can be dragged (clicking the track
    /// jumps there).
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
        let mut hovered = None;
        let style = style.node_handle(node).with_hovered(|style, signal| {
            hovered = Some(signal);
            style
        });
        let hovered = hovered.expect("with_hovered runs its callback");
        ctx.push_style(style);
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
                bars: Scrollbars::default(),
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
                    {
                        let mut state = state.borrow_mut();
                        let layout = state.bar_layout();
                        state.bars.advance(&layout, *dt);
                    }
                    // Run callbacks after releasing the state.
                    let action = state.borrow_mut().advance(*dt);
                    if let Some(action) = action {
                        (action.borrow_mut())();
                    }
                }
            });
            style.on_hover_event.append({
                let state = state.clone();
                move |event| {
                    let mut state = state.borrow_mut();
                    let layout = state.bar_layout();
                    state.bars.hover(&layout, Some(event.position()));
                }
            });
            {
                let state = state.clone();
                watch(hovered, move |hovered| {
                    if !hovered.get() {
                        let mut state = state.borrow_mut();
                        let layout = state.bar_layout();
                        state.bars.hover(&layout, None);
                    }
                });
            }
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
                    if options.scrollbars {
                        state.bars.draw(&paint, &state.bar_layout());
                    }
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
                let on_bar = options.scrollbars && state.bars.blocks(&state.bar_layout(), pos);
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
