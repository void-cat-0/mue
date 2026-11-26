//! Mouse hover tracking.
//!
//! Touch input never hovers. A touch also clears any hover left by an earlier
//! mouse position, and mouse events that arrive just after a touch (emulated
//! by some platforms, such as Windows touch screens and browsers) do not
//! hover either, so `&hover` styles do not stick after a tap (MB2).
//! [`hover_capable`] reports whether the last pointer could hover.

use std::cell::Cell;

use indexmap::IndexSet;
use mue_core::{
    batch,
    scope::Scope,
    signal::{Access, ReadSignal, Signal},
};

use crate::{
    app::runtime::Runtime,
    math::{Rect, Vector},
    node::NodeRef,
};

use super::hit_test::hit_test;

/// Seconds after a touch during which mouse events are treated as emulated.
const TOUCH_MOUSE_GRACE: f64 = 0.8;

thread_local! {
    static HOVER_CAPABLE: Signal<bool> = Scope::null().run(|| {
        Signal::new(!cfg!(any(target_os = "android", target_os = "ios")))
    });
    static LAST_TOUCH: Cell<f64> = const { Cell::new(f64::NEG_INFINITY) };
}

/// Whether the most recent pointer could hover (a mouse or pen rather than a
/// finger), like the CSS `(hover: hover)` media feature. Tracked. Starts
/// true on desktop platforms and false on Android and iOS.
pub fn hover_capable() -> ReadSignal<bool> {
    HOVER_CAPABLE.with(|signal| **signal)
}

fn set_hover_capable(capable: bool) {
    HOVER_CAPABLE.with(|signal| {
        if signal.get_untracked() != capable {
            signal.set(capable);
        }
    });
}

/// Record a touch: clears hover and starts the emulated-mouse grace period.
pub(crate) fn note_touch() {
    LAST_TOUCH.with(|last| last.set(miniquad::date::now()));
    set_hover_capable(false);
}

fn mouse_is_emulated() -> bool {
    LAST_TOUCH.with(|last| miniquad::date::now() - last.get() < TOUCH_MOUSE_GRACE)
}

/// Mouse location is independent of the pressed pointer's captured down path.
#[derive(Clone)]
pub(crate) struct HoverState {
    position: Option<Vector>,
    focused: bool,
    outside: bool,
    window_bounds: Option<Rect>,
    path: IndexSet<NodeRef>,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            position: None,
            focused: true,
            outside: false,
            window_bounds: None,
            path: IndexSet::new(),
        }
    }
}

impl HoverState {
    pub(crate) fn set_window_bounds(&mut self, bounds: Option<Rect>) {
        self.window_bounds = bounds.filter(|bounds| {
            bounds.min().is_finite() && bounds.max().is_finite() && bounds.w > 0. && bounds.h > 0.
        });
        // A resize changes hover geometry, not the physical contact lifecycle.
        // Only a fresh mouse event or native leave may cancel mouse capture.
    }

    fn inside_window(&self, position: Vector) -> bool {
        self.window_bounds.is_none_or(|bounds| {
            position.x >= bounds.x
                && position.x < bounds.right()
                && position.y >= bounds.y
                && position.y < bounds.bottom()
        })
    }

    /// Forget the mouse position for a touch.
    pub(crate) fn touch(&mut self) {
        self.position = None;
    }

    pub(crate) fn observe_mouse(&mut self, position: Vector) -> bool {
        // Mouse events emulated from a recent touch do not hover.
        if mouse_is_emulated() {
            self.position = None;
            return false;
        }
        // Captured native events may continue outside the window after leave.
        // They remain raw Scene input, but cannot establish a UI mouse route.
        let valid = self.focused
            && position.is_finite()
            && self.inside_window(position)
            && (!self.outside || self.window_bounds.is_some());
        self.position = valid.then_some(position);
        if valid {
            // A fresh in-window position also confirms re-entry when capture
            // prevents the platform from reporting a separate native enter.
            self.outside = false;
            set_hover_capable(true);
        }
        valid
    }

    pub(crate) fn enter_mouse(&mut self, position: Vector) -> bool {
        self.outside = false;
        self.observe_mouse(position)
    }

    pub(crate) fn leave(&mut self) {
        self.position = None;
        self.outside = true;
    }

    pub(crate) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        // A restored window needs a new enter/move/button position confirmation.
        self.position = None;
        self.outside = false;
    }

    pub(crate) fn rehit(&mut self, root: NodeRef) -> IndexSet<NodeRef> {
        let next = self
            .position()
            .map_or_else(IndexSet::new, |pos| hit_test(root, pos));
        self.update_path(next.clone());
        next
    }

    pub(crate) fn position(&self) -> Option<Vector> {
        self.position
            .filter(|position| self.inside_window(*position))
    }

    pub(crate) fn update_path(&mut self, next: IndexSet<NodeRef>) {
        let writes: Vec<(Signal<bool>, bool)> = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            self.path
                .difference(&next)
                .map(|node| (node, false))
                .chain(next.difference(&self.path).map(|node| (node, true)))
                .filter_map(|(node, value)| nodes.get(node.id).map(|node| (node, value)))
                .flat_map(|(node, value)| {
                    node.hovered
                        .iter()
                        .chain(&node.hovered_aliases)
                        .map(move |signal| (*signal, value))
                })
                .collect()
        });
        self.path = next;
        // No UI/Node borrow may survive into user computations at batch flush.
        batch(|| {
            for (signal, value) in writes {
                if !signal.is_disposed() {
                    signal.set(value);
                }
            }
        });
    }
}
