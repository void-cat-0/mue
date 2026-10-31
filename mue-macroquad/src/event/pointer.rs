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
    event::{hit_test::hit_test, hover::HoverState},
    gesture::{Gesture, GestureId},
    math::Vector,
    node::NodeRef,
    runtime::Runtime,
};

fn input_time() -> f32 {
    #[cfg(not(test))]
    {
        crate::runtime::get_time()
    }
    #[cfg(test)]
    {
        0.
    }
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
            time: input_time(),
        }
    }

    fn update(&mut self, action: PointerAction, current_pos: Vector) {
        self.delta = current_pos - self.position;
        self.position = current_pos;
        self.action = action;
        self.time = input_time();
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
                if let Some(node) = rt.nodes.borrow().get(node.id) {
                    gestures.extend(node.gestures.iter().copied());
                }
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
                if let Some(node) = rt.nodes.borrow().get(node.id) {
                    gestures.extend(node.gestures.iter().copied());
                }
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
    hover: HoverState,
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
        Self {
            root_node: NodeRef::null(),
            subscriber_id: register_input_subscriber(),
            tracks: HashMap::new(),
            hover: HoverState::default(),
        }
    }

    pub fn process(&mut self, root_node: NodeRef) {
        self.root_node = root_node;
        for track in self.tracks.values_mut() {
            track.flush_updates();
        }
        // Recognizers may synchronously schedule disposal; flush after their
        // gesture arena borrow has been released. Hover writes remain atomic.
        mue_core::batch(|| repeat_all_miniquad_input(self, self.subscriber_id));
        // Always re-hit after the frame's layout/transform/scroll publication,
        // even if no mouse event arrived or a button is currently held.
        self.refresh_hover(false);
    }

    fn refresh_hover(&mut self, moved: bool) {
        let path = self.hover.rehit(self.root_node);
        if let Some(track) = self.tracks.get_mut(&PointerId::MOUSE) {
            Self::update_hover_nodes(track, path, moved);
        }
    }

    fn invalidate_mouse(&mut self) {
        self.hover.leave();
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

    fn on_track_down(track: &mut PointerTrack, root_node: NodeRef, pos: Vector) {
        assert!(track.event.is_hover);
        track.event.is_hover = false;
        track.event.start_position = pos;
        track.event.position = pos;
        track.set_nodes(hit_test(root_node, pos));
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
                event.action = if hover_nodes.contains(node) {
                    PointerAction::Move
                } else {
                    PointerAction::Up
                };
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &event);
            }
            for node in hover_nodes.difference(&track.hover_nodes) {
                event.action = PointerAction::Down;
                rt.invoke_hook(node.id, |hooks| &mut hooks.hover_event, &event);
            }
            track.hover_nodes = hover_nodes;
        });
    }

    fn on_track_move(track: &mut PointerTrack, _root_node: NodeRef, pos: Vector) {
        if track.event.is_hover {
            track.event.update(PointerAction::Move, pos);
        } else {
            track.update(PointerAction::Move, pos);
        }
    }

    fn on_track_up(track: &mut PointerTrack, _root_node: NodeRef, pos: Vector) {
        track.update(PointerAction::Up, pos);
        track.state.reset();
        track.event.is_hover = true;
        track.event.action = PointerAction::Move;
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
        self.hover.observe_mouse(pos);
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = Vector::new(x, y);
        self.hover.observe_mouse(pos);
        self.on_pointer_button_down(PointerId::MOUSE, PointerType::Mouse, button_state, pos);
        self.refresh_hover(true);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let button_state = ButtonState::from_mouse_button(button);
        let pos = Vector::new(x, y);
        self.hover.observe_mouse(pos);
        self.on_pointer_move(PointerId::MOUSE, PointerType::Mouse, pos);
        self.on_pointer_button_up(PointerId::MOUSE, button_state, pos);
        self.refresh_hover(true);
    }

    fn mouse_leave_event(&mut self) {
        self.invalidate_mouse();
    }

    fn mouse_enter_event(&mut self, _button: MouseButton, x: f32, y: f32) {
        // The platform supplies a fresh position; entering is not a button press.
        self.mouse_motion_event(x, y);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::hit_test::tests::TestNode, math::Rect};
    use mue_core::{
        Disposable,
        signal::{Access, signal},
    };

    fn manager(root: NodeRef) -> PointerManager {
        PointerManager {
            root_node: root,
            subscriber_id: 0,
            tracks: HashMap::new(),
            hover: HoverState::default(),
        }
    }
    #[test]
    fn drag_hover_follows_current_position_without_changing_down_path() {
        let left = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let right = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
        let root = TestNode::new(None, &[left.0, right.0]);
        let (a, b) = (signal(false), signal(false));
        Runtime::with(|rt| {
            let mut nodes = rt.nodes.borrow_mut();
            nodes[left.0.id].hovered = Some(a);
            nodes[right.0.id].hovered = Some(b);
        });
        let mut input = manager(root.0);
        input.mouse_motion_event(5., 5.);
        input.mouse_button_down_event(MouseButton::Left, 5., 5.);
        input.mouse_button_down_event(MouseButton::Right, 5., 5.);
        input.mouse_motion_event(35., 5.);
        assert!(!a.get() && b.get());
        let track = &input.tracks[&PointerId::MOUSE];
        assert!(track.nodes.contains(&left.0));
        assert!(!track.nodes.contains(&right.0));
        assert!(
            track
                .event
                .button_state
                .contains(ButtonState::PRIMARY | ButtonState::SECONDARY)
        );
        input.mouse_leave_event();
        assert!(!b.get());
        assert!(!input.tracks.contains_key(&PointerId::MOUSE));
        input.mouse_enter_event(MouseButton::Unknown, 35., 5.);
        assert!(b.get());
        assert!(
            input.tracks[&PointerId::MOUSE]
                .event
                .button_state
                .is_empty()
        );
        a.dispose();
        b.dispose();
    }
    #[test]
    fn touch_does_not_create_hover_and_restore_waits_for_a_fresh_position() {
        let root = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let hovered = signal(false);
        Runtime::with(|rt| rt.nodes.borrow_mut()[root.0.id].hovered = Some(hovered));
        let mut input = manager(root.0);
        input.touch_event(miniquad::TouchPhase::Moved, 1, 5., 5., 0.);
        assert!(input.tracks.is_empty());
        input.touch_event(miniquad::TouchPhase::Started, 1, 5., 5., 0.);
        input.touch_event(miniquad::TouchPhase::Moved, 1, 6., 5., 0.);
        input.touch_event(miniquad::TouchPhase::Ended, 1, 6., 5., 0.);
        assert!(!hovered.get());
        input.mouse_motion_event(5., 5.);
        assert!(hovered.get());
        input.window_minimized_event();
        assert!(!hovered.get());
        input.window_restored_event();
        input.refresh_hover(false);
        assert!(!hovered.get());
        input.mouse_enter_event(MouseButton::Unknown, 5., 5.);
        assert!(hovered.get());
        hovered.dispose();
    }
}
