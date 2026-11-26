use std::rc::Rc;

use indexmap::IndexSet;
use mue_core::signal::Access;

use crate::{
    app::runtime::Runtime,
    math::{Rect, Vector},
    node::{NodeInner, NodeRef},
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
    ran_custom: bool,
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
    hit_test_with_cache(node, pos).0
}

/// Return the path and whether it can be reused at the same position in this
/// input batch. Custom hit tests may depend on mutable external state.
pub(crate) fn hit_test_with_cache(node: NodeRef, pos: Vector) -> (IndexSet<NodeRef>, bool) {
    let mut result = HitTestResult::default();
    visit(node, pos, &mut result);
    (result.path, !result.ran_custom)
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
            || crate::input::focus::declares_focus(inner)
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
    visit(node, pos, result)
}

/// The rectangle that accepts pointer hits for a node: its border box grown
/// by `hit_slop` and then to at least `min_hit_size` on each axis.
pub fn hit_rect(rect: Rect, slop: f32, min_size: f32) -> Rect {
    let mut hit = rect.outset(slop.max(0.));
    let min_size = min_size.max(0.);
    if hit.w < min_size {
        hit.x -= (min_size - hit.w) / 2.;
        hit.w = min_size;
    }
    if hit.h < min_size {
        hit.y -= (min_size - hit.h) / 2.;
        hit.h = min_size;
    }
    hit
}

fn visit(node: NodeRef, pos: Vector, result: &mut HitTestResult) -> bool {
    let snapshot = Runtime::with(|rt| {
        let nodes = rt.nodes.borrow();
        let n = nodes.get(node.id)?;
        Some((
            n.transform(),
            n.layout.rect,
            n.clip,
            n.clip_radius,
            n.clip_sector,
            *n.children,
            n.hit_test.clone(),
            n.hit_area
                .as_ref()
                .map(|(slop, size)| (slop.get_untracked(), size.get_untracked())),
            n.layout_id,
        ))
    });
    let Some((
        transform,
        rect,
        clip,
        clip_radius,
        clip_sector,
        children,
        custom,
        hit_area,
        layout_id,
    )) = snapshot
    else {
        return false;
    };
    if layout_id.is_some_and(crate::node::layout_hidden) {
        return false;
    }
    let transform = transform.resolve();
    if !transform.is_finite() || transform.determinant().abs() < f32::EPSILON {
        return false;
    }
    let pos = crate::math::project_point(transform.inverse(), pos);
    if !pos.is_finite() || clip.is_some_and(|rect| !rect.get_untracked().contains(&pos)) {
        return false;
    }
    let mut in_slop = false;
    if !rect.is_null() {
        let bounds = rect.get_untracked();
        in_slop = !bounds.contains(&pos)
            && hit_area.is_some_and(|(slop, size)| hit_rect(bounds, slop, size).contains(&pos));
        if !in_slop
            && !crate::paint::clip::contains_point(
                bounds,
                clip_radius.and_then(|radius| radius.try_get_untracked()),
                clip_sector.and_then(|sector| sector.try_get_untracked()),
                pos,
            )
        {
            return false;
        }
    }
    let children = if children.is_null() {
        Rc::from([])
    } else {
        children.get_clone_untracked()
    };
    let hit = if let Some(custom) = custom {
        result.frames.push((node, children));
        custom(node, pos, result);
        result.frames.pop();
        result.ran_custom = true;
        !result.path.is_empty()
    } else {
        // Preserve the parent list for custom callbacks deeper in this subtree.
        result.frames.push((node, children.clone()));
        let child_hit = crate::node::paint_order(children.clone())
            .iter()
            .rev()
            .any(|&child| visit(child, pos, result));
        result.frames.pop();
        child_hit || in_slop || (!rect.is_null() && rect.get_untracked().contains(&pos))
    };
    if hit {
        result.path.insert(node);
    }
    hit
}

pub fn set_hit_test_fn(hit_test: HitTestFn) {
    NodeInner::with_mut(|node| node.hit_test = Some(hit_test));
}
