//! Input policy local to the portal example; not a framework game-input API.
//!
//! The host keeps the input mode and the UI snapshot fixed while both input
//! subscribers consume a batch, then commits pause/resume after UI dispatch.

use macroquad::{
    math::Vec2,
    miniquad::{EventHandler, MouseButton, TouchPhase as MiniquadTouchPhase},
};
use std::collections::HashMap;

pub const MOUSE_POINTER: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Gameplay,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    /// Synchronize a pointer held across resume without judging a click.
    Hold,
    Move,
    Up,
    Cancel,
}

/// Chart event in physical target coordinates.
#[derive(Clone, Copy, Debug)]
pub struct ChartTouch {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
    // The mock ignores hardware timestamps; a real judge would use them.
    #[allow(dead_code)]
    pub time: Option<f64>,
}

impl ChartTouch {
    pub fn new(id: u64, phase: TouchPhase, position: Vec2, time: Option<f64>) -> Self {
        Self {
            id,
            phase,
            position,
            time,
        }
    }
}

/// Raw subscriber events. Positions are physical window pixels, not target pixels.
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    Pointer(ChartTouch),
    CancelAll,
}

/// Collect events without reading macroquad's final mouse-button state.
#[derive(Default)]
pub struct InputEvents {
    events: Vec<InputEvent>,
    mouse_down: bool,
}

impl InputEvents {
    /// A missed host frame invalidates both buffered events and held state.
    pub fn interrupt(&mut self) {
        self.events.clear();
        self.mouse_down = false;
        self.events.push(InputEvent::CancelAll);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = InputEvent> + '_ {
        self.events.drain(..)
    }

    fn pointer(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: Option<f64>) {
        self.events.push(InputEvent::Pointer(ChartTouch::new(
            id,
            phase,
            Vec2::new(x, y),
            time,
        )));
    }
}

impl EventHandler for InputEvents {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn touch_event(&mut self, phase: MiniquadTouchPhase, id: u64, x: f32, y: f32, time: f64) {
        let phase = match phase {
            MiniquadTouchPhase::Started => TouchPhase::Down,
            MiniquadTouchPhase::Moved => TouchPhase::Move,
            MiniquadTouchPhase::Ended => TouchPhase::Up,
            MiniquadTouchPhase::Cancelled => TouchPhase::Cancel,
        };
        self.pointer(id, phase, x, y, Some(time));
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        if self.mouse_down {
            self.pointer(MOUSE_POINTER, TouchPhase::Move, x, y, None);
        }
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.mouse_down = true;
            self.pointer(MOUSE_POINTER, TouchPhase::Down, x, y, None);
        }
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.mouse_down = false;
            self.pointer(MOUSE_POINTER, TouchPhase::Up, x, y, None);
        }
    }

    fn mouse_leave_event(&mut self) {
        self.mouse_down = false;
        self.pointer(MOUSE_POINTER, TouchPhase::Cancel, 0., 0., None);
    }

    fn window_minimized_event(&mut self) {
        self.mouse_down = false;
        self.events.push(InputEvent::CancelAll);
    }
}

/// Per-pointer owner state for the chart.
///
/// mode is the default owner for a new pointer. chart_pointers remembers
/// pointers that started in gameplay; they keep receiving move/up even while
/// the overlay is open.
#[derive(Debug)]
pub struct ChartInput {
    mode: InputMode,
    /// Every pointer currently down, regardless of owner.
    active_pointers: HashMap<u64, Vec2>,
    /// The subset owned by the chart.
    chart_pointers: HashMap<u64, Vec2>,
}

impl ChartInput {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Gameplay,
            active_pointers: HashMap::new(),
            chart_pointers: HashMap::new(),
        }
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    /// New pointers now belong to the overlay. Existing chart pointers keep
    /// their owner and are not cancelled.
    pub fn enter_overlay(&mut self) {
        self.mode = InputMode::Overlay;
    }

    /// New pointers now belong to the chart. Pointers that are still down but
    /// started in the overlay are synced as Hold events, not Down: the chart
    /// must track them for flick/drag but must not judge a click.
    pub fn exit_overlay(&mut self, time: Option<f64>) -> Vec<ChartTouch> {
        if self.mode == InputMode::Gameplay {
            return Vec::new();
        }
        self.mode = InputMode::Gameplay;

        let held: Vec<(u64, Vec2)> = self
            .active_pointers
            .iter()
            .filter(|(id, _)| !self.chart_pointers.contains_key(id))
            .map(|(id, position)| (*id, *position))
            .collect();

        let mut events = Vec::with_capacity(held.len());
        for (id, position) in held {
            self.chart_pointers.insert(id, position);
            events.push(ChartTouch::new(id, TouchPhase::Hold, position, time));
        }
        events.sort_by_key(|event| event.id);
        events
    }

    pub fn pointer_down(
        &mut self,
        id: u64,
        position: Vec2,
        time: Option<f64>,
    ) -> Option<ChartTouch> {
        self.active_pointers.insert(id, position);
        if self.mode != InputMode::Gameplay {
            return None;
        }
        self.chart_pointers.insert(id, position);
        Some(ChartTouch::new(id, TouchPhase::Down, position, time))
    }

    pub fn pointer_move(
        &mut self,
        id: u64,
        position: Vec2,
        time: Option<f64>,
    ) -> Option<ChartTouch> {
        if let Some(last) = self.active_pointers.get_mut(&id) {
            *last = position;
        }
        let last = self.chart_pointers.get_mut(&id)?;
        *last = position;
        Some(ChartTouch::new(id, TouchPhase::Move, position, time))
    }

    pub fn pointer_up(&mut self, id: u64, position: Vec2, time: Option<f64>) -> Option<ChartTouch> {
        self.active_pointers.remove(&id);
        self.chart_pointers.remove(&id)?;
        Some(ChartTouch::new(id, TouchPhase::Up, position, time))
    }

    pub fn pointer_cancel(&mut self, id: u64) -> Option<ChartTouch> {
        self.active_pointers.remove(&id);
        let position = self.chart_pointers.remove(&id)?;
        Some(ChartTouch::new(id, TouchPhase::Cancel, position, None))
    }

    /// Cancel every chart-owned pointer, e.g. when the window is minimized.
    pub fn cancel_all(&mut self) -> Vec<ChartTouch> {
        self.active_pointers.clear();
        let mut events: Vec<_> = self
            .chart_pointers
            .drain()
            .map(|(id, position)| ChartTouch::new(id, TouchPhase::Cancel, position, None))
            .collect();
        events.sort_by_key(|event| event.id);
        events
    }

    #[cfg(test)]
    pub fn chart_pointer_count(&self) -> usize {
        self.chart_pointers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pos(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }
    #[test]
    fn gameplay_routes_new_pointer_to_chart() {
        let mut input = ChartInput::new();
        let down = input.pointer_down(1, pos(10., 10.), None).unwrap();
        assert_eq!(down.phase, TouchPhase::Down);
        let moved = input.pointer_move(1, pos(20., 20.), None).unwrap();
        assert_eq!(moved.phase, TouchPhase::Move);
        assert_eq!(moved.position, pos(20., 20.));
        let up = input.pointer_up(1, pos(20., 20.), None).unwrap();
        assert_eq!(up.phase, TouchPhase::Up);
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn held_chart_pointer_survives_overlay() {
        let mut input = ChartInput::new();
        input.pointer_down(1, pos(10., 10.), None).unwrap();
        input.enter_overlay();

        let moved = input
            .pointer_move(1, pos(20., 20.), None)
            .expect("a held chart pointer keeps its owner");
        assert_eq!(moved.phase, TouchPhase::Move);

        let up = input
            .pointer_up(1, pos(20., 20.), None)
            .expect("a held chart pointer receives its up");
        assert_eq!(up.phase, TouchPhase::Up);
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn overlay_pointer_does_not_reach_chart() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        assert!(input.pointer_down(2, pos(1., 1.), None).is_none());
        assert!(input.pointer_move(2, pos(2., 2.), None).is_none());
        assert!(input.pointer_up(2, pos(2., 2.), None).is_none());

        input.exit_overlay(None);
        assert!(input.pointer_down(3, pos(3., 3.), None).is_some());
    }

    #[test]
    fn chart_and_overlay_pointers_are_independent() {
        let mut input = ChartInput::new();
        assert!(input.pointer_down(1, pos(10., 10.), None).is_some());
        input.enter_overlay();
        assert!(input.pointer_down(2, pos(20., 20.), None).is_none());
        assert!(input.pointer_move(1, pos(11., 11.), None).is_some());
        assert!(input.pointer_move(2, pos(21., 21.), None).is_none());
        assert!(input.pointer_up(1, pos(11., 11.), None).is_some());
        assert!(input.pointer_up(2, pos(21., 21.), None).is_none());
    }

    #[test]
    fn cancel_releases_owner() {
        let mut input = ChartInput::new();
        input.pointer_down(1, pos(10., 10.), None).unwrap();
        let cancel = input.pointer_cancel(1).expect("cancel");
        assert_eq!(cancel.phase, TouchPhase::Cancel);
        assert_eq!(cancel.position, pos(10., 10.));
        assert!(input.pointer_move(1, pos(20., 20.), None).is_none());
    }

    #[test]
    fn cancel_all_drains_chart_pointers() {
        let mut input = ChartInput::new();
        input.pointer_down(1, pos(10., 10.), None).unwrap();
        input.pointer_down(2, pos(20., 20.), None).unwrap();
        input.enter_overlay();

        let events = input.cancel_all();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 1);
        assert_eq!(events[1].id, 2);
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn overlay_down_is_swallowed_but_synced_as_hold_on_exit() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        assert!(input.pointer_down(7, pos(10., 10.), None).is_none());
        assert_eq!(input.chart_pointer_count(), 0);

        let events = input.exit_overlay(Some(1.5));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, 7);
        assert_eq!(events[0].phase, TouchPhase::Hold);
        assert_ne!(events[0].phase, TouchPhase::Down);
        assert_eq!(events[0].position, pos(10., 10.));
        assert_eq!(events[0].time, Some(1.5));
        assert_eq!(input.chart_pointer_count(), 1);

        let moved = input.pointer_move(7, pos(20., 20.), None).unwrap();
        assert_eq!(moved.phase, TouchPhase::Move);
        let up = input.pointer_up(7, pos(20., 20.), None).unwrap();
        assert_eq!(up.phase, TouchPhase::Up);
    }

    #[test]
    fn overlay_down_released_before_resume_is_not_synced() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        assert!(input.pointer_down(8, pos(1., 1.), None).is_none());
        assert!(input.pointer_up(8, pos(1., 1.), None).is_none());

        let events = input.exit_overlay(Some(2.0));
        assert!(events.is_empty());
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn held_chart_pointer_is_not_reemitted_on_exit() {
        let mut input = ChartInput::new();
        input.pointer_down(9, pos(3., 3.), None).unwrap();
        input.enter_overlay();

        let events = input.exit_overlay(Some(3.0));
        assert!(events.is_empty());
        assert_eq!(input.chart_pointer_count(), 1);
    }

    #[test]
    fn exit_overlay_syncs_every_held_pointer() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        input.pointer_down(3, pos(30., 30.), None);
        input.pointer_down(1, pos(10., 10.), None);

        let events = input.exit_overlay(Some(4.0));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 1);
        assert_eq!(events[1].id, 3);
        assert!(events.iter().all(|event| event.phase == TouchPhase::Hold));
    }

    #[test]
    fn cancel_all_clears_held_overlay_pointers() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        input.pointer_down(4, pos(4., 4.), None);
        input.cancel_all();

        let events = input.exit_overlay(Some(5.0));
        assert!(events.is_empty());
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn chart_pointers_are_independent() {
        let mut input = ChartInput::new();
        assert!(input.pointer_down(1, pos(1., 1.), None).is_some());
        assert!(input.pointer_down(2, pos(2., 2.), None).is_some());

        assert_eq!(
            input.pointer_move(1, pos(3., 3.), None).unwrap().position,
            pos(3., 3.)
        );
        assert_eq!(
            input.pointer_move(2, pos(4., 4.), None).unwrap().position,
            pos(4., 4.)
        );

        assert!(input.pointer_up(1, pos(3., 3.), None).is_some());
        assert!(input.pointer_move(2, pos(5., 5.), None).is_some());
        assert!(input.pointer_up(2, pos(5., 5.), None).is_some());
        assert_eq!(input.chart_pointer_count(), 0);
    }

    #[test]
    fn exit_overlay_holds_only_overlay_pointers() {
        let mut input = ChartInput::new();
        input.pointer_down(1, pos(1., 1.), None).unwrap();
        input.enter_overlay();
        input.pointer_down(2, pos(2., 2.), None);
        input.pointer_down(3, pos(3., 3.), None);

        let events = input.exit_overlay(Some(9.0));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 2);
        assert_eq!(events[1].id, 3);
        assert!(events.iter().all(|event| event.phase == TouchPhase::Hold));
        assert_eq!(input.chart_pointer_count(), 3);
    }

    #[test]
    fn cancelled_overlay_pointer_is_not_synced() {
        let mut input = ChartInput::new();
        input.enter_overlay();
        input.pointer_down(1, pos(1., 1.), None);
        input.pointer_down(2, pos(2., 2.), None);
        input.pointer_cancel(1);

        let events = input.exit_overlay(Some(1.0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, 2);
        assert_eq!(events[0].phase, TouchPhase::Hold);
    }

    #[test]
    fn pointer_id_reuse_starts_a_new_touch() {
        let mut input = ChartInput::new();
        input.pointer_down(1, pos(1., 1.), None).unwrap();
        input.pointer_up(1, pos(1., 1.), None).unwrap();

        let down = input.pointer_down(1, pos(2., 2.), None).unwrap();
        assert_eq!(down.phase, TouchPhase::Down);
        assert_eq!(input.chart_pointer_count(), 1);
    }

    #[test]
    fn mouse_move_is_preserved_in_a_batch_ending_with_release() {
        let mut events = InputEvents::default();
        events.mouse_button_down_event(MouseButton::Left, 1., 2.);
        events.mouse_motion_event(3., 4.);
        events.mouse_button_up_event(MouseButton::Left, 5., 6.);
        let phases: Vec<_> = events
            .drain()
            .map(|event| match event {
                InputEvent::Pointer(event) => event.phase,
                InputEvent::CancelAll => panic!("unexpected cancel"),
            })
            .collect();
        assert_eq!(phases, [TouchPhase::Down, TouchPhase::Move, TouchPhase::Up]);
    }

    #[test]
    fn interruption_drops_buffered_clicks_and_held_mouse_before_current_events() {
        let mut events = InputEvents::default();
        events.mouse_button_down_event(MouseButton::Left, 1., 2.);
        events.interrupt();
        events.mouse_motion_event(3., 4.);
        assert!(matches!(
            events.drain().collect::<Vec<_>>().as_slice(),
            [InputEvent::CancelAll]
        ));
        events.mouse_button_down_event(MouseButton::Left, 5., 6.);
        assert!(
            matches!(events.drain().collect::<Vec<_>>().as_slice(), [InputEvent::Pointer(event)] if event.phase == TouchPhase::Down)
        );
    }

    #[test]
    fn mouse_move_survives_release_in_the_next_batch() {
        let mut events = InputEvents::default();
        events.mouse_button_down_event(MouseButton::Left, 1., 2.);
        events.drain().for_each(drop);
        events.mouse_motion_event(3., 4.);
        events.mouse_button_up_event(MouseButton::Left, 5., 6.);
        let phases: Vec<_> = events
            .drain()
            .map(|event| match event {
                InputEvent::Pointer(event) => event.phase,
                InputEvent::CancelAll => panic!("unexpected cancel"),
            })
            .collect();
        assert_eq!(phases, [TouchPhase::Move, TouchPhase::Up]);
        events.mouse_motion_event(7., 8.);
        assert_eq!(events.drain().count(), 0);
    }
}
