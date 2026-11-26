//! Reusable reactive helpers (Vue's composables): call them in a component
//! body and use the returned signals and methods.
//!
//! ```
//! use mue_components::prelude::*;
//!
//! #[component]
//! fn details() -> impl Component {
//!     let section = use_disclosure(false);
//!     stack().children((
//!         button("More").on_activate(move || section.toggle()),
//!         presence(section.is_open(), || text("Hidden details")),
//!     ))
//! }
//! ```

use std::{cell::Cell, rc::Rc};

use mue_core::{
    effect::{on_cleanup, watch, watch_effect},
    prelude::*,
};
use mue_macroquad::runtime::{TimeoutHandle, set_timeout};

/// An open/closed state for dialogs, menus, and collapsible sections.
#[derive(Clone, Copy)]
pub struct Disclosure {
    open: Signal<bool>,
}

impl Disclosure {
    pub fn is_open(&self) -> ReadSignal<bool> {
        *self.open
    }

    pub fn signal(&self) -> Signal<bool> {
        self.open
    }

    pub fn open(&self) {
        self.open.set(true);
    }

    pub fn close(&self) {
        self.open.set(false);
    }

    pub fn toggle(&self) {
        self.open.set(!self.open.get_untracked());
    }
}

/// Create a [`Disclosure`] in the current scope.
pub fn use_disclosure(initially_open: bool) -> Disclosure {
    Disclosure {
        open: signal(initially_open),
    }
}

/// A boolean signal with `toggle`; equivalent to a disclosure.
pub fn use_toggle(initial: bool) -> Disclosure {
    use_disclosure(initial)
}

/// Follow `source` after it has stopped changing for `delay` seconds.
///
/// Useful for search fields: the debounced value changes once typing pauses.
/// Timers advance with the App frame loop.
pub fn use_debounced<T: Clone + PartialEq + 'static>(
    source: ReadSignal<T>,
    delay: f32,
) -> ReadSignal<T> {
    let output = signal(source.get_clone_untracked());
    let pending: Rc<Cell<Option<TimeoutHandle>>> = Rc::default();
    watch(source, {
        let pending = pending.clone();
        move |value| {
            if let Some(previous) = pending.take() {
                previous.cancel();
            }
            let value = value.get_clone_untracked();
            pending.set(Some(set_timeout(delay, move || {
                if !output.is_disposed() {
                    output.set(value);
                }
            })));
        }
    });
    watch_effect(move || {
        let pending = pending.clone();
        on_cleanup(move || {
            if let Some(previous) = pending.take() {
                previous.cancel();
            }
        });
    });
    *output
}

/// Call `tick` every `period` seconds while the current scope lives.
pub fn use_interval(period: f32, tick: impl FnMut() + 'static) {
    let alive = Rc::new(Cell::new(true));
    let tick = Rc::new(std::cell::RefCell::new(tick));
    fn schedule(period: f32, alive: Rc<Cell<bool>>, tick: Rc<std::cell::RefCell<dyn FnMut()>>) {
        set_timeout(period.max(0.001), move || {
            if alive.get() {
                (tick.borrow_mut())();
                schedule(period, alive, tick);
            }
        });
    }
    schedule(period, alive.clone(), tick);
    watch_effect(move || {
        let alive = alive.clone();
        on_cleanup(move || alive.set(false));
    });
}

/// The system clipboard (text only). On platforms without a clipboard,
/// `paste` returns `None` and `copy` does nothing.
#[derive(Clone, Copy, Default)]
pub struct Clipboard;

impl Clipboard {
    pub fn copy(&self, text: &str) {
        macroquad::miniquad::window::clipboard_set(text);
    }

    pub fn paste(&self) -> Option<String> {
        macroquad::miniquad::window::clipboard_get()
    }
}

/// Access the system clipboard.
pub fn use_clipboard() -> Clipboard {
    Clipboard
}

/// Which of the ascending width `thresholds` the window has reached; see
/// [`mue_macroquad::viewport::breakpoint`].
pub fn use_breakpoint(thresholds: &[f32]) -> ReadSignal<usize> {
    mue_macroquad::viewport::breakpoint(thresholds)
}
