use std::rc::Rc;

use indexmap::IndexSet;
use mue_core::signal::Access;
use slotmap::Key;

use crate::{
    math::Vector,
    node::{NodeInner, NodeRef},
    runtime::Runtime,
};

pub(crate) type HitTestFn = Rc<dyn Fn(NodeRef, Vector, &mut IndexSet<NodeRef>)>;

pub fn hit_test(node: NodeRef, pos: Vector) -> IndexSet<NodeRef> {
    let mut result = IndexSet::new();
    do_hit_test(node, pos, &mut result);
    result
}
pub(crate) fn do_hit_test(node: NodeRef, pos: Vector, result: &mut IndexSet<NodeRef>) {
    if node.id.is_null() {
        return;
    }
    Runtime::with(|rt| {
        let n = rt.node(node.id);
        let transform = n.transform.get();
        if !transform.is_finite() || transform.determinant().abs() < f32::EPSILON {
            return;
        }
        let pos = transform.inverse().transform_point2(pos);
        let inner = n.hit_test.clone();
        drop(n);
        let inner = inner.as_deref().unwrap_or(&default_hit_test);
        inner(node, pos, result);
    });
}

fn default_hit_test(node: NodeRef, pos: Vector, result: &mut IndexSet<NodeRef>) {
    let (layout_rect, children, clip) = Runtime::with(|rt| {
        let n = rt.node(node.id);
        let rect = n.layout.rect;
        let clip = n.clip;
        let children = if n.children.is_null() {
            Default::default()
        } else {
            let children = *n.children;
            drop(n);
            children.get_clone_untracked()
        };
        (rect, children, clip)
    });

    let contains = layout_rect.is_null() || layout_rect.get().contains(&pos);
    if clip.is_some_and(|rect| !rect.get().contains(&pos)) {
        return;
    }

    for &child in children.iter().rev() {
        do_hit_test(child, pos, result);
    }

    if contains {
        result.insert(node);
    }
}

#[allow(dead_code)]
pub(crate) fn set_hit_test_fn(hit_test: HitTestFn) {
    NodeInner::with_mut(|node| {
        node.hit_test = Some(hit_test);
    })
}
