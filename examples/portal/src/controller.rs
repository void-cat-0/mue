//! Host-side game controller prototype.
//!
//! The controller owns the channel and drives it at frame start. It decides
//! which pointers belong to the chart; it does not own chart time, pause state,
//! or the mue overlay.
//!
//! A mode switch only changes the owner of *new* pointers. Pointers that are
//! already down keep their owner, so a touch held across pause/resume keeps
//! participating in judgement instead of being cancelled by the overlay.

use std::collections::HashMap;

use macroquad::{
    input::{
        is_mouse_button_down, simulate_mouse_with_touch,
        utils::{register_input_subscriber, repeat_all_miniquad_input},
    },
    math::Vec2,
    miniquad::{EventHandler, MouseButton, TouchPhase as MiniquadTouchPhase},
};
use mue_core::prelude::Signal;
use mue_macroquad::{RenderStateGuard, shader::SharedTexture};

/// Reserved pointer id for the mouse.
pub const MOUSE_POINTER: u64 = u64::MAX;

/// Who owns a newly pressed pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    /// The chart owns new pointers, including its own HUD.
    Gameplay,
    /// The modal mue overlay owns new pointers.
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    /// A pointer that was already down when gameplay resumed. The chart must
    /// add it to its touch state and initialize tracking, but must not treat
    /// it as a click.
    Hold,
    Move,
    Up,
    Cancel,
}

/// A pointer event in the chart's coordinate space.
#[derive(Clone, Copy, Debug)]
pub struct ChannelTouch {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
    /// Hardware event time when the host has one. Mouse events are usually
    /// None; the chart falls back to its own time.
    #[allow(dead_code)] // part of the host contract; the mock channel ignores it
    pub time: Option<f64>,
}

impl ChannelTouch {
    fn new(id: u64, phase: TouchPhase, position: Vec2, time: Option<f64>) -> Self {
        Self {
            id,
            phase,
            position,
            time,
        }
    }
}

/// The chart renderer/session adapter.
///
/// The channel owns its render target, camera, effects, and pass. It must
/// restore macroquad state before returning from render.
pub trait GameChannel {
    fn resize(&mut self, size: (u32, u32), dpi: f32);
    fn render(&mut self, time: f64, dt: f32);
    fn touch(&mut self, event: ChannelTouch) -> bool;
    fn texture(&self) -> SharedTexture;
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
    pub fn exit_overlay(&mut self, time: Option<f64>) -> Vec<ChannelTouch> {
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
            events.push(ChannelTouch::new(id, TouchPhase::Hold, position, time));
        }
        events.sort_by_key(|event| event.id);
        events
    }

    pub fn pointer_down(
        &mut self,
        id: u64,
        position: Vec2,
        time: Option<f64>,
    ) -> Option<ChannelTouch> {
        self.active_pointers.insert(id, position);
        if self.mode != InputMode::Gameplay {
            return None;
        }
        self.chart_pointers.insert(id, position);
        Some(ChannelTouch::new(id, TouchPhase::Down, position, time))
    }

    pub fn pointer_move(
        &mut self,
        id: u64,
        position: Vec2,
        time: Option<f64>,
    ) -> Option<ChannelTouch> {
        if let Some(last) = self.active_pointers.get_mut(&id) {
            *last = position;
        }
        let last = self.chart_pointers.get_mut(&id)?;
        *last = position;
        Some(ChannelTouch::new(id, TouchPhase::Move, position, time))
    }

    pub fn pointer_up(
        &mut self,
        id: u64,
        position: Vec2,
        time: Option<f64>,
    ) -> Option<ChannelTouch> {
        self.active_pointers.remove(&id);
        self.chart_pointers.remove(&id)?;
        Some(ChannelTouch::new(id, TouchPhase::Up, position, time))
    }

    #[allow(dead_code)] // used by tests and by hosts that cancel one pointer
    pub fn pointer_cancel(&mut self, id: u64) -> Option<ChannelTouch> {
        self.active_pointers.remove(&id);
        let position = self.chart_pointers.remove(&id)?;
        Some(ChannelTouch::new(id, TouchPhase::Cancel, position, None))
    }

    /// Cancel every chart-owned pointer, e.g. when the window is minimized.
    pub fn cancel_all(&mut self) -> Vec<ChannelTouch> {
        self.active_pointers.clear();
        let mut events: Vec<_> = self
            .chart_pointers
            .drain()
            .map(|(id, position)| ChannelTouch::new(id, TouchPhase::Cancel, position, None))
            .collect();
        events.sort_by_key(|event| event.id);
        events
    }

    #[allow(dead_code)] // used by tests
    pub fn chart_pointer_count(&self) -> usize {
        self.chart_pointers.len()
    }
}

/// Drives one channel at frame start.
///
/// Call begin_frame before App::frame, and feed pointer events before
/// begin_frame. The controller publishes the channel texture through a signal.
pub struct GameController<C: GameChannel> {
    channel: C,
    texture: Signal<SharedTexture>,
    input: ChartInput,
    target_size: (u32, u32),
    subscriber_id: usize,
}

impl<C: GameChannel> GameController<C> {
    pub fn new(channel: C, texture: Signal<SharedTexture>) -> Self {
        // Touch events already carry their own pointer id. Letting macroquad
        // synthesize a mouse stream as well would deliver every finger twice.
        simulate_mouse_with_touch(false);
        Self {
            channel,
            texture,
            input: ChartInput::new(),
            target_size: (0, 0),
            subscriber_id: register_input_subscriber(),
        }
    }

    /// Replay queued input, resize/render the channel, and publish its texture.
    #[allow(dead_code)] // convenience for hosts that do not inspect input before render
    pub fn begin_frame(&mut self, size: (u32, u32), dpi: f32, time: f64, dt: f32) {
        self.process_input();
        self.render(size, dpi, time, dt);
    }

    /// Replay queued miniquad input through the controller.
    ///
    /// Call this before inspecting pause requests or mode changes; the host
    /// can then call render in the same frame.
    pub fn process_input(&mut self) {
        let subscriber_id = self.subscriber_id;
        repeat_all_miniquad_input(self, subscriber_id);
    }

    /// Resize the channel if needed, render it, and publish its texture.
    pub fn render(&mut self, size: (u32, u32), dpi: f32, time: f64, dt: f32) {
        let size = (size.0.max(1), size.1.max(1));
        if size != self.target_size {
            self.target_size = size;
            // Target coordinates change with the surface; cancel held pointers
            // before the channel rebuilds its target.
            self.cancel_all();
            self.channel.resize(size, dpi);
        }

        let guard = RenderStateGuard::enter();
        self.channel.render(time, dt);
        drop(guard);

        self.texture.set(self.channel.texture());
    }

    pub fn pointer_down(&mut self, id: u64, position: Vec2, time: Option<f64>) {
        if let Some(event) = self.input.pointer_down(id, position, time) {
            self.channel.touch(event);
        }
    }

    pub fn pointer_move(&mut self, id: u64, position: Vec2, time: Option<f64>) {
        if let Some(event) = self.input.pointer_move(id, position, time) {
            self.channel.touch(event);
        }
    }

    pub fn pointer_up(&mut self, id: u64, position: Vec2, time: Option<f64>) {
        if let Some(event) = self.input.pointer_up(id, position, time) {
            self.channel.touch(event);
        }
    }

    #[allow(dead_code)] // used by hosts that cancel one pointer
    pub fn pointer_cancel(&mut self, id: u64) {
        if let Some(event) = self.input.pointer_cancel(id) {
            self.channel.touch(event);
        }
    }

    pub fn cancel_all(&mut self) {
        for event in self.input.cancel_all() {
            self.channel.touch(event);
        }
    }

    pub fn enter_overlay(&mut self) {
        self.input.enter_overlay();
    }

    pub fn exit_overlay(&mut self, time: Option<f64>) {
        for event in self.input.exit_overlay(time) {
            self.channel.touch(event);
        }
    }

    /// Close the overlay if it is open. Returns true when the back press was
    /// consumed.
    pub fn back(&mut self, time: Option<f64>) -> bool {
        if self.mode() == InputMode::Overlay {
            self.exit_overlay(time);
            true
        } else {
            false
        }
    }

    pub fn mode(&self) -> InputMode {
        self.input.mode()
    }

    #[allow(dead_code)] // used by tests
    pub fn chart_pointer_count(&self) -> usize {
        self.input.chart_pointer_count()
    }

    pub fn channel_mut(&mut self) -> &mut C {
        &mut self.channel
    }
}

impl<C: GameChannel> EventHandler for GameController<C> {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn touch_event(&mut self, phase: MiniquadTouchPhase, id: u64, x: f32, y: f32, time: f64) {
        let position = Vec2::new(x, y);
        match phase {
            MiniquadTouchPhase::Started => self.pointer_down(id, position, Some(time)),
            MiniquadTouchPhase::Moved => self.pointer_move(id, position, Some(time)),
            MiniquadTouchPhase::Ended => self.pointer_up(id, position, Some(time)),
            MiniquadTouchPhase::Cancelled => self.pointer_cancel(id),
        }
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        if is_mouse_button_down(MouseButton::Left) {
            self.pointer_move(MOUSE_POINTER, Vec2::new(x, y), None);
        }
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.pointer_down(MOUSE_POINTER, Vec2::new(x, y), None);
        }
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.pointer_up(MOUSE_POINTER, Vec2::new(x, y), None);
        }
    }

    fn mouse_leave_event(&mut self) {
        self.pointer_cancel(MOUSE_POINTER);
    }

    fn window_minimized_event(&mut self) {
        self.cancel_all();
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
}
