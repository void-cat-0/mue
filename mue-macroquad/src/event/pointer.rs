use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

use bitflags::bitflags;
use glam::vec2;
use indexmap::IndexSet;
use macroquad::input::utils::{register_input_subscriber, repeat_all_miniquad_input};
use miniquad::{EventHandler, MouseButton};

use slotmap::{Key, SlotMap};

use crate::{
    event::hit_test::hit_test,
    gesture::{Gesture, GestureId},
    math::Vector,
    node::NodeRef,
    runtime::{Runtime, get_time},
};

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

#[derive(Debug)]
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
            time: get_time(),
        }
    }

    fn update(&mut self, action: PointerAction, current_pos: Vector) {
        self.delta = current_pos - self.position;
        self.position = current_pos;
        self.action = action;
        self.time = get_time();
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
        self.state.claimed_by.borrow().is_null()
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

    #[must_use]
    pub fn claim_all(tokens: &[&ClaimToken]) -> bool {
        if !tokens.iter().all(|token| token.can_claim()) {
            false
        } else {
            for token in tokens {
                let _ = token.claim();
            }
            true
        }
    }

    pub fn dismiss(&self) {
        self.state.dismissed.borrow_mut().insert(self.gesture_id);
    }
}

struct PointerGestureState {
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
                gestures.extend(rt.node(node.id).gestures.iter().copied());
            }
        });
        let state = Rc::new(PointerGestureState {
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
        self.state.reset();
        let mut gestures = self.state.gestures.borrow_mut();
        gestures.clear();
        Runtime::with(|rt| {
            for node in nodes.iter() {
                gestures.extend(rt.node(node.id).gestures.iter().copied());
            }
        });
        self.nodes = nodes;
    }

    fn flush_updates(&mut self) {
        Runtime::with(|rt| self.flush_gestures(&mut rt.gestures.borrow_mut()));
    }

    fn flush_gestures(&mut self, gestures: &mut SlotMap<GestureId, Box<dyn Gesture>>) {
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
            if let Some(gesture) = gestures.get_mut(id) {
                gesture.on_rejected(self.event.pointer_id());
            }
        }
        let dismissed = std::mem::take(&mut *self.state.dismissed.borrow_mut());
        self.state
            .gestures
            .borrow_mut()
            .retain(|id| !dismissed.contains(id));
    }

    fn dispatch(&mut self) {
        Runtime::with(|rt| {
            if self.event.is_hover {
                for node in self.hover_nodes.iter() {
                    rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &self.event);
                }
            } else {
                for node in self.nodes.iter() {
                    rt.invoke_hook(node.id, |hooks| &mut hooks.pointer_event, &self.event);
                }

                self.dispatch_gestures(&mut rt.gestures.borrow_mut());
            }
        });
    }

    fn dispatch_gestures(&mut self, gestures: &mut SlotMap<GestureId, Box<dyn Gesture>>) {
        self.flush_gestures(gestures);
        for gesture_id in self.state.gestures.borrow().iter() {
            let claimed_by = *self.state.claimed_by.borrow();
            if (!claimed_by.is_null() && claimed_by != *gesture_id)
                || self.state.dismissed.borrow().contains(gesture_id)
            {
                continue;
            }
            let Some(gesture) = gestures.get_mut(*gesture_id) else {
                continue;
            };
            self.claim_token.gesture_id = *gesture_id;
            gesture.on_event(&self.event, &self.claim_token);
        }
        self.flush_gestures(gestures);
    }

    pub fn update(&mut self, action: PointerAction, pos: Vector) {
        self.event.update(action, pos);
        self.dispatch();
    }
}

pub struct PointerManager {
    root_node: NodeRef,
    subscriber_id: usize,
    tracks: HashMap<PointerId, PointerTrack>,
}

impl Default for PointerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerManager {
    pub fn new() -> Self {
        Self {
            root_node: NodeRef::null(),
            subscriber_id: register_input_subscriber(),
            tracks: HashMap::new(),
        }
    }

    pub fn process(&mut self, root_node: NodeRef) {
        self.root_node = root_node;
        for track in self.tracks.values_mut() {
            track.flush_updates();
            if track.event.is_hover {
                Self::update_hover_nodes(track, hit_test(root_node, track.event.position()));
            }
        }
        repeat_all_miniquad_input(self, self.subscriber_id);
    }

    fn on_track_down(track: &mut PointerTrack, root_node: NodeRef, pos: Vector) {
        assert!(track.event.is_hover);
        track.event.is_hover = false;
        track.event.start_position = pos;
        track.event.position = pos;
        track.set_nodes(hit_test(root_node, pos));
        track.update(PointerAction::Down, pos);
    }

    fn update_hover_nodes(track: &mut PointerTrack, hover_nodes: IndexSet<NodeRef>) {
        assert!(track.event.is_hover);
        Runtime::with(|rt| {
            for node in track.hover_nodes.iter() {
                track.event.action = if hover_nodes.contains(node) {
                    PointerAction::Move
                } else {
                    PointerAction::Up
                };
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &track.event);
            }
            for node in hover_nodes.difference(&track.hover_nodes) {
                track.event.action = PointerAction::Down;
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &track.event);
            }
            track.hover_nodes = hover_nodes;
        });
    }

    fn on_track_move(track: &mut PointerTrack, root_node: NodeRef, pos: Vector) {
        if track.event.is_hover {
            track.event.update(PointerAction::Move, pos);
            Self::update_hover_nodes(track, hit_test(root_node, pos));
        } else {
            track.update(PointerAction::Move, pos);
        }
    }

    fn on_track_up(track: &mut PointerTrack, root_node: NodeRef, pos: Vector) {
        track.update(PointerAction::Up, pos);
        track.state.reset();
        track.event.is_hover = true;
        track.event.action = PointerAction::Move;
        Self::update_hover_nodes(track, hit_test(root_node, pos));
    }

    fn on_pointer_start(&mut self, id: PointerId, pointer_type: PointerType, pos: Vector) {
        if let Some(track) = self.tracks.get_mut(&id) {
            Self::on_track_down(track, self.root_node, pos);
        } else {
            let event = PointerEvent::new(id, pointer_type, pos);
            let mut track = PointerTrack::new(hit_test(self.root_node, pos), event);
            track.dispatch();
            self.tracks.insert(id, track);
        }
    }

    fn on_pointer_move(&mut self, id: PointerId, pointer_type: PointerType, pos: Vector) {
        if let Some(track) = self.tracks.get_mut(&id) {
            Self::on_track_move(track, self.root_node, pos);
        } else {
            // Move event without start, enters hover state
            let mut event = PointerEvent::new(id, pointer_type, pos);
            event.is_hover = true;
            let mut track = PointerTrack::new(IndexSet::new(), event);
            Self::update_hover_nodes(&mut track, hit_test(self.root_node, pos));
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
        if let Some(track) = self.tracks.get_mut(&id) {
            track.event.button_state.insert(button_state);
            if track.event.is_hover {
                Self::on_track_down(track, self.root_node, pos);
            } else {
                Self::on_track_move(track, self.root_node, pos);
            }
        } else {
            let mut event = PointerEvent::new(id, pointer_type, pos);
            event.button_state.insert(button_state);
            let mut track = PointerTrack::new(hit_test(self.root_node, pos), event);
            track.dispatch();
            self.tracks.insert(id, track);
        }
    }

    fn on_pointer_button_up(&mut self, id: PointerId, button_state: ButtonState, pos: Vector) {
        if let Some(track) = self.tracks.get_mut(&id) {
            track.event.button_state.remove(button_state);
            if track.event.button_state.is_empty() {
                Self::on_track_up(track, self.root_node, pos);
            } else {
                Self::on_track_move(track, self.root_node, pos);
            }
        }
    }
}

impl EventHandler for PointerManager {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let pos = Vector::new(x, y);
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = Vector::new(x, y);
        self.on_pointer_button_down(PointerId::MOUSE, PointerType::Mouse, button_state, pos);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = Vector::new(x, y);
        self.on_pointer_button_up(PointerId::MOUSE, button_state, pos);
    }

    fn touch_event(&mut self, phase: miniquad::TouchPhase, id: u64, x: f32, y: f32, _time: f64) {
        let pointer_id = PointerId::from_touch_id(id);
        let pos = Vector::new(x, y);
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
