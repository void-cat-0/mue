use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use taffy::Overflow;

use crate::{
    color::Color,
    event::pointer::{
        ButtonState, ClaimToken, PointerAction, PointerEvent, PointerId, PointerType,
    },
    gesture::Gesture,
    layout::use_layout,
    math::{Rect, Vector},
    node::{IntoChildren, IntoNode, Node, NodeInner, flexbox},
    paint::{use_background, use_paint},
    runtime::Runtime,
    style::{Style, Styleable, StyleableExt},
};

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

/// A clipped viewport. Give it a bounded height (and width for horizontal scrolling).
/// Content uses a column by default; customize it with `content_style`.
/// Touch drags claim the existing gesture arena only after the threshold; a
/// second contact cancels scrolling so pinch gestures can take over before claim.
/// Bounds are hard stops, without elastic overscroll. Wheel momentum is left to the OS.
pub fn scroll_area() -> ScrollAreaBuilder {
    ScrollAreaBuilder {
        style: Style::new(),
        content_style: Style::new(),
        options: ScrollOptions::default(),
        offset: None,
    }
}

pub struct ScrollAreaBuilder {
    style: Style,
    content_style: Style,
    options: ScrollOptions,
    offset: Option<Signal<Vector>>,
}

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
}

impl Styleable for ScrollAreaBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl IntoNode for ScrollAreaBuilder {
    fn into_node(self) -> Node {
        let Self {
            mut style,
            mut content_style,
            options,
            offset,
        } = self;
        assert!(options.wheel_speed.is_finite() && options.wheel_speed >= 0.);
        assert!(options.drag_threshold.is_finite() && options.drag_threshold >= 0.);
        assert!(options.friction.is_finite() && options.friction > 0.);
        assert!(options.max_velocity.is_finite() && options.max_velocity >= 0.);
        style.provide_defaults(
            Style::new()
                .flex_column()
                .min_width(0.)
                .min_height(0.)
                .overflow_x(Overflow::Scroll)
                .overflow_y(Overflow::Scroll),
        );
        Node::build_with_style(style, move |style| {
            let offset = offset.unwrap_or_else(|| signal(Vector::ZERO));
            let clip_rect = signal(Rect::default());
            let state = Rc::new(RefCell::new(ScrollState {
                offset,
                options,
                max: Vector::ZERO,
                viewport: Rect::default(),
                velocity: Vector::ZERO,
                drag: None,
                last_offset: offset.get(),
            }));
            let source = NodeInner::with_mut(|node| {
                std::mem::replace(&mut node.children, ().into_children())
            });
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
            let content_id = Runtime::with(|rt| rt.node(content.id).layout_id.unwrap());
            NodeInner::with_mut(|node| {
                node.children = content.into_children();
                node.clip = Some(*clip_rect);
                node.child_offset = Some(*offset);
            });
            let layout = use_layout(style);
            let rect = layout.rect;
            let layout_id = layout.layout_id;
            use_background(style, rect);
            style.on_render.append({
                let state = state.clone();
                move |_| {
                    let (viewport, extent) = Runtime::with_taffy_mut(|taffy| {
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
                    // External navigation cancels an old fling.
                    if state.offset.get() != state.last_offset {
                        state.velocity = Vector::ZERO;
                    }
                    state.set_offset(state.offset.get());
                    if state.drag.is_none() && options.momentum {
                        let dt = macroquad::time::get_frame_time().clamp(0., 0.05);
                        let decay = (-options.friction * dt).exp();
                        let delta = state.velocity * (1. - decay) / options.friction;
                        let moved = state.scroll_by(delta);
                        if (moved.x - delta.x).abs() > 0.01 {
                            state.velocity.x = 0.;
                        }
                        if (moved.y - delta.y).abs() > 0.01 {
                            state.velocity.y = 0.;
                        }
                        state.velocity *= decay;
                        if state.velocity.length() < 5. {
                            state.velocity = Vector::ZERO;
                        }
                    }
                    state.last_offset = state.offset.get();
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
            NodeInner::with_mut(|node| {
                node.hooks.after_children.append({
                    let state = state.clone();
                    move |_| {
                        let state = state.borrow();
                        if !options.scrollbars {
                            return;
                        }
                        paint.draw(|p| {
                            for axis in 0..2 {
                                if let Some((track, thumb)) = state.bar(axis) {
                                    p.fill_rounded_rect(
                                        track,
                                        3.,
                                        Color::from_rgba(128, 128, 128, 40),
                                    );
                                    p.fill_rounded_rect(
                                        thumb,
                                        3.,
                                        Color::from_rgba(128, 128, 128, 180),
                                    );
                                }
                            }
                        });
                    }
                });
                let id = Runtime::with(|rt| {
                    rt.gestures.borrow_mut().insert(Box::new(ScrollGesture {
                        state: state.clone(),
                    }))
                });
                node.gestures.push(id);
                // Scrollbar input should not activate the content beneath it.
                node.hit_test = Some(Rc::new(move |node, pos, result| {
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
                        let children = Runtime::with(|rt| *rt.node(node.id).children);
                        for child in children.get_clone().iter().rev() {
                            crate::event::hit_test::do_hit_test(*child, pos, result);
                        }
                    }
                    result.insert(node);
                }));
            });
        })
    }
}

struct ScrollState {
    offset: Signal<Vector>,
    options: ScrollOptions,
    max: Vector,
    viewport: Rect,
    velocity: Vector,
    drag: Option<Drag>,
    last_offset: Vector,
}

impl ScrollState {
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
                    if (!can_move && options.chain) || !token.claim() {
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
