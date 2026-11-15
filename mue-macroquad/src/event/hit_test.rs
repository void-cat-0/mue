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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::clip::Sector;
    use crate::math::{Matrix, Rect, vec2};
    use crate::runtime::Runtime;
    use mue_core::{Disposable, prop::Prop, scope::Scope, signal::signal};

    pub(crate) struct TestNode(pub NodeRef);
    impl TestNode {
        pub(crate) fn new(bounds: Option<Rect>, children: &[NodeRef]) -> Self {
            let scope = Scope::new_attached();
            let id = scope.run(|| {
                let children = (*signal(Rc::from(children))).owned();
                let mut node = NodeInner::new(scope, children);
                if let Some(bounds) = bounds {
                    node.layout.rect = signal(bounds);
                }
                node.register()
            });
            Self(NodeRef::from_id(id))
        }

        fn with_clip_radius(self, radius: f32) -> Self {
            Runtime::with(|rt| {
                rt.nodes.borrow_mut()[self.0.id].clip_radius = Some(Prop::Static(radius));
            });
            self
        }

        fn with_clip_sector(self, sector: Sector) -> Self {
            Runtime::with(|rt| {
                rt.nodes.borrow_mut()[self.0.id].clip_sector = Some(Prop::Static(sector));
            });
            self
        }
    }
    impl Drop for TestNode {
        fn drop(&mut self) {
            self.0.dispose();
        }
    }
    fn ids(root: NodeRef, position: Vector) -> Vec<crate::node::NodeId> {
        hit_test(root, position)
            .iter()
            .map(|node| node.id)
            .collect()
    }

    #[test]
    fn rounded_clip_rejects_corner_hits() {
        let node = TestNode::new(Some(Rect::new(0., 0., 30., 30.)), &[]).with_clip_radius(10.);
        assert_eq!(ids(node.0, vec2(15., 15.)), vec![node.0.id]);
        assert!(hit_test(node.0, vec2(1., 1.)).is_empty());
    }

    #[test]
    fn sector_clip_rejects_outside_angle_hits() {
        let sector = Sector::new(vec2(15., 15.), -std::f32::consts::FRAC_PI_2, 0.);
        let node = TestNode::new(Some(Rect::new(0., 0., 30., 30.)), &[]).with_clip_sector(sector);
        assert_eq!(ids(node.0, vec2(25., 5.)), vec![node.0.id]);
        assert!(hit_test(node.0, vec2(5., 25.)).is_empty());
    }

    #[test]
    fn topmost_without_hover_blocks_siblings_but_layoutless_nodes_do_not() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let back = TestNode::new(rect, &[]);
        let front = TestNode::new(rect, &[]);
        let empty = TestNode::new(None, &[]);
        let root = TestNode::new(None, &[back.0, front.0, empty.0]);
        assert_eq!(ids(root.0, vec2(10., 10.)), vec![front.0.id, root.0.id]);
        assert!(hit_test(root.0, vec2(100., 100.)).is_empty());
    }
    #[test]
    fn overflow_keeps_real_ancestors_and_clipping_rejects_it() {
        let child = TestNode::new(Some(Rect::new(30., 0., 20., 20.)), &[]);
        let root = TestNode::new(Some(Rect::new(0., 0., 10., 10.)), &[child.0]);
        assert_eq!(ids(root.0, vec2(35., 5.)), vec![child.0.id, root.0.id]);
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[root.0.id].clip = Some(*signal(Rect::new(0., 0., 10., 10.)))
        });
        assert!(hit_test(root.0, vec2(35., 5.)).is_empty());
    }
    #[test]
    fn transform_and_scrolled_canvas_bounds_share_the_same_path() {
        let child = TestNode::new(Some(Rect::new(0., -20., 50., 50.)), &[]);
        let root = TestNode::new(Some(Rect::new(0., 0., 50., 50.)), &[child.0]);
        Runtime::with(|rt| {
            let mut nodes = rt.nodes.borrow_mut();
            let node = &mut nodes[root.0.id];
            node.matrix_transform = Prop::Static(Matrix::from_translation(vec2(100., 10.)));
            node.clip = Some(*signal(Rect::new(0., 0., 50., 50.)));
            node.child_offset = Some(*signal(vec2(0., 20.)));
        });
        assert_eq!(ids(root.0, vec2(110., 20.)), vec![child.0.id, root.0.id]);
        assert!(hit_test(root.0, vec2(110., 5.)).is_empty());
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[root.0.id].matrix_transform =
                Prop::Static(Matrix::from_scale(vec2(0., 1.)))
        });
        assert!(hit_test(root.0, vec2(110., 20.)).is_empty());
    }
    #[test]
    fn custom_hit_selects_one_child_and_rejects_foreign_nodes() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let back = TestNode::new(rect, &[]);
        let front = TestNode::new(rect, &[]);
        let foreign = TestNode::new(rect, &[]);
        let root = TestNode::new(rect, &[back.0, front.0]);
        let (a, b, outside) = (front.0, back.0, foreign.0);
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[root.0.id].hit_test = Some(Rc::new(move |node, pos, result| {
                assert!(!hit_test_into(outside, pos, result));
                hit_test_into(a, pos, result);
                hit_test_into(b, pos, result);
                result.hit_self(node);
            }))
        });
        assert_eq!(ids(root.0, vec2(5., 5.)), vec![front.0.id, root.0.id]);
    }
    #[test]
    fn custom_passthrough_and_disposed_nodes_are_safe() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let back = TestNode::new(rect, &[]);
        let front = TestNode::new(rect, &[]);
        let root = TestNode::new(None, &[back.0, front.0]);
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[front.0.id].hit_test = Some(Rc::new(|_, _, _| {}))
        });
        assert_eq!(ids(root.0, vec2(5., 5.)), vec![back.0.id, root.0.id]);
        back.0.dispose();
        assert!(hit_test(back.0, vec2(5., 5.)).is_empty());
        assert!(hit_test(root.0, vec2(5., 5.)).is_empty());
    }

    fn set_pointer_hook(node: NodeRef) {
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[node.id]
                .hooks
                .pointer_event
                .append(|_| {});
        });
    }

    fn set_render_hook(node: NodeRef) {
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[node.id].hooks.render.append(|_| {});
        });
    }

    fn set_hover_hook(node: NodeRef) {
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[node.id]
                .hooks
                .hover_event
                .append(|_| {});
        });
    }

    #[test]
    fn interactive_requires_pointer_hook_gesture_or_custom_hit() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let child = TestNode::new(rect, &[]);
        let root = TestNode::new(rect, &[child.0]);
        assert!(!hit_test_interactive(root.0, vec2(5., 5.)));
        set_pointer_hook(child.0);
        assert!(hit_test_interactive(root.0, vec2(5., 5.)));
        assert!(!hit_test_interactive(root.0, vec2(100., 100.)));
    }

    #[test]
    fn render_and_hover_hooks_do_not_make_a_pointer_target() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let child = TestNode::new(rect, &[]);
        let root = TestNode::new(None, &[child.0]);
        set_render_hook(child.0);
        set_hover_hook(child.0);
        assert!(!hit_test_interactive(root.0, vec2(5., 5.)));
    }

    #[test]
    fn custom_hit_test_is_interactive() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let child = TestNode::new(rect, &[]);
        let root = TestNode::new(None, &[child.0]);
        Runtime::with(|rt| {
            rt.nodes.borrow_mut()[child.0.id].hit_test = Some(Rc::new(|node, _, result| {
                result.hit_self(node);
            }));
        });
        assert!(hit_test_interactive(root.0, vec2(5., 5.)));
    }

    #[test]
    fn ancestor_pointer_hook_counts() {
        let rect = Some(Rect::new(0., 0., 30., 30.));
        let child = TestNode::new(rect, &[]);
        let root = TestNode::new(rect, &[child.0]);
        set_pointer_hook(root.0);
        assert!(hit_test_interactive(root.0, vec2(5., 5.)));
    }
}
