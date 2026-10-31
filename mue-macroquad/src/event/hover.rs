use indexmap::IndexSet;
use mue_core::{batch, signal::Signal};

use crate::{math::Vector, node::NodeRef, runtime::Runtime};

use super::hit_test::hit_test;

/// Mouse location is independent of the pressed pointer's captured down path.
pub(crate) struct HoverState {
    position: Option<Vector>,
    focused: bool,
    path: IndexSet<NodeRef>,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            position: None,
            focused: true,
            path: IndexSet::new(),
        }
    }
}

impl HoverState {
    pub(crate) fn observe_mouse(&mut self, position: Vector) {
        self.position = (self.focused && position.is_finite()).then_some(position);
    }

    pub(crate) fn leave(&mut self) {
        self.position = None;
    }

    pub(crate) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        // A restored window needs a new enter/move/button position confirmation.
        self.leave();
    }

    pub(crate) fn rehit(&mut self, root: NodeRef) -> IndexSet<NodeRef> {
        let next = self
            .position
            .map_or_else(IndexSet::new, |pos| hit_test(root, pos));
        self.update_path(next.clone());
        next
    }

    fn update_path(&mut self, next: IndexSet<NodeRef>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::hit_test::tests::TestNode,
        math::{Rect, vec2},
    };
    use mue_core::{
        Disposable,
        effect::watch_effect,
        signal::{Access, signal},
    };
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn canonical_aliases_are_atomic_and_observers_can_borrow_the_node_arena() {
        let node = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let canonical = signal(false);
        let alias = signal(false);
        Runtime::with(|rt| {
            let mut nodes = rt.nodes.borrow_mut();
            nodes[node.0.id].hovered = Some(canonical);
            nodes[node.0.id].hovered_aliases = vec![alias];
        });
        let values = Rc::new(RefCell::new(Vec::new()));
        let out = values.clone();
        let observer = watch_effect(move || {
            let pair = (canonical.get(), alias.get());
            Runtime::with(|rt| {
                let _nodes = rt.nodes.borrow_mut();
            });
            out.borrow_mut().push(pair);
        });
        let mut hover = HoverState::default();
        hover.observe_mouse(vec2(5., 5.));
        hover.rehit(node.0);
        hover.rehit(node.0);
        hover.leave();
        hover.rehit(node.0);
        hover.rehit(node.0);
        assert_eq!(
            *values.borrow(),
            vec![(false, false), (true, true), (false, false)]
        );
        observer.dispose();
        canonical.dispose();
        alias.dispose();
    }
    #[test]
    fn stationary_cursor_rehits_occlusion_visibility_and_disposal() {
        let back = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let front = TestNode::new(Some(Rect::new(50., 0., 20., 20.)), &[]);
        let root = TestNode::new(None, &[back.0, front.0]);
        let hovered = signal(false);
        Runtime::with(|rt| rt.nodes.borrow_mut()[back.0.id].hovered = Some(hovered));
        let mut hover = HoverState::default();
        hover.observe_mouse(vec2(5., 5.));
        hover.rehit(root.0);
        assert!(hovered.get());
        let front_bounds = Runtime::with(|rt| rt.nodes.borrow()[front.0.id].layout.rect);
        front_bounds.set(Rect::new(0., 0., 20., 20.));
        hover.rehit(root.0);
        assert!(!hovered.get());
        let old_children = Runtime::with(|rt| *rt.nodes.borrow()[root.0.id].children);
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[root.0.id].children = (*signal(Rc::from([back.0]))).owned()
        });
        hover.rehit(root.0);
        assert!(hovered.get());
        old_children.dispose();
        back.0.dispose();
        hover.rehit(root.0);
        hover.leave();
        hover.rehit(root.0);
        assert!(!hover.path.contains(&back.0));
        hovered.dispose();
    }
    #[test]
    fn leave_and_focus_restore_require_fresh_valid_mouse_position() {
        let node = TestNode::new(Some(Rect::new(0., 0., 20., 20.)), &[]);
        let mut hover = HoverState::default();
        hover.observe_mouse(vec2(5., 5.));
        assert!(!hover.rehit(node.0).is_empty());
        hover.set_focused(false);
        hover.observe_mouse(vec2(5., 5.));
        assert!(hover.rehit(node.0).is_empty());
        hover.set_focused(true);
        assert!(hover.rehit(node.0).is_empty());
        hover.observe_mouse(vec2(5., 5.));
        assert!(!hover.rehit(node.0).is_empty());
        hover.leave();
        assert!(hover.rehit(node.0).is_empty());
        hover.observe_mouse(vec2(f32::NAN, 5.));
        assert!(hover.rehit(node.0).is_empty());
    }
}
