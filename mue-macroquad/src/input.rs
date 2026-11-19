//! Shared raw input for UI and host integrations.
//!
//! One legacy macroquad subscriber is retained on the host thread. Consumers
//! hold only cursors into the current frame, so dropping or pausing a consumer
//! leaves no queue behind. A macroquad coroutine expires the batch at frame end,
//! including frames without an App. This module does not change pointer owners,
//! coordinates, timestamps, or the stage at which consumers dispatch input.
//!
//! [`InputSubscriber`] is for a host integration that needs the same raw events
//! as the UI runtime. Construct it after macroquad starts and call
//! [`InputSubscriber::read`] from the main application future or a synchronous
//! callback, before yielding to [`macroquad::window::next_frame`]. A new
//! subscriber starts at the current end of the stream. Unread events expire at
//! frame end; consumers do not get a private replay queue.
//!
//! [`App`](crate::App) handles input automatically. In a custom
//! [`Scene`](crate::Scene), [`SceneEvent::Input`](crate::SceneEvent::Input)
//! receives the complete raw [`InputBatch`] after Node UI dispatch. UI hit-testing
//! and capture are separate from game/contact ownership. When replaying a batch,
//! check [`InputBatch::is_interrupted`] first and cancel local pressed state as
//! needed; [`InputBatch::repeat`] preserves event order and reports source DPI
//! metadata without rescaling positions.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use macroquad::{
    experimental::coroutines::start_coroutine,
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
};
use miniquad::{EventHandler, KeyCode, KeyMods, MouseButton, TouchPhase};

pub(crate) fn valid_dpi_scale(scale: f32) -> Option<f32> {
    (scale.is_finite() && scale > 0.).then_some(scale.max(f32::EPSILON))
}

#[derive(Clone, Debug)]
pub(crate) struct InputSample {
    pub event: InputEvent,
    pub dpi_scale: Option<f32>,
}

impl InputSample {
    pub(crate) fn repeat(&self, handler: &mut impl EventHandler) {
        if let Some(scale) = self.dpi_scale {
            handler.input_event_dpi_scale(scale);
        }
        self.event.repeat(handler);
    }
}

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
        struct Collector {
            events: Vec<InputSample>,
            next_dpi_scale: Option<f32>,
        }

        impl EventHandler for Collector {
            fn update(&mut self) {}
            fn draw(&mut self) {}

            fn input_event_dpi_scale(&mut self, scale: f32) {
                self.next_dpi_scale = valid_dpi_scale(scale);
            }

            $(fn $callback(&mut self, $($arg: $ty),*) {
                self.events.push(InputSample {
                    event: InputEvent::$variant { $($arg),* },
                    dpi_scale: self.next_dpi_scale.take(),
                });
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
    events: Rc<Vec<InputSample>>,
}

impl FrameInput {
    fn append(&mut self, collector: &mut Collector) {
        if !collector.events.is_empty() {
            Rc::make_mut(&mut self.events).append(&mut collector.events);
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
        self.collector.events.clear();
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
    events: Rc<Vec<InputSample>>,
    start: usize,
    end: usize,
    interrupted: bool,
}

impl InputBatch {
    pub(crate) fn events(&self) -> &[InputSample] {
        &self.events[self.start..self.end]
    }

    /// Whether one or more complete host frames were skipped by this consumer.
    pub fn is_interrupted(&self) -> bool {
        self.interrupted
    }

    /// Dispatch this batch in original order. The caller handles interruption
    /// first; no UI lifecycle or game-specific cancellation is synthesized here.
    /// When available, the event's source DPI scale is delivered as metadata
    /// immediately before the raw callback. Positions are not rescaled.
    pub fn repeat(&self, handler: &mut impl EventHandler) {
        for event in &self.events[self.start..self.end] {
            event.repeat(handler);
        }
    }
}
