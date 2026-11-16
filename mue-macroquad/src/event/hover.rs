use indexmap::IndexSet;
use mue_core::{batch, signal::Signal};

use crate::{
    math::{Rect, Vector},
    node::NodeRef,
    runtime::Runtime,
};

use super::hit_test::hit_test;

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

    pub(crate) fn observe_mouse(&mut self, position: Vector) -> bool {
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
