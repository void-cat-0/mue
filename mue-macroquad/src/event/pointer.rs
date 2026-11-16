use std::{
    cell::RefCell,
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
        hit_test::{hit_test, is_interactive},
        hover::HoverState,
    },
    gesture::GestureId,
    input::{InputBatch, InputEvent, InputSubscriber},
    math::{Rect, Vector},
    node::NodeRef,
    runtime::Runtime,
    scene::{RootInput, SceneRoot},
};

fn screen_scale() -> f32 {
    let scale = macroquad::window::screen_dpi_scale();
    if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    }
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
        path_is_live(&self.state.nodes)
            && Runtime::with(|rt| rt.gestures.borrow().contains_key(self.gesture_id))
            && self.state.claimed_by.borrow().is_null()
            && self.state.gestures.borrow().contains(&self.gesture_id)
            && !self.state.dismissed.borrow().contains(&self.gesture_id)
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
        // Validation invokes no callbacks; commit directly so repeated tokens
        // do not fail a second claim after their shared state has been claimed.
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
}

impl PointerGestureState {
    fn reset(&self) {
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

// Geometry only: no pointer, hover or gesture callbacks run while this plan
// is built. The complete batch observes one displayed tree.
struct PlannedEvent {
    event: InputEvent,
    path: IndexSet<NodeRef>,
    position: Option<Vector>,
}

/// A single batch of pre-hit-tested Node routes; never filters Scene input.
pub(crate) struct InputPlan {
    events: Vec<PlannedEvent>,
    hover: IndexSet<NodeRef>,
}

impl InputPlan {
    fn new(root: NodeRef, input: &InputBatch, hover: &HoverState, dpi: f32) -> Self {
        Self::with_hit_test(input, hover, dpi, |pos| hit_test(root, pos))
    }

    fn with_hit_test(
        input: &InputBatch,
        hover: &HoverState,
        dpi: f32,
        mut hit: impl FnMut(Vector) -> IndexSet<NodeRef>,
    ) -> Self {
        let mut mouse = hover.clone();
        if input.is_interrupted() {
            mouse.set_focused(true);
        }
        let mut events = Vec::new();
        for event in input.events() {
            let position = match *event {
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
            let path = position.map_or_else(IndexSet::new, &mut hit);
            events.push(PlannedEvent {
                event: event.clone(),
                path,
                position,
            });
        }
        let hover = mouse.position().map_or_else(IndexSet::new, hit);
        Self { events, hover }
    }
}

/// Match the existing public pointer-interactive query, with Scene modal bounds.
pub(crate) fn hit_scene_roots(roots: &[SceneRoot], position: Vector) -> IndexSet<NodeRef> {
    hit_roots(roots, position, false)
}

fn hit_roots(
    roots: &[SceneRoot],
    position: Vector,
    include_hover_wheel: bool,
) -> IndexSet<NodeRef> {
    if !position.is_finite() {
        return IndexSet::new();
    }
    for root in roots.iter().rev() {
        if root.input == RootInput::None || !root.bounds.contains(&position) {
            continue;
        }
        let mut path = hit_test(root.node, position);
        if root.input == RootInput::Modal {
            if path.is_empty() && path_is_live(&IndexSet::from([root.node])) {
                path.insert(root.node);
            }
            if !path.is_empty() {
                return path;
            }
        } else if path.iter().any(|node| {
            is_interactive(*node)
                || (include_hover_wheel
                    && Runtime::with(|rt| {
                        rt.nodes.borrow().get(node.id).is_some_and(|node| {
                            node.hovered.is_some()
                                || !node.hovered_aliases.is_empty()
                                || !node.hooks.hover_event.is_empty()
                                || !node.hooks.wheel_event.is_empty()
                        })
                    }))
        }) {
            return path;
        }
    }
    IndexSet::new()
}

struct PointerTrack {
    nodes: IndexSet<NodeRef>,
    hover_nodes: IndexSet<NodeRef>,
    state: Rc<PointerGestureState>,
    event: PointerEvent,
    claim_token: ClaimToken,
}

impl PointerTrack {
    fn new(nodes: IndexSet<NodeRef>, event: PointerEvent) -> Self {
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

    fn flush_updates(&mut self) {
        if !path_is_live(&self.nodes) {
            self.cancel_route();
        } else {
            self.flush_gestures();
        }
    }

    fn cancel_route(&mut self) {
        let gestures: Vec<_> = self.state.gestures.borrow().iter().copied().collect();
        self.state.reset();
        self.nodes.clear();
        for id in gestures {
            Runtime::with(|rt| {
                rt.invoke_gesture(id, |gesture| {
                    gesture.on_rejected(self.event.pointer_id());
                })
            });
        }
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
        if !self.event.is_hover && !path_is_live(&self.nodes) {
            self.cancel_route();
            return;
        }
        Runtime::with(|rt| {
            if self.event.is_hover {
                for node in self.hover_nodes.iter() {
                    rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &self.event);
                }
            } else {
                for node in &self.nodes {
                    if !path_is_live(&self.nodes) {
                        break;
                    }
                    rt.invoke_hook(node.id, |hooks| &mut hooks.pointer_event, &self.event);
                }

                self.dispatch_gestures();
            }
        });
    }

    fn dispatch_gestures(&mut self) {
        self.flush_gestures();
        let gestures: Vec<_> = self.state.gestures.borrow().iter().copied().collect();
        for gesture_id in gestures {
            if !path_is_live(&self.nodes) {
                self.cancel_route();
                break;
            }
            let claimed_by = *self.state.claimed_by.borrow();
            if (!claimed_by.is_null() && claimed_by != gesture_id)
                || self.state.dismissed.borrow().contains(&gesture_id)
            {
                continue;
            }
            self.claim_token.gesture_id = gesture_id;
            Runtime::with(|rt| {
                rt.invoke_gesture(gesture_id, |gesture| {
                    gesture.on_event(&self.event, &self.claim_token);
                })
            });
        }
        self.flush_gestures();
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
    /// Ratio of physical miniquad event coordinates to logical layout
    /// coordinates. Updated from macroquad on every frame.
    dpi_scale: f32,
    planned_path: Option<IndexSet<NodeRef>>,
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
            planned_path: None,
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
        let plan = InputPlan::new(root_node, input, &self.hover, self.dpi_scale);
        // Cancellation callbacks may mutate UI too, so plan before invoking them.
        self.handle_interruption(input);
        self.dispatch_plan(plan);
    }

    /// Freeze Node targets before lifecycle, cancellation or input callbacks.
    /// Scene receives the original InputBatch independently of this plan.
    pub(crate) fn plan_roots(&mut self, roots: &[SceneRoot], input: &InputBatch) -> InputPlan {
        self.update_window_metrics();
        InputPlan::with_hit_test(input, &self.hover, self.dpi_scale, |pos| {
            hit_roots(roots, pos, true)
        })
    }

    fn update_window_metrics(&mut self) {
        // Event coordinates and bounds are logical.
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
        // Cancellation can invoke callbacks, so every target is planned first.
        self.handle_interruption(input);
        let removed: Vec<_> = self
            .tracks
            .iter()
            .filter(|(_, track)| {
                !track.event.is_hover
                    && !track.nodes.is_empty()
                    && !roots.iter().any(|root| {
                        root.input != RootInput::None && track.nodes.contains(&root.node)
                    })
            })
            .map(|(id, _)| *id)
            .collect();
        mue_core::batch(|| {
            for id in removed {
                // Cancel the old UI operation, but retain this Node track
                // through release. Another mouse button cannot retarget a drag.
                // This state does not restrict what the Scene receives.
                if let Some(mut track) = self.tracks.remove(&id) {
                    track.update(PointerAction::Cancel, track.event.position());
                    track.state.reset();
                    track.nodes.clear();
                    self.tracks.insert(id, track);
                }
            }
        });
        self.dispatch_plan(plan);
    }

    fn dispatch_plan(&mut self, plan: InputPlan) {
        // Even deferred recognizer callbacks may change the tree: build the
        // entire routing plan before flushing them or touching hover signals.
        for track in self.tracks.values_mut() {
            track.flush_updates();
        }
        for planned in plan.events {
            self.planned_path = Some(planned.path);
            mue_core::batch(|| {
                if let InputEvent::MouseWheel { x, y } = planned.event {
                    if let Some(position) = planned.position {
                        self.dispatch_wheel(position, vec2(x, y));
                    }
                } else {
                    planned.event.repeat(self);
                }
            });
            for track in self.tracks.values_mut() {
                track.flush_updates();
            }
        }
        self.planned_path = Some(plan.hover);
        self.refresh_hover(false);
        self.planned_path = None;
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

    fn logical_position(&self, x: f32, y: f32) -> Vector {
        let scale = self.dpi_scale.max(f32::EPSILON);
        Vector::new(x / scale, y / scale)
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
        if let Some(track) = self.tracks.get_mut(&PointerId::MOUSE) {
            Self::update_hover_nodes(track, path, moved);
        }
    }

    fn invalidate_mouse(&mut self) {
        // The caller has already cleared/updated the mouse boundary state.
        self.refresh_hover(false);
        // Capture never keeps a pointer alive after leaving the window.
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
        track.event.is_hover = true;
        track.event.action = PointerAction::Move;
    }

    fn on_pointer_start(&mut self, id: PointerId, pointer_type: PointerType, pos: Vector) {
        let path = self.hit_path(pos);
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
            track.update(PointerAction::Cancel, track.event.position());
            track.state.reset();
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

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let pos = self.logical_position(x, y);
        if !self.hover.observe_mouse(pos) {
            self.invalidate_mouse();
            return;
        }
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = self.logical_position(x, y);
        if !self.hover.observe_mouse(pos) {
            self.invalidate_mouse();
            return;
        }
        self.on_pointer_button_down(PointerId::MOUSE, PointerType::Mouse, button_state, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = self.logical_position(x, y);
        if !self.hover.observe_mouse(pos) {
            self.invalidate_mouse();
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
        if !self.hover.enter_mouse(pos) {
            self.invalidate_mouse();
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

    fn window_restored_event(&mut self) {
        self.hover.set_focused(true);
        self.refresh_hover(false);
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
