//! Scene roots share preparation evidence without sharing a layout origin.

use std::{cell::Cell, collections::HashSet, rc::Rc};

use mue_core::{
    Disposable,
    effect::watch,
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};

use crate::{
    layout::use_layout,
    math::{Rect, Vector, vec2},
    node::{self, Node, NodeRef, map_keyed},
    runtime::Runtime,
    style::{Size, Style, Styleable, StyleableExt},
};

fn layout_node(style: Style) -> Node {
    Node::leaf(style, |style| {
        use_layout(style);
    })
}

fn rect(node: NodeRef) -> ReadSignal<Rect> {
    Runtime::with(|rt| *rt.node(node.id).layout.rect)
}

fn dirty(node: NodeRef) -> bool {
    Runtime::with(|rt| rt.taffy.borrow().dirty(node.layout_id().unwrap()).unwrap())
}

#[test]
fn bounded_prepare_uses_available_size_for_percentages_and_translates_scrolled_children() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(vec2(8., 6.));
        let child = layout_node(
            Style::new()
                .absolute()
                .inset_left(Size::Percent(25.))
                .inset_top(Size::Percent(50.))
                .width(Size::Percent(50.))
                .height(Size::Percent(25.)),
        );
        let child_ref = *child;
        let root = Node::leaf(
            Style::new()
                .width(Size::Percent(50.))
                .height(Size::Percent(50.))
                .children(child),
            |style| {
                use_layout(style);
                node::set_child_offset(*offset);
            },
        );

        root.prepare_layout_in(Rect::new(80., 50., 400., 200.));

        assert_eq!(rect(*root).get(), Rect::new(80., 50., 200., 100.));
        assert_eq!(rect(child_ref).get(), Rect::new(122., 94., 100., 25.));
        let previous = root.prepared_geometry();
        assert!(previous.is_current());

        root.prepare_layout_in(Rect::new(-20., 30., 200., 160.));

        assert_eq!(rect(*root).get(), Rect::new(-20., 30., 100., 80.));
        assert_eq!(rect(child_ref).get(), Rect::new(-3., 64., 50., 20.));
        assert!(!previous.is_current());
        assert!(root.prepared_geometry().is_current());
    });
}

#[test]
fn layoutless_root_preserves_bounded_origin_and_available_constraints() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(vec2(4., 3.));
        let child = layout_node(
            Style::new()
                .width(Size::Percent(50.))
                .height(Size::Percent(50.)),
        );
        let child_ref = *child;
        let root = Node::leaf(Style::new().children(child), |_| {
            node::set_child_offset(*offset);
        });

        root.prepare_layout_in(Rect::new(10., 20., 300., 120.));

        assert!(root.layout_id().is_none());
        assert_eq!(rect(child_ref).get(), Rect::new(6., 17., 150., 60.));
        assert!(root.prepared_geometry().is_current());
    });
}

#[test]
fn bounded_prepare_and_snapshot_reads_do_not_advance_updates_or_run_extra_layout_hooks() {
    let layouts = Rc::new(Cell::new(0));
    let updates = Rc::new(Cell::new(0));
    let root = layout_node(
        Style::new()
            .width(40.)
            .height(20.)
            .on_layout({
                let layouts = layouts.clone();
                move |_| layouts.set(layouts.get() + 1)
            })
            .on_update({
                let updates = updates.clone();
                move |_| updates.set(updates.get() + 1)
            })
            .on_render(|_| panic!("preparation must not render")),
    );
    let bounds = Rect::new(12., 23., 200., 100.);
    for _ in 0..3 {
        root.prepare_layout_in(bounds);
    }
    let prepared_layouts = layouts.get();
    for _ in 0..3 {
        assert!(root.prepared_geometry().is_current());
        assert_eq!(root.frame_nodes().len(), 1);
    }
    assert_eq!(layouts.get(), prepared_layouts);
    assert_eq!(updates.get(), 0);

    root.advance_frame(0.25);
    root.prepare_layout_in(bounds);
    root.prepare_layout_in(bounds);

    assert_eq!(updates.get(), 1);
    assert_eq!(rect(*root).get(), Rect::new(12., 23., 40., 20.));
}

#[test]
fn later_root_rect_feedback_invalidates_earlier_taffy_snapshot_until_prepared_again() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let first_width = signal(Size::Px(40.));
        let second_width = signal(Size::Px(10.));
        let first = Node::leaf(Style::new().width(first_width).height(20.), |style| {
            let layout = use_layout(style);
            watch(layout.rect, move |bounds| {
                if bounds.get().w > 0. {
                    second_width.set(Size::Px(80.));
                }
            });
        });
        let second = Node::leaf(Style::new().width(second_width).height(20.), |style| {
            let layout = use_layout(style);
            watch(layout.rect, move |bounds| {
                if bounds.get().w > 0. {
                    first_width.set(Size::Px(60.));
                }
            });
        });
        let first_bounds = Rect::new(10., 20., 200., 100.);
        let second_bounds = Rect::new(250., 20., 200., 100.);
        first.prepare_layout_in(first_bounds);
        let first_snapshot = first.prepared_geometry();
        assert!(first_snapshot.is_current());
        assert!(!dirty(*first));

        second.prepare_layout_in(second_bounds);
        let second_snapshot = second.prepared_geometry();

        assert_eq!(rect(*first).get().w, 40.);
        assert!(
            dirty(*first),
            "later root feedback must invalidate earlier layout"
        );
        assert!(!first_snapshot.is_current());
        let captured_dirty = first.prepared_geometry();
        assert!(!captured_dirty.is_current());
        first.prepare_layout_in(first_bounds);
        assert_eq!(rect(*first).get(), Rect::new(10., 20., 60., 20.));
        assert!(
            !captured_dirty.is_current(),
            "capturing a dirty tree is never prepared evidence"
        );
        assert!(first.prepared_geometry().is_current());
        assert!(second_snapshot.is_current());
    });
}

#[test]
fn later_root_rect_feedback_invalidates_descendant_offset_without_taffy_dirtiness() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(Vector::ZERO);
        let child = layout_node(Style::new().width(20.).height(10.));
        let child_ref = *child;
        let wrapper = Node::leaf(Style::new().children(child), |_| {
            node::set_child_offset(*offset);
        });
        let first = layout_node(Style::new().width(100.).height(50.).children(wrapper));
        let second = Node::leaf(Style::new().width(50.).height(20.), |style| {
            let layout = use_layout(style);
            watch(layout.rect, move |bounds| {
                if bounds.get().w > 0. {
                    offset.set(vec2(12., 7.));
                }
            });
        });
        let bounds = Rect::new(30., 40., 200., 100.);
        first.prepare_layout_in(bounds);
        let first_snapshot = first.prepared_geometry();
        assert!(first_snapshot.is_current());

        second.prepare_layout_in(Rect::new(250., 40., 200., 100.));

        assert!(!dirty(*first));
        assert!(!dirty(child_ref));
        assert_eq!(rect(child_ref).get(), Rect::new(30., 40., 20., 10.));
        assert!(!first_snapshot.is_current());
        first.prepare_layout_in(bounds);
        assert_eq!(rect(child_ref).get(), Rect::new(18., 33., 20., 10.));
        assert!(first.prepared_geometry().is_current());
    });
}

#[test]
fn later_root_rect_feedback_invalidates_layoutless_children_without_taffy_dirtiness() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let child_layouts = Rc::new(Cell::new(0));
        let children = map_keyed(rows, |key| *key, {
            let child_layouts = child_layouts.clone();
            move |_| {
                let child_layouts = child_layouts.clone();
                Node::leaf(
                    Style::new().on_layout(move |_| child_layouts.set(child_layouts.get() + 1)),
                    |_| {},
                )
            }
        });
        let wrapper = Node::leaf(Style::new().children(children), |_| {});
        let first = layout_node(Style::new().width(100.).height(50.).children(wrapper));
        let second = Node::leaf(Style::new().width(50.).height(20.), |style| {
            let layout = use_layout(style);
            watch(layout.rect, move |bounds| {
                if bounds.get().w > 0. {
                    rows.set(Rc::from([1]));
                }
            });
        });
        let bounds = Rect::new(30., 40., 200., 100.);
        first.prepare_layout_in(bounds);
        let first_snapshot = first.prepared_geometry();
        assert!(first_snapshot.is_current());

        second.prepare_layout_in(Rect::new(250., 40., 200., 100.));

        assert!(!dirty(*first));
        assert!(!first_snapshot.is_current());
        assert_eq!(
            child_layouts.get(),
            0,
            "snapshot validation must not run hooks"
        );
        first.prepare_layout_in(bounds);
        assert!(child_layouts.get() > 0);
        assert_eq!(first.frame_nodes().len(), 3);
        assert!(first.prepared_geometry().is_current());
    });
}

#[test]
fn bounded_prepare_measures_rect_observer_mounted_subtrees_before_their_hooks() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let layouts = Rc::new(Cell::new(0));
        let children = map_keyed(rows, |key| *key, {
            let layouts = layouts.clone();
            move |_| {
                let layouts = layouts.clone();
                Node::leaf(
                    Style::new()
                        .width(Size::Percent(50.))
                        .height(Size::Percent(50.)),
                    move |style| {
                        let layout = use_layout(style);
                        style.on_layout.append(move |_| {
                            assert_eq!(layout.rect.get(), Rect::new(25., 35., 100., 50.));
                            layouts.set(layouts.get() + 1);
                        });
                    },
                )
            }
        });
        let root = Node::leaf(
            Style::new().width(200.).height(100.).children(children),
            |style| {
                let layout = use_layout(style);
                watch(layout.rect, move |bounds| {
                    if bounds.get().w > 0. {
                        rows.set(Rc::from([1]));
                    }
                });
            },
        );

        root.prepare_layout_in(Rect::new(25., 35., 400., 200.));

        assert!(layouts.get() > 0);
        assert!(root.prepared_geometry().is_current());
    });
}

#[test]
fn frame_node_snapshots_expose_cross_root_and_ancestor_overlap_and_skip_dead_nodes() {
    let child = Node::empty();
    let child_ref = *child;
    let root = Node::leaf(Style::new().children(child), |_| {});
    let other = Node::empty();
    let nodes = root.frame_nodes();
    assert!(nodes.as_slice() == [*root, child_ref]);
    assert!(child_ref.frame_nodes().as_slice() == [child_ref]);
    let mut seen = HashSet::new();
    assert!(nodes.iter().all(|node| seen.insert(*node)));
    assert!(
        !child_ref
            .frame_nodes()
            .iter()
            .all(|node| seen.insert(*node))
    );
    assert!(!root.frame_nodes().iter().all(|node| seen.insert(*node)));
    assert!(other.frame_nodes().iter().all(|node| seen.insert(*node)));

    child_ref.dispose();

    assert!(root.frame_nodes().as_slice() == [*root]);
    assert!(child_ref.frame_nodes().is_empty());
}

#[test]
fn dead_root_has_empty_snapshots_and_does_not_prepare_or_update() {
    let root = layout_node(Style::new().width(40.).height(20.));
    root.prepare_layout_in(Rect::new(10., 20., 100., 50.));
    let live_snapshot = root.prepared_geometry();
    assert!(live_snapshot.is_current());
    root.dispose();

    let empty = root.prepared_geometry();
    assert!(empty.root.is_none());
    assert!(empty.published.is_empty());
    assert!(empty.layout_roots.is_empty());
    assert!(empty.is_current());
    assert!(!live_snapshot.is_current());
    assert!(root.frame_nodes().is_empty());
    root.prepare_layout_in(Rect::new(10., 20., 100., 50.));
    root.advance_frame(0.25);
}
