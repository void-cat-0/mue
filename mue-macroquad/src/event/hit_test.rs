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
fn do_hit_test(node: NodeRef, pos: Vector, result: &mut IndexSet<NodeRef>) {
    if node.id.is_null() {
        return;
    }
    Runtime::with(|rt| {
        let inner = rt.node(node.id).hit_test.clone();
        let inner = inner.as_deref().unwrap_or(&default_hit_test);
        inner(node, pos, result);
    });
}

fn default_hit_test(node: NodeRef, pos: Vector, result: &mut IndexSet<NodeRef>) {
    let (layout_rect, children) = Runtime::with(|rt| {
        let n = rt.node(node.id);
        let children = if n.children.is_null() {
            Default::default()
        } else {
            n.children.get_clone_untracked()
        };
        (n.layout.rect, children)
    });

    if !layout_rect.is_null() && !layout_rect.get().contains(&pos) {
        return;
    }

    for &child in children.iter().rev() {
        do_hit_test(child, pos, result);
    }

    result.insert(node);
}

#[allow(dead_code)]
pub(crate) fn set_hit_test_fn(hit_test: HitTestFn) {
    NodeInner::with_mut(|node| {
        node.hit_test = Some(hit_test);
    })
}
