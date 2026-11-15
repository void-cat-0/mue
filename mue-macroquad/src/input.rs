//! Shared raw input for UI and host integrations.
//!
//! One legacy macroquad subscriber is retained on the host thread. Consumers
//! hold only cursors into the current frame, so dropping or pausing a consumer
//! leaves no queue behind. A macroquad coroutine expires the batch at frame end,
//! including frames without an App. This module does not change pointer owners,
//! coordinates, timestamps, or the stage at which consumers dispatch input.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use macroquad::{
    experimental::coroutines::start_coroutine,
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
};
use miniquad::{EventHandler, KeyCode, KeyMods, MouseButton, TouchPhase};

// Keep the adapter private: callers consume the existing EventHandler contract.
macro_rules! input_events {
    ($($variant:ident => $callback:ident($($arg:ident: $ty:ty),*)),* $(,)?) => {
        #[derive(Clone, Debug)]
        pub(crate) enum InputEvent {
            $($variant { $($arg: $ty),* }),*
        }

        impl InputEvent {
            pub(crate) fn repeat(&self, handler: &mut impl EventHandler) {
                match *self {
                    $(Self::$variant { $($arg),* } => handler.$callback($($arg),*)),*
                }
            }
        }

        #[derive(Default)]
        struct Collector(Vec<InputEvent>);

        impl EventHandler for Collector {
            fn update(&mut self) {}
            fn draw(&mut self) {}
            $(fn $callback(&mut self, $($arg: $ty),*) {
                self.0.push(InputEvent::$variant { $($arg),* });
            })*
        }
    };
}

input_events! {
    MouseMotion => mouse_motion_event(x: f32, y: f32),
    MouseWheel => mouse_wheel_event(x: f32, y: f32),
    MouseDown => mouse_button_down_event(button: MouseButton, x: f32, y: f32),
    MouseUp => mouse_button_up_event(button: MouseButton, x: f32, y: f32),
    MouseLeave => mouse_leave_event(),
    MouseEnter => mouse_enter_event(button: MouseButton, x: f32, y: f32),
    Touch => touch_event(phase: TouchPhase, id: u64, x: f32, y: f32, time: f64),
    Char => char_event(character: char, modifiers: KeyMods, repeat: bool),
    KeyDown => key_down_event(keycode: KeyCode, modifiers: KeyMods, repeat: bool),
    KeyUp => key_up_event(keycode: KeyCode, modifiers: KeyMods),
    WindowMinimized => window_minimized_event(),
    WindowRestored => window_restored_event(),
}

#[derive(Default)]
struct FrameInput {
    generation: u64,
    events: Rc<Vec<InputEvent>>,
}

impl FrameInput {
    fn append(&mut self, collector: &mut Collector) {
        if !collector.0.is_empty() {
            Rc::make_mut(&mut self.events).append(&mut collector.0);
        }
    }

    fn finish(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        // Reuse storage unless a caller explicitly retains an old InputBatch.
        if let Some(events) = Rc::get_mut(&mut self.events) {
            events.clear();
        } else {
            self.events = Rc::default();
        }
    }

    fn subscribe(&self) -> InputSubscriber {
        InputSubscriber {
            generation: self.generation,
            offset: self.events.len(),
            _host_thread: std::marker::PhantomData,
        }
    }

    fn read(&self, subscriber: &mut InputSubscriber) -> InputBatch {
        let same_frame = subscriber.generation == self.generation;
        let interrupted = !same_frame && subscriber.generation.wrapping_add(1) != self.generation;
        let start = if same_frame { subscriber.offset } else { 0 };
        subscriber.generation = self.generation;
        subscriber.offset = self.events.len();
        InputBatch {
            events: self.events.clone(),
            start,
            end: self.events.len(),
            interrupted,
        }
    }
}

struct InputSource {
    subscriber: usize,
    collector: Collector,
    frame: FrameInput,
}

impl InputSource {
    fn new() -> Self {
        start_pump();
        Self {
            subscriber: register_input_subscriber(),
            collector: Collector::default(),
            frame: FrameInput::default(),
        }
    }

    fn collect(&mut self) {
        // Legacy replay holds a macroquad context borrow. Only our plain data
        // collector runs there; all application callbacks run afterwards.
        repeat_all_miniquad_input(&mut self.collector, self.subscriber);
        self.frame.append(&mut self.collector);
    }

    fn finish(&mut self) {
        // Drain even when every consumer has gone away or stopped reading.
        repeat_all_miniquad_input(&mut self.collector, self.subscriber);
        self.collector.0.clear();
        self.frame.finish();
    }

    fn resume_pump(&mut self) {
        if !PUMP_ALIVE.get() {
            // stop_all_coroutines (also scene::clear) can stop the pump. Do not
            // replay input accumulated while it was stopped to a new view.
            self.finish();
            self.frame.finish();
            start_pump();
        }
    }
}

thread_local! {
    static INPUT: RefCell<Option<InputSource>> = const { RefCell::new(None) };
    static PUMP_ALIVE: Cell<bool> = const { Cell::new(false) };
}

struct PumpLifetime;

impl Drop for PumpLifetime {
    fn drop(&mut self) {
        let _ = PUMP_ALIVE.try_with(|alive| alive.set(false));
    }
}

fn start_pump() {
    PUMP_ALIVE.set(true);
    // Capture the guard before the first poll so stop_all_coroutines is also
    // detected when called in the same frame as initialization.
    let lifetime = PumpLifetime;
    start_coroutine(async move {
        let _lifetime = lifetime;
        loop {
            // Coroutines run after the main future yields, before end_frame.
            // Clean on the very first poll as well as every subsequent frame.
            INPUT.with(|input| {
                if let Some(source) = input.borrow_mut().as_mut() {
                    source.finish();
                }
            });
            macroquad::window::next_frame().await;
        }
    });
}

fn with_source<R>(f: impl FnOnce(&mut InputSource) -> R) -> R {
    INPUT.with(|input| {
        let mut input = input.borrow_mut();
        let source = input.get_or_insert_with(InputSource::new);
        source.resume_pump();
        f(source)
    })
}

/// A host-thread cursor into mue's shared raw event stream.
///
/// Create after macroquad starts. No per-consumer queue or underlying macroquad
/// subscription is allocated. New cursors ignore events preceding construction;
/// dropping one requires no host context and does not affect other consumers.
/// Consume once per host frame. Unread events expire at frame end instead of
/// accumulating while a UI is inactive. One macroquad host per thread is assumed.
/// Call new/read from the main application future (or its synchronous callbacks),
/// before next_frame, not from automatic macroquad coroutines: the frame-expiry
/// coroutine runs after that future yields. If stop_all_coroutines clears it, the
/// next new/read discards stale input and restarts it.
pub struct InputSubscriber {
    generation: u64,
    offset: usize,
    _host_thread: std::marker::PhantomData<Rc<()>>,
}

impl InputSubscriber {
    pub fn new() -> Self {
        with_source(|source| {
            source.collect();
            source.frame.subscribe()
        })
    }

    /// Take events received since this cursor's previous read in this frame.
    ///
    /// After a skipped frame, the returned batch reports an interruption: cancel
    /// local pointer/pressed state before dispatching the current events. Reading
    /// a batch releases all internal borrows before any application callbacks.
    pub fn read(&mut self) -> InputBatch {
        with_source(|source| {
            source.collect();
            source.frame.read(self)
        })
    }
}

impl Default for InputSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

/// An immutable raw event batch, independent of host-context and queue borrows.
/// Positions remain physical window pixels and touch timestamps are unchanged.
/// Retain only as long as needed to dispatch this frame's input.
#[derive(Default)]
pub struct InputBatch {
    events: Rc<Vec<InputEvent>>,
    start: usize,
    end: usize,
    interrupted: bool,
}

impl InputBatch {
    #[cfg(test)]
    /// Build a recorded test batch, preserving its interruption status.
    pub(crate) fn from_events(events: Vec<InputEvent>, interrupted: bool) -> Self {
        let end = events.len();
        Self {
            events: Rc::new(events),
            start: 0,
            end,
            interrupted,
        }
    }

    pub(crate) fn events(&self) -> &[InputEvent] {
        &self.events[self.start..self.end]
    }

    #[cfg(test)]
    pub(crate) fn recorded(record: impl FnOnce(&mut dyn EventHandler)) -> Self {
        let mut collector = Collector::default();
        record(&mut collector);
        let end = collector.0.len();
        Self {
            events: Rc::new(collector.0),
            start: 0,
            end,
            interrupted: false,
        }
    }

    /// Whether one or more complete host frames were skipped by this consumer.
    pub fn is_interrupted(&self) -> bool {
        self.interrupted
    }

    #[cfg(test)]
    pub(crate) fn interrupted() -> Self {
        Self {
            interrupted: true,
            ..Default::default()
        }
    }

    /// Dispatch this batch in original order. The caller handles interruption
    /// first; no UI lifecycle or game-specific cancellation is synthesized here.
    pub fn repeat(&self, handler: &mut impl EventHandler) {
        for event in &self.events[self.start..self.end] {
            event.repeat(handler);
        }
    }
}

#[cfg(test)]
pub(crate) fn inject(record: impl FnOnce(&mut dyn EventHandler)) {
    with_source(|source| {
        let mut collector = Collector::default();
        record(&mut collector);
        source.frame.append(&mut collector);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_batch(frame: &mut FrameInput) {
        let mut collector = Collector::default();
        collector.mouse_button_down_event(MouseButton::Left, 10., 20.);
        collector.mouse_motion_event(11., 21.);
        collector.mouse_button_up_event(MouseButton::Left, 12., 22.);
        frame.append(&mut collector);
    }

    #[test]
    fn consumers_share_ordered_events_without_reading_final_button_state() {
        let mut frame = FrameInput::default();
        let mut ui = frame.subscribe();
        let mut game = frame.subscribe();
        mouse_batch(&mut frame);
        let mut ui_events = Collector::default();
        let mut game_events = Collector::default();
        frame.read(&mut ui).repeat(&mut ui_events);
        frame.read(&mut game).repeat(&mut game_events);
        assert_eq!(format!("{:?}", ui_events.0), format!("{:?}", game_events.0));
        assert!(matches!(
            ui_events.0.as_slice(),
            [
                InputEvent::MouseDown { .. },
                InputEvent::MouseMotion { .. },
                InputEvent::MouseUp { .. }
            ]
        ));
        frame.read(&mut ui).repeat(&mut ui_events);
        assert_eq!(ui_events.0.len(), 3);
    }

    #[test]
    fn new_consumer_does_not_replay_events_from_a_disposed_view() {
        let mut frame = FrameInput::default();
        let old = frame.subscribe();
        mouse_batch(&mut frame);
        drop(old);
        let mut new = frame.subscribe();
        let mut events = Collector::default();
        frame.read(&mut new).repeat(&mut events);
        assert!(events.0.is_empty());
        mouse_batch(&mut frame);
        frame.read(&mut new).repeat(&mut events);
        assert_eq!(events.0.len(), 3);
    }

    #[test]
    fn inactive_consumer_loses_old_events_and_reports_interruption() {
        let mut frame = FrameInput::default();
        let mut subscriber = frame.subscribe();
        mouse_batch(&mut frame);
        frame.finish();
        mouse_batch(&mut frame);
        frame.finish();
        assert!(frame.events.is_empty());
        let mut current = Collector::default();
        current.touch_event(TouchPhase::Started, 7, 2., 3., 123.456);
        frame.append(&mut current);
        let batch = frame.read(&mut subscriber);
        assert!(batch.is_interrupted());
        batch.repeat(&mut current);
        assert!(matches!(
            current.0.as_slice(),
            [InputEvent::Touch {
                phase: TouchPhase::Started,
                id: 7,
                x: 2.,
                y: 3.,
                time: 123.456
            }]
        ));
        frame.finish();
        assert!(!frame.read(&mut subscriber).is_interrupted());
    }

    #[test]
    fn replay_can_read_or_subscribe_and_preserves_events_added_during_callbacks() {
        struct Receiver<'a> {
            frame: &'a RefCell<FrameInput>,
            calls: usize,
        }
        impl EventHandler for Receiver<'_> {
            fn update(&mut self) {}
            fn draw(&mut self) {}
            fn mouse_leave_event(&mut self) {
                let mut frame = self.frame.borrow_mut();
                let mut nested = frame.subscribe();
                assert!(!frame.read(&mut nested).is_interrupted());
                let mut collector = Collector::default();
                collector.window_restored_event();
                frame.append(&mut collector);
                self.calls += 1;
            }
            fn window_restored_event(&mut self) {
                self.calls += 1;
            }
        }
        let frame = RefCell::new(FrameInput::default());
        let mut subscriber = frame.borrow().subscribe();
        let mut collector = Collector::default();
        collector.mouse_leave_event();
        frame.borrow_mut().append(&mut collector);
        let batch = frame.borrow().read(&mut subscriber);
        let mut receiver = Receiver {
            frame: &frame,
            calls: 0,
        };
        batch.repeat(&mut receiver);
        assert_eq!(receiver.calls, 1);
        let next = frame.borrow().read(&mut subscriber);
        next.repeat(&mut receiver);
        assert_eq!(receiver.calls, 2);
    }

    #[test]
    fn adapter_preserves_keyboard_focus_and_touch_fields() {
        let mut collector = Collector::default();
        let modifiers = KeyMods {
            shift: true,
            ctrl: true,
            alt: false,
            logo: false,
        };
        collector.key_down_event(KeyCode::A, modifiers, true);
        collector.char_event('界', modifiers, false);
        collector.key_up_event(KeyCode::A, modifiers);
        collector.mouse_enter_event(MouseButton::Unknown, 1., 2.);
        collector.mouse_wheel_event(3., 4.);
        collector.window_minimized_event();
        collector.window_restored_event();
        collector.touch_event(TouchPhase::Cancelled, 42, 5., 6., 789.25);
        let mut replay = Collector::default();
        for event in &collector.0 {
            event.repeat(&mut replay);
        }
        assert_eq!(format!("{:?}", collector.0), format!("{:?}", replay.0));
        assert!(matches!(
            replay.0[0],
            InputEvent::KeyDown {
                keycode: KeyCode::A,
                modifiers: KeyMods {
                    shift: true,
                    ctrl: true,
                    ..
                },
                repeat: true
            }
        ));
        assert!(matches!(
            replay.0[3],
            InputEvent::MouseEnter {
                button: MouseButton::Unknown,
                ..
            }
        ));
        assert!(matches!(
            replay.0[7],
            InputEvent::Touch {
                phase: TouchPhase::Cancelled,
                id: 42,
                time: 789.25,
                ..
            }
        ));
    }

    #[test]
    #[ignore = "requires a graphics display; exercises real macroquad frame scheduling"]
    fn host_frame_pump_expires_batches_and_recovers_after_global_coroutine_stop() {
        macroquad::Window::new("mue input lifecycle test", async {
            // An unpolled pump must still notice stop_all_coroutines.
            let before = register_input_subscriber();
            let mut subscriber = InputSubscriber::new();
            macroquad::experimental::coroutines::stop_all_coroutines();
            assert!(!PUMP_ALIVE.get());
            assert!(subscriber.read().is_interrupted());
            assert!(PUMP_ALIVE.get());
            let generation = INPUT.with(|input| input.borrow().as_ref().unwrap().frame.generation);
            macroquad::window::next_frame().await;
            assert_eq!(
                INPUT.with(|input| input.borrow().as_ref().unwrap().frame.generation),
                generation + 1
            );
            assert!(!subscriber.read().is_interrupted());
            let mut collector = Collector::default();
            collector.mouse_button_down_event(MouseButton::Left, 1., 2.);
            INPUT.with(|input| {
                input
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .frame
                    .append(&mut collector)
            });
            let batch = subscriber.read();
            let mut replay = Collector::default();
            batch.repeat(&mut replay);
            assert_eq!(replay.0.len(), 1);
            drop(batch);
            drop(subscriber);
            // Zero consumers still get frame expiry and a single source slot.
            for _ in 0..3 {
                macroquad::window::next_frame().await;
            }
            assert!(INPUT.with(|input| input.borrow().as_ref().unwrap().frame.events.is_empty()));
            let mut next = InputSubscriber::new();
            let after = register_input_subscriber();
            assert_eq!(after, before + 2);
            let mut replay = Collector::default();
            next.read().repeat(&mut replay);
            assert!(replay.0.is_empty());
            macroquad::window::next_frame().await;
            macroquad::window::next_frame().await;
            assert!(next.read().is_interrupted());
            macroquad::experimental::coroutines::stop_all_coroutines();
            assert!(next.read().is_interrupted());
            macroquad::window::next_frame().await;
            assert!(!next.read().is_interrupted());
        });
    }

    #[test]
    fn held_batch_is_immutable_across_frame_expiry() {
        let mut frame = FrameInput::default();
        let mut subscriber = frame.subscribe();
        mouse_batch(&mut frame);
        let batch = frame.read(&mut subscriber);
        frame.finish();
        let mut events = Collector::default();
        batch.repeat(&mut events);
        assert_eq!(events.0.len(), 3);
        assert!(frame.events.is_empty());
    }
}
