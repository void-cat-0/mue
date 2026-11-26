use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

use bitflags::bitflags;
use glam::vec2;
use indexmap::IndexSet;
use miniquad::{EventHandler, MouseButton};

use slotmap::Key;

use crate::{
    event::{
        hit_test::{hit_test, hit_test_with_cache, is_interactive},
        hover::HoverState,
    },
    gesture::GestureId,
    input::{InputBatch, InputEvent, InputSample, InputSubscriber, valid_dpi_scale},
    math::{Rect, Vector},
    node::NodeRef,
    runtime::Runtime,
    scene::{RootInput, SceneRoot},
};

fn screen_scale() -> f32 {
    valid_dpi_scale(macroquad::window::screen_dpi_scale()).unwrap_or(1.)
}

fn screen_bounds() -> Rect {
    Rect::new(
        0.,
        0.,
        macroquad::window::screen_width(),
        macroquad::window::screen_height(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerAction {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerType {
    Touch,
    Mouse,
}

/// Identifies a pointer (touch finger or mouse button).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PointerId(u64);

impl PointerId {
    pub const MOUSE: PointerId = PointerId(u64::MAX - 1);

    pub fn from_touch_id(id: u64) -> Self {
        Self(id)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ButtonState: u32 {
        const PRIMARY = 0x01;
        const SECONDARY = 0x02;
        const TERTIARY = 0x04;
        const UNKNOWN = 0x08;
    }
}

impl ButtonState {
    pub fn from_mouse_button(button: MouseButton) -> Self {
        match button {
            MouseButton::Left => ButtonState::PRIMARY,
            MouseButton::Right => ButtonState::SECONDARY,
            MouseButton::Middle => ButtonState::TERTIARY,
            MouseButton::Unknown => ButtonState::UNKNOWN,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PointerEvent {
    pointer_id: PointerId,
    pointer_type: PointerType,
    button_state: ButtonState,

    is_hover: bool,
    action: PointerAction,
    start_position: Vector,
    position: Vector,
    delta: Vector,
    time: f32,
}

impl PointerEvent {
    pub(crate) fn new(pointer_id: PointerId, pointer_type: PointerType, start_pos: Vector) -> Self {
        Self {
            pointer_id,
            pointer_type,
            button_state: ButtonState::empty(),

            is_hover: false,
            action: PointerAction::Down,
            start_position: start_pos,
            position: start_pos,
            delta: vec2(0., 0.),
            time: crate::runtime::get_time(),
        }
    }

    fn update(&mut self, action: PointerAction, current_pos: Vector) {
        self.delta = current_pos - self.position;
        self.position = current_pos;
        self.action = action;
        self.time = crate::runtime::get_time();
    }

    pub fn pointer_id(&self) -> PointerId {
        self.pointer_id
    }

    pub fn pointer_type(&self) -> PointerType {
        self.pointer_type
    }

    pub fn button_state(&self) -> ButtonState {
        self.button_state
    }

    pub fn action(&self) -> PointerAction {
        self.action
    }

    pub fn start_position(&self) -> Vector {
        self.start_position
    }

    pub fn position(&self) -> Vector {
        self.position
    }

    pub fn delta(&self) -> Vector {
        self.delta
    }

    pub fn time(&self) -> f32 {
        self.time
    }
}

#[derive(Clone)]
pub struct ClaimToken {
    pointer_id: PointerId,
    gesture_id: GestureId,
    state: Rc<PointerGestureState>,
}

impl fmt::Debug for ClaimToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimToken")
            .field("pointer_id", &self.pointer_id)
            .field("gesture_id", &self.gesture_id)
            .finish()
    }
}

impl ClaimToken {
    pub fn pointer_id(&self) -> PointerId {
        self.pointer_id
    }

    pub fn can_claim(&self) -> bool {
        if !self.can_claim_state() {
            return false;
        }
        if !path_is_attached(&self.state.nodes) {
            // A delayed recognizer can be first to observe a detached route.
            self.state.active.set(false);
            return false;
        }
        self.can_claim_state()
    }

    fn can_claim_state(&self) -> bool {
        self.state.active.get()
            && self.state.claimed_by.borrow().is_null()
            && self.state.gestures.borrow().contains(&self.gesture_id)
            && !self.state.dismissed.borrow().contains(&self.gesture_id)
            && Runtime::with(|rt| rt.gestures.borrow().contains_key(self.gesture_id))
    }

    #[must_use]
    pub fn claim(&self) -> bool {
        if self.can_claim() {
            *self.state.claimed_by.borrow_mut() = self.gesture_id;
            true
        } else {
            false
        }
    }

    /// Claim the whole batch or leave it unchanged. Repeated tokens for the same
    /// gesture are allowed, but different gestures cannot claim the same press.
    #[must_use]
    pub fn claim_all(tokens: &[&ClaimToken]) -> bool {
        for (index, token) in tokens.iter().enumerate() {
            if !token.can_claim()
                || tokens[..index].iter().any(|other| {
                    Rc::ptr_eq(&token.state, &other.state) && token.gesture_id != other.gesture_id
                })
            {
                return false;
            }
        }
        // Child-list reads may run reactive computations. Recheck the arena
        // state, then commit without invoking anything between claims.
        if tokens
            .iter()
            .any(|token| !token.can_claim_state() || !path_is_live(&token.state.nodes))
        {
            return false;
        }
        for token in tokens {
            *token.state.claimed_by.borrow_mut() = token.gesture_id;
        }
        true
    }

    pub fn dismiss(&self) {
        self.state.dismissed.borrow_mut().insert(self.gesture_id);
    }
}

struct PointerGestureState {
    nodes: IndexSet<NodeRef>,
    gestures: RefCell<IndexSet<GestureId>>,
    dismissed: RefCell<HashSet<GestureId>>,
    claimed_by: RefCell<GestureId>,
    active: Cell<bool>,
}

impl PointerGestureState {
    fn reset(&self) {
        self.active.set(false);
        self.gestures.borrow_mut().clear();
        self.dismissed.borrow_mut().clear();
        *self.claimed_by.borrow_mut() = GestureId::null();
    }
}

fn path_is_live(path: &IndexSet<NodeRef>) -> bool {
    Runtime::with(|rt| {
        let nodes = rt.nodes.borrow();
        path.iter().all(|node| nodes.contains_key(node.id))
    })
}

fn path_edges_match(
    path: &IndexSet<NodeRef>,
    mut is_child: impl FnMut(NodeRef, NodeRef) -> bool,
) -> bool {
    path.iter()
        .rev()
        .zip(path.iter().rev().skip(1))
        .all(|(&parent, &child)| is_child(parent, child))
}

// Hit paths are stored deepest first. An arena entry can be retained while
// hidden or mounted under a different parent; only the original edges qualify.
// Do not consult geometry: a capture can move outside the bounds it started in.
fn path_is_attached(path: &IndexSet<NodeRef>) -> bool {
    path_edges_match(path, |parent, child| {
        let children = Runtime::with(|rt| {
            let arena = rt.nodes.borrow();
            arena.get(parent.id).map(|node| *node.children)
        });
        // Pulling a computed list can create or dispose Nodes. Never do it
        // under an arena borrow, and check liveness after the pulls.
        children
            .and_then(|children| children.try_get_clone_untracked())
            .is_some_and(|children| children.contains(&child))
    }) && path_is_live(path)
}

// Geometry only: no pointer, hover or gesture callbacks run while this plan
// is built. The complete batch observes one displayed tree.
struct PlannedEvent {
    sample: InputSample,
    path: IndexSet<NodeRef>,
    position: Option<Vector>,
}

#[derive(Default)]
struct HitTestCache {
    last: Option<([u32; 2], usize)>,
}

impl HitTestCache {
    fn hit(
        &mut self,
        position: Vector,
        paths: &[PlannedEvent],
        hit: impl FnOnce() -> (IndexSet<NodeRef>, bool),
    ) -> IndexSet<NodeRef> {
        let key = position.to_array().map(f32::to_bits);
        if let Some((cached_key, index)) = self.last
            && cached_key == key
            && let Some(previous) = paths.get(index)
        {
            return previous.path.clone();
        }

        let (path, cacheable) = hit();
        self.last = cacheable.then_some((key, paths.len()));
        path
    }
}

/// A single batch of pre-hit-tested Node routes; never filters Scene input.
pub(crate) struct InputPlan {
    events: Vec<PlannedEvent>,
    hover: IndexSet<NodeRef>,
}

impl InputPlan {
    fn new(root: NodeRef, input: &InputBatch, hover: &HoverState, dpi: f32) -> Self {
        Self::with_hit_test(input, hover, dpi, |pos| hit_test_with_cache(root, pos))
    }

    fn with_hit_test(
        input: &InputBatch,
        hover: &HoverState,
        dpi: f32,
        mut hit: impl FnMut(Vector) -> (IndexSet<NodeRef>, bool),
    ) -> Self {
        let mut mouse = hover.clone();
        if input.is_interrupted() {
            mouse.set_focused(true);
        }
        let mut events = Vec::with_capacity(input.events().len());
        let mut hit_cache = HitTestCache::default();
        for sample in input.events() {
            let dpi = sample.dpi_scale.unwrap_or(dpi);
            let position = match sample.event {
                InputEvent::MouseMotion { x, y }
                | InputEvent::MouseDown { x, y, .. }
                | InputEvent::MouseUp { x, y, .. } => {
                    let pos = vec2(x / dpi, y / dpi);
                    mouse.observe_mouse(pos);
                    mouse.position()
                }
                InputEvent::MouseEnter { x, y, .. } => {
                    mouse.enter_mouse(vec2(x / dpi, y / dpi));
                    mouse.position()
                }
                InputEvent::Touch {
                    phase: miniquad::TouchPhase::Started,
                    x,
                    y,
                    ..
                } => Some(vec2(x / dpi, y / dpi)),
                InputEvent::MouseWheel { .. } => mouse.position(),
                InputEvent::MouseLeave {} => {
                    mouse.leave();
                    None
                }
                InputEvent::WindowMinimized {} => {
                    mouse.set_focused(false);
                    None
                }
                InputEvent::WindowRestored {} => {
                    mouse.set_focused(true);
                    None
                }
                _ => None,
            };
            let path = position.map_or_else(IndexSet::new, |position| {
                hit_cache.hit(position, &events, || hit(position))
            });
            events.push(PlannedEvent {
                sample: sample.clone(),
                path,
                position,
            });
        }
        let hover = mouse.position().map_or_else(IndexSet::new, |position| {
            hit_cache.hit(position, &events, || hit(position))
        });
        Self { events, hover }
    }
}

/// Match the existing public pointer-interactive query, with Scene modal bounds.
pub(crate) fn hit_scene_roots(roots: &[SceneRoot], position: Vector) -> IndexSet<NodeRef> {
    hit_roots(roots, position, false)
}

fn root_accepts_capture(roots: &[SceneRoot], node: NodeRef) -> bool {
    roots
        .iter()
        .any(|root| root.node == node && root.input != RootInput::None)
}

fn hit_roots(
    roots: &[SceneRoot],
    position: Vector,
    include_hover_wheel: bool,
) -> IndexSet<NodeRef> {
    hit_roots_with_cache(roots, position, include_hover_wheel).0
}

fn hit_roots_with_cache(
    roots: &[SceneRoot],
    position: Vector,
    include_hover_wheel: bool,
) -> (IndexSet<NodeRef>, bool) {
    if !position.is_finite() {
        return (IndexSet::new(), true);
    }
    let mut cacheable = true;
    for root in roots.iter().rev() {
        if root.input == RootInput::None || !root.bounds.contains(&position) {
            continue;
        }
        let (mut path, root_cacheable) = hit_test_with_cache(root.node, position);
        cacheable &= root_cacheable;
        if root.input == RootInput::Modal {
            if path.is_empty() && path_is_live(&IndexSet::from([root.node])) {
                path.insert(root.node);
            }
            if !path.is_empty() {
                return (path, cacheable);
            }
        } else if path.iter().any(|node| {
            is_interactive(*node)
                || (include_hover_wheel
                    && Runtime::with(|rt| {
                        rt.nodes.borrow().get(node.id).is_some_and(|node| {
                            node.hovered.is_some()
                                || node.cursor.is_some()
                                || !node.hovered_aliases.is_empty()
                                || !node.hooks.hover_event.is_empty()
                                || !node.hooks.wheel_event.is_empty()
                        })
                    }))
        }) {
            return (path, cacheable);
        }
    }
    (IndexSet::new(), cacheable)
}

struct PointerTrack {
    nodes: IndexSet<NodeRef>,
    hover_nodes: IndexSet<NodeRef>,
    state: Rc<PointerGestureState>,
    event: PointerEvent,
    claim_token: ClaimToken,
}

impl PointerTrack {
    fn new(mut nodes: IndexSet<NodeRef>, event: PointerEvent) -> Self {
        // An earlier event may have hidden a preplanned Down target. Keep
        // tracking the physical press, but don't start a stale UI operation.
        if !nodes.is_empty() && !path_is_attached(&nodes) {
            nodes.clear();
        }
        let mut gestures = IndexSet::new();
        Runtime::with(|rt| {
            for node in nodes.iter() {
                if let Some(node) = rt.nodes.borrow().get(node.id) {
                    gestures.extend(node.gestures.iter().copied());
                }
            }
        });
        let state = Rc::new(PointerGestureState {
            nodes: nodes.clone(),
            gestures: RefCell::new(gestures),
            dismissed: RefCell::default(),
            claimed_by: RefCell::new(GestureId::null()),
            active: Cell::new(true),
        });
        let claim_token = ClaimToken {
            pointer_id: event.pointer_id(),
            gesture_id: GestureId::null(),
            state: Rc::clone(&state),
        };
        Self {
            nodes,
            hover_nodes: IndexSet::new(),
            state,
            event,
            claim_token,
        }
    }

    fn set_nodes(&mut self, nodes: IndexSet<NodeRef>) {
        // A new press has a new arena state. Tokens retained by timers or
        // multi-touch recognizers from the preceding press stay invalid.
        self.state.reset();
        let next = Self::new(nodes, self.event.clone());
        self.nodes = next.nodes;
        self.state = next.state;
        self.claim_token = next.claim_token;
    }

    fn cancel_route(&mut self) {
        let nodes = std::mem::take(&mut self.nodes);
        self.state.active.set(false);
        if !nodes.is_empty() && !self.event.is_hover {
            self.event
                .update(PointerAction::Cancel, self.event.position());
            // Cancel is for the old route, even if it is detached. The runtime
            // only invokes hooks and gestures whose original owners still live.
            Runtime::with(|rt| {
                for node in nodes {
                    rt.invoke_hook(node.id, |hooks| &mut hooks.pointer_event, &self.event);
                }
            });
            self.dispatch_gestures(false);
        }
        self.state.reset();
    }

    fn flush_gestures(&mut self) {
        let claimed_by = *self.state.claimed_by.borrow();
        let rejected: Vec<_> = if claimed_by.is_null() {
            Vec::new()
        } else {
            self.state
                .gestures
                .borrow()
                .iter()
                .copied()
                .filter(|id| *id != claimed_by)
                .collect()
        };
        // Rejection may dismiss another contact (or this one), so release state
        // borrows before invoking recognizers.
        for id in rejected {
            self.state.gestures.borrow_mut().shift_remove(&id);
            Runtime::with(|rt| {
                rt.invoke_gesture(id, |gesture| {
                    gesture.on_rejected(self.event.pointer_id());
                })
            });
        }
        let dismissed = std::mem::take(&mut *self.state.dismissed.borrow_mut());
        self.state
            .gestures
            .borrow_mut()
            .retain(|id| !dismissed.contains(id));
    }

    fn dispatch(&mut self) {
        for node in &self.nodes {
            if !self.dispatch_pointer_hook(*node) {
                self.cancel_route();
                return;
            }
        }
        if !self.dispatch_gestures(true) {
            self.cancel_route();
        }
    }

    fn dispatch_pointer_hook(&self, node: NodeRef) -> bool {
        let detached = Cell::new(false);
        Runtime::with(|rt| {
            rt.invoke_hook_while(
                node.id,
                |hooks| &mut hooks.pointer_event,
                &self.event,
                || {
                    let attached = !detached.get() && self.route_is_attached();
                    detached.set(!attached);
                    attached
                },
            );
        });
        !detached.get()
    }

    fn route_is_attached(&self) -> bool {
        self.state.active.get() && path_is_attached(&self.nodes) && self.state.active.get()
    }

    fn dispatch_gestures(&mut self, require_attachment: bool) -> bool {
        self.flush_gestures();
        let gestures: Vec<_> = self.state.gestures.borrow().iter().copied().collect();
        for gesture_id in gestures {
            let claimed_by = *self.state.claimed_by.borrow();
            if (!claimed_by.is_null() && claimed_by != gesture_id)
                || self.state.dismissed.borrow().contains(&gesture_id)
            {
                continue;
            }
            if require_attachment && !self.route_is_attached() {
                return false;
            }
            self.claim_token.gesture_id = gesture_id;
            Runtime::with(|rt| {
                rt.invoke_gesture(gesture_id, |gesture| {
                    gesture.on_event(&self.event, &self.claim_token);
                })
            });
        }
        self.flush_gestures();
        true
    }

    pub fn update(&mut self, action: PointerAction, pos: Vector) {
        self.event.update(action, pos);
        self.dispatch();
    }
}

/// Routes Node pointer input. Drop revokes retained gesture tokens without
/// delivering pointer/gesture callbacks or accessing the host.
pub struct PointerManager {
    root_node: NodeRef,
    subscriber: InputSubscriber,
    tracks: HashMap<PointerId, PointerTrack>,
    hover: HoverState,
    // Fallback for position events without a source scale.
    dpi_scale: f32,
    next_dpi_scale: Option<f32>,
    planned_path: Option<IndexSet<NodeRef>>,
    cursor: crate::cursor::Cursor,
}

impl Drop for PointerManager {
    fn drop(&mut self) {
        // Node and recognizer owners can outlive the router. Invalidate their
        // delayed tokens before these tracks disappear, without user callbacks.
        for track in self.tracks.values() {
            track.state.reset();
        }
    }
}

impl Default for PointerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerManager {
    pub fn new() -> Self {
        // Native touch tracks already drive taps, drags and gestures. Otherwise
        // Macroquad also emits a mouse stream for every finger.
        macroquad::input::simulate_mouse_with_touch(false);
        let mut hover = HoverState::default();
        hover.set_window_bounds(Some(screen_bounds()));
        Self {
            root_node: NodeRef::null(),
            subscriber: InputSubscriber::new(),
            tracks: HashMap::new(),
            hover,
            dpi_scale: screen_scale(),
            next_dpi_scale: None,
            planned_path: None,
            cursor: crate::cursor::Cursor::Default,
        }
    }

    /// Collect before timers/tasks can trigger gestures from stale presses.
    pub(crate) fn prepare_input(&mut self) -> InputBatch {
        self.subscriber.read()
    }

    fn handle_interruption(&mut self, input: &InputBatch) {
        if input.is_interrupted() {
            self.cancel_input();
        }
    }

    /// Discard suspended UI input and cancel capture/gestures before ticking.
    pub(crate) fn pause_input(&mut self) {
        drop(self.subscriber.read());
        self.cancel_input();
    }

    pub(crate) fn cancel_input(&mut self) {
        // A restore event may have expired while this consumer was inactive.
        // Forget focus together with position, and wait for a fresh event.
        mue_core::batch(|| {
            self.hover.set_focused(true);
            self.invalidate_mouse();
            let ids: Vec<_> = self.tracks.keys().copied().collect();
            for id in ids {
                self.on_pointer_cancel(id);
            }
        });
    }

    pub fn process(&mut self, root_node: NodeRef) {
        let input = self.prepare_input();
        self.process_input(root_node, &input);
    }

    pub(crate) fn process_input(&mut self, root_node: NodeRef, input: &InputBatch) {
        self.update_window_metrics();
        self.root_node = root_node;
        crate::focus::set_roots([root_node]);
        let plan = InputPlan::new(root_node, input, &self.hover, self.dpi_scale);
        // Cancellation callbacks may mutate UI too, so plan before invoking them.
        self.handle_interruption(input);
        self.reconcile_captures(|root| root == root_node);
        self.dispatch_plan(plan);
        self.reconcile_captures(|root| root == root_node);
    }

    /// Freeze Node targets before lifecycle, cancellation or input callbacks.
    /// Scene receives the original InputBatch independently of this plan.
    pub(crate) fn plan_roots(&mut self, roots: &[SceneRoot], input: &InputBatch) -> InputPlan {
        self.update_window_metrics();
        InputPlan::with_hit_test(input, &self.hover, self.dpi_scale, |pos| {
            hit_roots_with_cache(roots, pos, true)
        })
    }

    fn update_window_metrics(&mut self) {
        // Window bounds are sampled once; position samples may have their own
        // scale. Neither metric change terminates an existing capture.
        self.dpi_scale = screen_scale();
        self.hover.set_window_bounds(Some(screen_bounds()));
    }

    /// Dispatch precomputed Node routes without modifying the raw Scene batch.
    pub(crate) fn dispatch_roots(
        &mut self,
        plan: InputPlan,
        roots: &[SceneRoot],
        input: &InputBatch,
    ) {
        crate::focus::set_roots(
            roots
                .iter()
                .filter(|root| root.input != RootInput::None)
                .map(|root| root.node),
        );
        // Cancellation can invoke callbacks, so every target is planned first.
        self.handle_interruption(input);
        self.reconcile_roots(roots);
        self.dispatch_plan(plan);
        self.reconcile_roots(roots);
    }

    /// Cancel existing routes no longer attached to the displayed forest.
    pub(crate) fn reconcile_roots(&mut self, roots: &[SceneRoot]) {
        self.reconcile_captures(|node| root_accepts_capture(roots, node));
    }

    /// Record dropped captures using the prepared edges of the roots actually
    /// drawn. Do not run callbacks or read reactive children after rendering.
    /// A later reattachment cannot restore a press absent from this frame.
    pub(crate) fn mark_undisplayed_captures(
        &mut self,
        roots: &[SceneRoot],
        mut is_child: impl FnMut(NodeRef, NodeRef) -> bool,
    ) {
        for track in self.tracks.values() {
            if track.state.active.get()
                && track.nodes.last().is_some_and(|root| {
                    !root_accepts_capture(roots, *root)
                        || !path_edges_match(&track.nodes, &mut is_child)
                })
            {
                // Retain the old callback route until the next input boundary.
                track.state.active.set(false);
            }
        }
    }

    fn reconcile_captures(&mut self, mut root_is_eligible: impl FnMut(NodeRef) -> bool) {
        // A Cancel callback can detach another contact. Repeat only if a route
        // was revoked, never scan the displayed tree or re-hit an old press.
        loop {
            let mut canceled = false;
            mue_core::batch(|| {
                let revoked: Vec<_> = self
                    .tracks
                    .values_mut()
                    .filter(|track| {
                        !track.event.is_hover
                            && track.nodes.last().is_some_and(|root| {
                                !root_is_eligible(*root) || !track.route_is_attached()
                            })
                    })
                    .collect();
                // Revoke every invalid claim before a Cancel callback can
                // reattach a route. Keep tracking its physical buttons until Up.
                canceled = !revoked.is_empty();
                for track in &revoked {
                    track.state.active.set(false);
                }
                for track in revoked {
                    track.cancel_route();
                }
            });
            if !canceled {
                break;
            }
        }
    }

    fn dispatch_plan(&mut self, plan: InputPlan) {
        // Deferred recognizer callbacks can change the tree too. All routing
        // targets are already planned against the displayed tree at this point.
        self.finish_event();
        for planned in plan.events {
            self.planned_path = Some(planned.path);
            mue_core::batch(|| {
                if let InputEvent::MouseWheel { x, y } = planned.sample.event {
                    if let Some(position) = planned.position {
                        self.dispatch_wheel(position, vec2(x, y));
                    }
                } else {
                    planned.sample.repeat(self);
                }
            });
            self.finish_event();
        }
        self.planned_path = Some(plan.hover);
        self.refresh_hover(false);
        self.planned_path = None;
    }

    fn finish_event(&mut self) {
        for track in self.tracks.values_mut() {
            track.flush_gestures();
        }
        // A callback may hide a different contact. Revoke it before a later
        // event can re-show it. The displayed root set does not change here.
        self.reconcile_captures(|_| true);
    }

    fn hit_path(&self, position: Vector) -> IndexSet<NodeRef> {
        self.planned_path
            .clone()
            .unwrap_or_else(|| hit_test(self.root_node, position))
    }

    fn dispatch_wheel(&self, position: Vector, delta: Vector) {
        let path = self.hit_path(position);
        if !path_is_live(&path) {
            return;
        }
        let event = crate::event::wheel::WheelEvent::new(position, delta);
        mue_core::batch(|| {
            Runtime::with(|rt| {
                for node in &path {
                    if !path_is_live(&path) {
                        break;
                    }
                    rt.invoke_hook(node.id, |hooks| &mut hooks.wheel_event, &event);
                }
            })
        });
    }

    fn logical_position(&mut self, x: f32, y: f32) -> Vector {
        let scale = self.next_dpi_scale.take().unwrap_or(self.dpi_scale);
        Vector::new(x / scale, y / scale)
    }

    fn accepts_mouse_position(&mut self, pos: Vector, in_window: bool) -> bool {
        // Report no hover outside the current viewport, but don't infer native
        // leave from coordinates: the window can resize between event and replay.
        if in_window
            || (pos.is_finite()
                && self
                    .tracks
                    .get(&PointerId::MOUSE)
                    .is_some_and(|track| !track.event.is_hover))
        {
            true
        } else {
            self.invalidate_mouse();
            false
        }
    }

    fn refresh_hover(&mut self, moved: bool) {
        let path = if let Some(path) = &self.planned_path {
            let path = if self.hover.position().is_some() && path_is_live(path) {
                path.clone()
            } else {
                IndexSet::new()
            };
            self.hover.update_path(path.clone());
            path
        } else {
            self.hover.rehit(self.root_node)
        };
        let path = if path_is_live(&path) {
            path
        } else {
            IndexSet::new()
        };
        self.update_cursor(&path);
        if let Some(track) = self.tracks.get_mut(&PointerId::MOUSE) {
            Self::update_hover_nodes(track, path, moved);
        }
    }

    /// Show the deepest hovered node's cursor, applying only changes.
    fn update_cursor(&mut self, path: &IndexSet<NodeRef>) {
        use crate::cursor::Cursor;
        let cursor = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            path.iter()
                .filter_map(|node| nodes.get(node.id)?.cursor.as_ref())
                .map(mue_core::signal::Access::get_untracked)
                .find(|cursor| *cursor != Cursor::Auto)
        })
        .unwrap_or(Cursor::Default);
        if cursor != self.cursor {
            self.cursor = cursor;
            miniquad::window::set_mouse_cursor(cursor.icon());
        }
    }

    fn invalidate_mouse(&mut self) {
        // The caller has already cleared/updated the mouse boundary state.
        self.refresh_hover(false);
        // Native leave, focus loss and interruptions end mouse capture.
        if let Some(track) = self.tracks.get(&PointerId::MOUSE) {
            if !track.event.is_hover {
                self.on_pointer_cancel(PointerId::MOUSE);
            } else {
                self.tracks.remove(&PointerId::MOUSE);
            }
        }
    }

    fn on_track_down(track: &mut PointerTrack, path: IndexSet<NodeRef>, pos: Vector) {
        assert!(track.event.is_hover);
        track.event.is_hover = false;
        track.event.start_position = pos;
        track.event.position = pos;
        track.set_nodes(path);
        track.update(PointerAction::Down, pos);
    }

    fn update_hover_nodes(track: &mut PointerTrack, hover_nodes: IndexSet<NodeRef>, moved: bool) {
        let mut event = track.event.clone();
        event.is_hover = true;
        Runtime::with(|rt| {
            for node in track.hover_nodes.iter() {
                if hover_nodes.contains(node) && !moved {
                    continue;
                }
                if hover_nodes.contains(node) && !path_is_live(&hover_nodes) {
                    continue;
                }
                event.action = if hover_nodes.contains(node) {
                    PointerAction::Move
                } else {
                    PointerAction::Up
                };
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &event);
            }
            for node in hover_nodes.difference(&track.hover_nodes) {
                if !path_is_live(&hover_nodes) {
                    break;
                }
                event.action = PointerAction::Down;
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &event);
            }
            track.hover_nodes = if path_is_live(&hover_nodes) {
                hover_nodes
            } else {
                IndexSet::new()
            };
        });
    }

    fn on_track_move(track: &mut PointerTrack, pos: Vector) {
        if track.event.is_hover {
            track.event.update(PointerAction::Move, pos);
        } else {
            track.update(PointerAction::Move, pos);
        }
    }

    fn on_track_up(track: &mut PointerTrack, pos: Vector) {
        track.update(PointerAction::Up, pos);
        track.state.reset();
        track.nodes.clear();
        track.event.is_hover = true;
        track.event.action = PointerAction::Move;
    }

    fn on_pointer_start(&mut self, id: PointerId, pointer_type: PointerType, pos: Vector) {
        let path = self.hit_path(pos);
        crate::focus::pointer_down(&path);
        // A repeated native touch id starts a new sequence, not a second Down
        // on the old recognizer state.
        self.on_pointer_cancel(id);
        let event = PointerEvent::new(id, pointer_type, pos);
        let mut track = PointerTrack::new(path, event);
        track.dispatch();
        self.tracks.insert(id, track);
    }

    fn on_pointer_move(&mut self, id: PointerId, pointer_type: PointerType, pos: Vector) {
        if let Some(track) = self.tracks.get_mut(&id) {
            Self::on_track_move(track, pos);
        } else if pointer_type == PointerType::Mouse {
            // Only a mouse move without start can establish hover.
            let mut event = PointerEvent::new(id, pointer_type, pos);
            event.is_hover = true;
            let track = PointerTrack::new(IndexSet::new(), event);
            self.tracks.insert(id, track);
        }
    }

    fn on_pointer_end(&mut self, id: PointerId, pos: Vector) {
        if let Some(mut track) = self.tracks.remove(&id) {
            track.update(PointerAction::Up, pos);
            track.state.reset();
        }
    }

    fn on_pointer_cancel(&mut self, id: PointerId) {
        if let Some(mut track) = self.tracks.remove(&id) {
            track.cancel_route();
        }
    }

    fn on_pointer_button_down(
        &mut self,
        id: PointerId,
        pointer_type: PointerType,
        button_state: ButtonState,
        pos: Vector,
    ) {
        if let Some(track) = self.tracks.get_mut(&id)
            && !track.event.is_hover
        {
            // Additional mouse buttons stay on the existing capture path.
            track.event.button_state.insert(button_state);
            Self::on_track_move(track, pos);
            return;
        }
        let path = self.hit_path(pos);
        crate::focus::pointer_down(&path);
        if let Some(track) = self.tracks.get_mut(&id) {
            track.event.button_state.insert(button_state);
            Self::on_track_down(track, path, pos);
        } else {
            let mut event = PointerEvent::new(id, pointer_type, pos);
            event.button_state.insert(button_state);
            let mut track = PointerTrack::new(path, event);
            track.dispatch();
            self.tracks.insert(id, track);
        }
    }

    fn on_pointer_button_up(&mut self, id: PointerId, button_state: ButtonState, pos: Vector) {
        if let Some(track) = self.tracks.get_mut(&id) {
            if !track.event.button_state.contains(button_state) {
                return;
            }
            track.event.button_state.remove(button_state);
            if track.event.button_state.is_empty() {
                Self::on_track_up(track, pos);
            } else {
                Self::on_track_move(track, pos);
            }
        }
    }
}

impl EventHandler for PointerManager {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn input_event_dpi_scale(&mut self, scale: f32) {
        self.next_dpi_scale = valid_dpi_scale(scale);
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let pos = self.logical_position(x, y);
        let in_window = self.hover.observe_mouse(pos);
        if !self.accepts_mouse_position(pos, in_window) {
            return;
        }
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = self.logical_position(x, y);
        let in_window = self.hover.observe_mouse(pos);
        if !self.accepts_mouse_position(pos, in_window) {
            return;
        }
        self.on_pointer_button_down(PointerId::MOUSE, PointerType::Mouse, button_state, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = self.logical_position(x, y);
        let in_window = self.hover.observe_mouse(pos);
        if !self.accepts_mouse_position(pos, in_window) {
            return;
        }
        let unmatched_capture = self.tracks.get(&PointerId::MOUSE).is_some_and(|track| {
            !track.event.is_hover && !track.event.button_state.contains(button_state)
        });
        // A release from an earlier scene/press may update hover, but must not
        // move or release a new capture established by another physical button.
        // The raw Scene batch still contains this event unchanged.
        if !unmatched_capture {
            self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
            self.on_pointer_button_up(PointerId::MOUSE, button_state, pos);
        }
        self.refresh_hover(true);
    }

    fn mouse_leave_event(&mut self) {
        self.hover.leave();
        self.invalidate_mouse();
    }

    fn mouse_enter_event(&mut self, _button: MouseButton, x: f32, y: f32) {
        // The platform supplies a fresh position; entering is not a button press.
        let pos = self.logical_position(x, y);
        let in_window = self.hover.enter_mouse(pos);
        if !self.accepts_mouse_position(pos, in_window) {
            return;
        }
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
        self.refresh_hover(true);
    }

    fn window_minimized_event(&mut self) {
        self.hover.set_focused(false);
        self.invalidate_mouse();
        let ids: Vec<_> = self.tracks.keys().copied().collect();
        for id in ids {
            self.on_pointer_cancel(id);
        }
    }

    fn key_down_event(
        &mut self,
        keycode: miniquad::KeyCode,
        modifiers: miniquad::KeyMods,
        repeat: bool,
    ) {
        crate::focus::dispatch_key_down(crate::event::keyboard::KeyEvent::new(
            keycode, modifiers, repeat,
        ));
    }

    fn key_up_event(&mut self, keycode: miniquad::KeyCode, modifiers: miniquad::KeyMods) {
        crate::focus::dispatch_key_up(crate::event::keyboard::KeyEvent::new(
            keycode, modifiers, false,
        ));
    }

    fn char_event(&mut self, character: char, modifiers: miniquad::KeyMods, repeat: bool) {
        crate::focus::dispatch_char(crate::event::keyboard::CharEvent::new(
            character, modifiers, repeat,
        ));
    }

    fn window_restored_event(&mut self) {
        self.hover.set_focused(true);
        self.refresh_hover(false);
    }

    fn on_ime_preedit(&mut self, text: &str, cursor: usize) {
        crate::text_input::dispatch_platform_event(crate::text_input::PlatformEvent::Preedit {
            text: text.to_string(),
            cursor_utf16: Some(cursor),
        });
    }

    fn on_ime_commit(&mut self, text: Option<&str>) {
        crate::text_input::dispatch_platform_event(crate::text_input::PlatformEvent::Commit(
            text.map(str::to_string),
        ));
    }

    fn on_ime_state_changed(
        &mut self,
        text: &str,
        selection_start: usize,
        selection_end: usize,
        composing_start: Option<usize>,
        composing_end: Option<usize>,
        _element_id: u64,
    ) {
        let offset = |value: Option<usize>| value.map_or(-1, |value| value as i32);
        crate::text_input::dispatch_platform_event(crate::text_input::PlatformEvent::State {
            text: text.to_string(),
            selection: (selection_start as i32, selection_end as i32),
            composing: (offset(composing_start), offset(composing_end)),
        });
    }

    fn text_input_action_event(&mut self, action: miniquad::TextInputAction) {
        crate::text_input::dispatch_platform_event(crate::text_input::PlatformEvent::Action(
            crate::text_input::from_miniquad_action(action),
        ));
    }

    fn soft_keyboard_event(&mut self, visible: bool, x: f32, y: f32, w: f32, h: f32) {
        let dpi = self.dpi_scale.max(f32::EPSILON);
        let frame = if visible {
            Rect::new(x / dpi, y / dpi, w / dpi, h / dpi)
        } else {
            Rect::default()
        };
        crate::text_input::dispatch_platform_event(crate::text_input::PlatformEvent::Keyboard(
            frame,
        ));
    }

    fn touch_event(&mut self, phase: miniquad::TouchPhase, id: u64, x: f32, y: f32, _time: f64) {
        let pointer_id = PointerId::from_touch_id(id);
        let pos = self.logical_position(x, y);
        match phase {
            miniquad::TouchPhase::Started => {
                self.on_pointer_start(pointer_id, PointerType::Touch, pos)
            }
            miniquad::TouchPhase::Moved => {
                self.on_pointer_move(pointer_id, PointerType::Touch, pos)
            }
            miniquad::TouchPhase::Ended => self.on_pointer_end(pointer_id, pos),
            miniquad::TouchPhase::Cancelled => self.on_pointer_cancel(pointer_id),
        }
    }
}
