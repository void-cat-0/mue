use crate::{event::hit_test::hit_test, math::Vector, node::NodeRef, runtime::Runtime};
use std::cell::Cell;

#[derive(Debug, Clone)]
pub struct WheelEvent {
    pub position: Vector,
    pub delta: Vector,
    remaining: Cell<Vector>,
}

impl WheelEvent {
    pub fn new(position: Vector, delta: Vector) -> Self {
        Self {
            position,
            delta,
            remaining: Cell::new(delta),
        }
    }

    /// Wheel motion not yet handled by descendants, in Macroquad wheel units.
    pub fn remaining_delta(&self) -> Vector {
        self.remaining.get()
    }

    /// Consume motion on each axis; any remainder can scroll an ancestor.
    pub fn consume_delta(&self, delta: Vector) {
        let remaining = self.remaining.get();
        let consumed = delta.clamp(remaining.min(Vector::ZERO), remaining.max(Vector::ZERO));
        self.remaining.set(remaining - consumed);
    }
}

pub fn check_wheel(root_node: NodeRef) {
    let pos = macroquad::input::mouse_position();
    let delta = macroquad::input::mouse_wheel();
    if delta.0 == 0. && delta.1 == 0. {
        return;
    }

    let event = WheelEvent::new(Vector::new(pos.0, pos.1), Vector::new(delta.0, delta.1));
    let nodes = hit_test(root_node, event.position);
    Runtime::with(|rt| {
        for node in nodes.iter() {
            rt.invoke_hook(node.id, |hooks| &mut hooks.wheel_event, &event);
        }
    });
}
