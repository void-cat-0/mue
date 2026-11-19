//! Host-side pointer owner routing for a chart channel plus mue UI.
//!
//! This is a pure logic prototype. It does not read macroquad events and does
//! not touch `App`. The host supplies `ui_hit` (whether a pointer-down is over
//! an interactive UI node) and dispatches the returned [`Route`] to the chart
//! channel or leaves it for mue's post-render pointer manager.
//!
//! The owner is fixed on pointer-down and kept until up/cancel. Move events do
//! not re-hit-test, so a pointer that starts on the chart cannot slide onto a
//! UI button and trigger it (and vice versa).
//!
//! Modal overlays and disabled chart input cancel chart-owned pointers and
//! force new pointers to the UI, even when `ui_hit` is false. This is the
//! "modal capture" / "pause blocks chart" behavior.

use std::collections::HashMap;
use std::hash::Hash;

/// Which consumer owns a pointer for the duration of its gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Chart,
    Ui,
}

/// Where a single event should be dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Chart,
    Ui,
    None,
}

/// Per-pointer owner map plus modal/chart-enabled gates.
pub struct InputRouter<Id> {
    owners: HashMap<Id, Owner>,
    modal: bool,
    chart_enabled: bool,
}

impl<Id> Default for InputRouter<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id> InputRouter<Id> {
    pub fn new() -> Self {
        Self {
            owners: HashMap::new(),
            modal: false,
            chart_enabled: true,
        }
    }

    pub fn chart_enabled(&self) -> bool {
        self.chart_enabled
    }
}

impl<Id: Copy + Eq + Hash> InputRouter<Id> {
    /// Route a pointer-down and remember its owner until up/cancel.
    ///
    /// `ui_hit` must come from a hit test against the last frame's interactive
    /// UI (the host runs this before the current frame's layout). While a modal
    /// is open or chart input is disabled, the pointer is UI-owned regardless
    /// of `ui_hit`, so the chart never sees it.
    pub fn down(&mut self, id: Id, ui_hit: bool) -> Route {
        if let Some(owner) = self.owners.get(&id) {
            return owner_route(*owner);
        }
        let owner = if self.modal || !self.chart_enabled || ui_hit {
            Owner::Ui
        } else {
            Owner::Chart
        };
        self.owners.insert(id, owner);
        owner_route(owner)
    }

    /// Route a move. The owner is not recomputed.
    pub fn move_(&mut self, id: Id) -> Route {
        self.route(id)
    }

    /// Route an up and release the pointer.
    pub fn up(&mut self, id: Id) -> Route {
        let route = self.route(id);
        self.owners.remove(&id);
        route
    }

    /// Route a cancel and release the pointer.
    pub fn cancel(&mut self, id: Id) -> Route {
        let route = self.route(id);
        self.owners.remove(&id);
        route
    }

    /// Called when the chart rejects a chart-owned down. Reassigns the pointer
    /// to the UI so the rest of the gesture continues there. Only meaningful
    /// for the down decision; later events keep the owner fixed.
    pub fn fallback_to_ui(&mut self, id: Id) -> Route {
        if self.owners.get(&id) == Some(&Owner::Chart) {
            self.owners.insert(id, Owner::Ui);
            Route::Ui
        } else {
            self.route(id)
        }
    }

    /// Enable or disable chart input. Disabling cancels every chart-owned
    /// pointer and returns their ids so the host can send them a cancel.
    pub fn set_chart_enabled(&mut self, enabled: bool) -> Vec<Id> {
        self.chart_enabled = enabled;
        if enabled {
            Vec::new()
        } else {
            self.cancel_owned(Owner::Chart)
        }
    }

    /// Open or close a modal overlay. Opening cancels chart-owned pointers and
    /// returns their ids; closing does not restore them.
    pub fn set_modal(&mut self, modal: bool) -> Vec<Id> {
        self.modal = modal;
        if modal {
            self.cancel_owned(Owner::Chart)
        } else {
            Vec::new()
        }
    }

    /// Back-key priority: close the modal if one is open. Returns true when the
    /// router consumed the key; false means the host should handle it.
    pub fn back(&mut self) -> bool {
        if self.modal {
            self.set_modal(false);
            true
        } else {
            false
        }
    }

    fn route(&self, id: Id) -> Route {
        self.owners
            .get(&id)
            .copied()
            .map_or(Route::None, owner_route)
    }

    fn cancel_owned(&mut self, owner: Owner) -> Vec<Id> {
        let ids: Vec<Id> = self
            .owners
            .iter()
            .filter(|(_, current)| **current == owner)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.owners.remove(id);
        }
        ids
    }
}

fn owner_route(owner: Owner) -> Route {
    match owner {
        Owner::Chart => Route::Chart,
        Owner::Ui => Route::Ui,
    }
}
