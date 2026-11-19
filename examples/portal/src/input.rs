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
}
