use std::rc::Rc;

use indexmap::IndexSet;
use mue_core::signal::Access;

use crate::{
    math::Vector,
    node::{NodeInner, NodeRef},
    runtime::Runtime,
};

/// A custom hit test receives a point in the node's untransformed canvas space.
/// Visit direct children in reverse paint order with `hit_test_into`, then call
/// `hit_self` if the node itself accepts the point. An empty result is pass-through.
pub type HitTestFn = Rc<dyn Fn(NodeRef, Vector, &mut HitTestResult)>;

/// One topmost target followed by its real Node ancestors, deepest first.
#[derive(Default)]
pub struct HitTestResult {
    path: IndexSet<NodeRef>,
    frames: Vec<(NodeRef, Rc<[NodeRef]>)>,
}

impl HitTestResult {
    /// Accept the current custom node only when no child has already hit.
    pub fn hit_self(&mut self, node: NodeRef) -> bool {
        if self.path.is_empty() && self.frames.last().is_some_and(|frame| frame.0 == node) {
            self.path.insert(node);
        }
        !self.path.is_empty()
    }
}

pub fn hit_test(node: NodeRef, pos: Vector) -> IndexSet<NodeRef> {
    let mut result = HitTestResult::default();
    hit_test_into(node, pos, &mut result);
    result.path
}

/// Whether `pos` hits a node that wants pointer input.
///
/// A node is interactive when it has a pointer handler, a gesture, or a custom
/// hit test. Layout-only containers are pass-through, so an empty overlay area
/// does not claim a chart touch.
pub fn hit_test_interactive(root: NodeRef, pos: Vector) -> bool {
    hit_test(root, pos).iter().any(|node| is_interactive(*node))
}

pub(crate) fn is_interactive(node: NodeRef) -> bool {
    Runtime::with(|rt| {
        let nodes = rt.nodes.borrow();
        let Some(inner) = nodes.get(node.id) else {
            return false;
        };
        !inner.hooks.pointer_event.is_empty()
            || !inner.gestures.is_empty()
            || inner.hit_test.is_some()
    })
}

/// Visit a direct child from a custom hit test. Returns whether a target exists.
/// Once a target is found, later siblings cannot replace it or join its path.
pub fn hit_test_into(node: NodeRef, pos: Vector, result: &mut HitTestResult) -> bool {
    if !result.path.is_empty() {
        return true;
    }
    if result
        .frames
        .last()
        .is_some_and(|frame| !frame.1.contains(&node))
    {
        return false;
    }
    let snapshot = Runtime::with(|rt| {
        let nodes = rt.nodes.borrow();
        let n = nodes.get(node.id)?;
        Some((
            n.effective_transform(),
            n.layout.rect,
            n.clip,
            n.clip_radius,
            n.clip_sector,
            *n.children,
            n.hit_test.clone(),
        ))
    });
    let Some((transform, rect, clip, clip_radius, clip_sector, children, custom)) = snapshot else {
        return false;
    };
    if !transform.is_finite() || transform.determinant().abs() < f32::EPSILON {
        return false;
    }
    let pos = crate::math::project_point(transform.inverse(), pos);
    if !pos.is_finite() || clip.is_some_and(|rect| !rect.get_untracked().contains(&pos)) {
        return false;
    }
    if !rect.is_null()
        && !crate::clip::contains_point(
            rect.get_untracked(),
            clip_radius.and_then(|radius| radius.try_get_untracked()),
            clip_sector.and_then(|sector| sector.try_get_untracked()),
            pos,
        )
    {
        return false;
    }
    let children = if children.is_null() {
        Rc::from([])
    } else {
        children.get_clone_untracked()
    };
    result.frames.push((node, children.clone()));
    if let Some(custom) = custom {
        custom(node, pos, result);
    } else {
        // Layout preparation has baked scroll offsets into children's canvas rects.
        for &child in children.iter().rev() {
            if hit_test_into(child, pos, result) {
                break;
            }
        }
        // Layoutless containers remain traversable, but are not infinite blockers.
        if !rect.is_null() && rect.get_untracked().contains(&pos) {
            result.hit_self(node);
        }
    }
    result.frames.pop();
    if !result.path.is_empty() {
        result.path.insert(node);
        true
    } else {
        false
    }
}

pub fn set_hit_test_fn(hit_test: HitTestFn) {
    NodeInner::with_mut(|node| node.hit_test = Some(hit_test));
}
