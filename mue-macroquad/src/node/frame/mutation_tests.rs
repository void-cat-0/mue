//! Geometry mutations must stabilize before new hooks consume their bounds.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::{
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};

use crate::{
    event::hit_test::hit_test,
    layout::use_layout,
    math::{Rect, Vector, vec2},
    node::{Children, Node, NodeId, NodeRef, map_keyed},
    runtime::Runtime,
    style::{Size, Style, Styleable},
};

const AVAILABLE: Vector = Vector::new(400., 300.);

fn layout_node(style: Style) -> Node {
    Node::leaf(style, |style| {
        use_layout(style);
    })
}

fn rect(node: NodeRef) -> ReadSignal<Rect> {
    Runtime::with(|rt| *rt.node(node.id).layout.rect)
}

fn alive(node: NodeRef) -> bool {
    Runtime::with(|rt| rt.nodes.borrow().contains_key(node.id))
}

fn hit_path(root: NodeRef, point: Vector) -> Vec<NodeId> {
    hit_test(root, point)
        .into_iter()
        .map(|node| node.id)
        .collect()
}

#[test]
fn layoutless_root_computes_its_layout_children_with_available_constraints() {
    let child = layout_node(Style::new().width(Size::Percent(50.)).height(20.));
    let child_ref = *child;
    let root = Node::leaf(Style::new().children(child), |_| {});

    assert!(root.layout_id().is_none());
    root.prepare_layout(AVAILABLE);

    assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 200., 20.));
    assert_eq!(
        hit_path(*root, vec2(150., 10.)),
        vec![child_ref.id, root.id]
    );
    assert!(hit_path(*root, vec2(250., 10.)).is_empty());

    root.prepare_layout(vec2(200., 300.));

    assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 100., 20.));
    assert!(hit_path(*root, vec2(150., 10.)).is_empty());
}

#[test]
fn layoutless_middle_node_does_not_leave_its_detached_taffy_subtree_uncomputed() {
    let child = layout_node(Style::new().width(80.).height(20.));
    let child_ref = *child;
    let wrapper = Node::leaf(Style::new().children(child), |_| {});
    let wrapper_ref = *wrapper;
    let root = layout_node(
        Style::new()
            .width(200.)
            .height(100.)
            .padding((10.,))
            .children(wrapper),
    );

    root.prepare_layout(AVAILABLE);

    assert!(wrapper_ref.layout_id().is_none());
    assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 80., 20.));
    assert_eq!(
        hit_path(*root, vec2(50., 15.)),
        vec![child_ref.id, wrapper_ref.id, root.id]
    );
}

#[test]
fn layout_hook_mounts_subtree_only_after_its_first_layout_has_been_computed() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mounted = Rc::new(Cell::new(None::<NodeRef>));
        let children = map_keyed(rows, |key| *key, {
            let seen = seen.clone();
            let mounted = mounted.clone();
            move |_| {
                let seen = seen.clone();
                let child = Node::leaf(Style::new().width(80.).height(20.), move |style| {
                    let layout = use_layout(style);
                    style.on_layout.append(move |_| {
                        let bounds = layout.rect.get();
                        assert_eq!(
                            bounds,
                            Rect::new(0., 0., 80., 20.),
                            "a newly mounted geometry consumer must never receive zero-sized layout"
                        );
                        seen.borrow_mut().push(bounds);
                    });
                });
                mounted.set(Some(*child));
                child
            }
        });
        let mut mounted_once = false;
        let root = layout_node(
            Style::new()
                .width(200.)
                .height(100.)
                .children(children)
                .on_layout(move |_| {
                    if !mounted_once {
                        mounted_once = true;
                        rows.set(Rc::from([1]));
                    }
                }),
        );

        root.prepare_layout(AVAILABLE);

        let child = mounted.get().expect("the layout hook mounted its child");
        assert!(!seen.borrow().is_empty());
        assert_eq!(rect(child).get(), Rect::new(0., 0., 80., 20.));
        assert_eq!(hit_path(*root, vec2(70., 15.)), vec![child.id, root.id]);
    });
}

#[test]
fn layout_callback_self_removal_skips_later_callbacks_and_owned_children() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([1]));
        let root_children = Rc::new(Cell::new(None::<Children>));
        let removed = Rc::new(Cell::new(None::<NodeRef>));
        let descendant = Rc::new(Cell::new(None::<NodeRef>));
        let children = map_keyed(rows, |key| *key, {
            let root_children = root_children.clone();
            let removed = removed.clone();
            let descendant = descendant.clone();
            move |_| {
                let child = layout_node(Style::new().width(10.).height(10.).on_layout(|_| {
                    panic!("a removed node must not prepare its former children");
                }));
                descendant.set(Some(*child));
                let root_children = root_children.clone();
                let branch = Node::leaf(
                    Style::new().width(80.).height(30.).children(child),
                    move |style| {
                        use_layout(style);
                        style.on_layout.append(move |_| {
                            rows.set(Rc::from([]));
                            // Pull the child list now, disposing this branch before
                            // the next callback in this same hook chain.
                            root_children.get().unwrap().get_clone();
                        });
                        style.on_layout.append(|_| {
                            panic!("callbacks after self removal must be skipped");
                        });
                    },
                );
                removed.set(Some(*branch));
                branch
            }
        });
        let root = layout_node(Style::new().width(200.).height(100.).children(children));
        root_children.set(Some(root.children()));
        let branch = removed.get().unwrap();
        let child = descendant.get().unwrap();

        root.prepare_layout(AVAILABLE);

        assert!(!alive(branch));
        assert!(!alive(child));
        assert!(root.children().get_clone().is_empty());
        assert_eq!(rect(*root).get(), Rect::new(0., 0., 200., 100.));
        assert_eq!(hit_path(*root, vec2(20., 10.)), vec![root.id]);
    });
}

#[test]
fn descendant_layout_callback_can_remove_its_ancestor_and_keep_the_remaining_tree_valid() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([1]));
        let root_children = Rc::new(Cell::new(None::<Children>));
        let branch_ref = Rc::new(Cell::new(None::<NodeRef>));
        let mutator_ref = Rc::new(Cell::new(None::<NodeRef>));
        let branch = map_keyed(rows, |key| *key, {
            let root_children = root_children.clone();
            let branch_ref = branch_ref.clone();
            let mutator_ref = mutator_ref.clone();
            move |_| {
                let root_children = root_children.clone();
                let mutator = Node::leaf(Style::new().width(10.).height(10.), move |style| {
                    use_layout(style);
                    style.on_layout.append(move |_| {
                        rows.set(Rc::from([]));
                        root_children.get().unwrap().get_clone();
                    });
                    style.on_layout.append(|_| {
                        panic!("removing an ancestor also invalidates the current callback chain");
                    });
                });
                mutator_ref.set(Some(*mutator));
                let sibling = layout_node(Style::new().width(10.).height(10.).on_layout(|_| {
                    panic!("a former sibling in the removed ancestor must not be visited");
                }));
                let branch = layout_node(
                    Style::new()
                        .width(80.)
                        .height(30.)
                        .children((mutator, sibling)),
                );
                branch_ref.set(Some(*branch));
                branch
            }
        });
        let survivor = layout_node(Style::new().width(20.).height(20.));
        let survivor_ref = *survivor;
        let root = layout_node(
            Style::new()
                .width(200.)
                .height(100.)
                .children((branch, survivor)),
        );
        root_children.set(Some(root.children()));
        let removed_branch = branch_ref.get().unwrap();
        let removed_mutator = mutator_ref.get().unwrap();

        root.prepare_layout(AVAILABLE);

        assert!(!alive(removed_branch));
        assert!(!alive(removed_mutator));
        assert!(alive(survivor_ref));
        assert_eq!(rect(survivor_ref).get(), Rect::new(0., 0., 20., 20.));
        assert_eq!(
            hit_path(*root, vec2(10., 10.)),
            vec![survivor_ref.id, root.id]
        );
    });
}

#[test]
fn update_callback_self_removal_skips_later_callbacks_in_the_same_node() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([1]));
        let root_children = Rc::new(Cell::new(None::<Children>));
        let removed = Rc::new(Cell::new(None::<NodeRef>));
        let children = map_keyed(rows, |key| *key, {
            let root_children = root_children.clone();
            let removed = removed.clone();
            move |_| {
                let root_children = root_children.clone();
                let child = Node::leaf(Style::new().width(20.).height(20.), move |style| {
                    use_layout(style);
                    style.on_update.append(move |_| {
                        rows.set(Rc::from([]));
                        root_children.get().unwrap().get_clone();
                    });
                    style.on_update.append(|_| {
                        panic!("callbacks after self removal must be skipped");
                    });
                });
                removed.set(Some(*child));
                child
            }
        });
        let root = layout_node(Style::new().width(200.).height(100.).children(children));
        root_children.set(Some(root.children()));
        root.prepare_layout(AVAILABLE);
        let child = removed.get().unwrap();

        root.advance_frame(0.02);
        root.prepare_layout(AVAILABLE);

        assert!(!alive(child));
        assert!(root.children().get_clone().is_empty());
        assert_eq!(hit_path(*root, vec2(10., 10.)), vec![root.id]);
    });
}
