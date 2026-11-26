//! Back navigation: the Android back button, Escape, and back gestures.
//!
//! Handlers form a stack like Android's `OnBackPressedDispatcher`: a back
//! press goes to the enabled handler that was enabled most recently, so an
//! overlay that just opened handles back before the page beneath it. A
//! handler registered with [`use_back_handler`] lives as long as the current
//! scope.
//!
//! Back presses come from:
//!
//! - the Android back button (and system back gesture);
//! - Escape or the Back key when no focused node handled the key (a focused
//!   control can still use Escape itself);
//! - [`dispatch_back`], for example from an edge-swipe gesture.
//!
//! When no handler is enabled, the fallback runs: on Android it closes the
//! app, as the platform expects; elsewhere it does nothing. Replace it with
//! [`set_back_fallback`].
//!
//! ```
//! use mue_core::prelude::*;
//! use mue_macroquad::back::use_back_handler;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let editing = signal(true);
//! // While editing, back leaves edit mode instead of the page.
//! use_back_handler(editing, move || editing.set(false));
//! # });
//! ```

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::{
    effect::{on_cleanup, watch_effect},
    prop::Prop,
    signal::Access,
};

type Handler = Rc<RefCell<dyn FnMut()>>;

struct Entry {
    id: u64,
    enabled: Prop<bool>,
    /// When the handler was last enabled; the highest enabled one wins.
    order: Rc<Cell<u64>>,
    handler: Handler,
}

thread_local! {
    static HANDLERS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    static COUNTER: Cell<u64> = const { Cell::new(0) };
    static FALLBACK: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

fn next_order() -> u64 {
    COUNTER.with(|counter| {
        counter.set(counter.get() + 1);
        counter.get()
    })
}

/// Handle back presses while `enabled`, for the lifetime of the current scope.
///
/// Each time `enabled` becomes true the handler moves to the top of the stack.
pub fn use_back_handler(enabled: impl Into<Prop<bool>>, handler: impl FnMut() + 'static) {
    let enabled = enabled.into();
    let id = next_order();
    let order = Rc::new(Cell::new(id));
    {
        let order = order.clone();
        watch_effect(move || {
            if enabled.get() {
                order.set(next_order());
            }
        });
    }
    HANDLERS.with(|handlers| {
        handlers.borrow_mut().push(Entry {
            id,
            enabled,
            order,
            handler: Rc::new(RefCell::new(handler)),
        })
    });
    // An effect owned by the current scope unregisters when the scope ends.
    watch_effect(move || {
        on_cleanup(move || {
            let _ = HANDLERS.try_with(|handlers| {
                handlers.borrow_mut().retain(|entry| entry.id != id);
            });
        });
    });
}

/// Deliver a back press. Returns whether a handler (not the fallback) took it.
pub fn dispatch_back() -> bool {
    let handler = HANDLERS.with(|handlers| {
        handlers
            .borrow()
            .iter()
            .filter(|entry| entry.enabled.get_untracked())
            .max_by_key(|entry| entry.order.get())
            .map(|entry| entry.handler.clone())
    });
    match handler {
        Some(handler) => {
            mue_core::batch(|| (handler.borrow_mut())());
            true
        }
        None => {
            let fallback = FALLBACK.with(|fallback| fallback.borrow().clone());
            match fallback {
                Some(fallback) => (fallback.borrow_mut())(),
                None => default_fallback(),
            }
            false
        }
    }
}

/// Replace what an unhandled back press does (by default: close the app on
/// Android, nothing elsewhere).
pub fn set_back_fallback(fallback: impl FnMut() + 'static) {
    FALLBACK.with(|current| *current.borrow_mut() = Some(Rc::new(RefCell::new(fallback))));
}

fn default_fallback() {
    #[cfg(target_os = "android")]
    miniquad::window::order_quit();
}

/// Whether any back handler is enabled now (for showing a back button).
pub fn can_go_back() -> bool {
    HANDLERS.with(|handlers| {
        handlers
            .borrow()
            .iter()
            .any(|entry| entry.enabled.get_untracked())
    })
}
