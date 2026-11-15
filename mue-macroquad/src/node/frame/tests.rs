//! Headless geometry regressions: nodes have layout and hooks, but no paint.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::{
    Disposable,
    effect::watch,
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};

use crate::{
    event::hit_test::hit_test,
    layout::use_layout,
    math::{Rect, Vector, vec2},
    node::{self, Node, NodeRef, map_keyed},
    runtime::Runtime,
    style::{FontSize, Size, Style, Styleable, StyleableExt},
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

fn hit_path(root: NodeRef, point: Vector) -> Vec<node::NodeId> {
    hit_test(root, point)
        .into_iter()
        .map(|node| node.id)
        .collect()
}

#[test]
fn prepare_publishes_positioned_centered_scrolled_bounds_before_hit_testing() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(vec2(8., 6.));
        let parent_rect = Rc::new(Cell::new(None::<ReadSignal<Rect>>));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let child = Node::leaf(
            Style::new()
                .absolute()
                .inset_left(50.)
                .inset_top(30.)
                .width(20.)
                .height(12.),
            {
                let parent_rect = parent_rect.clone();
                let seen = seen.clone();
                move |style| {
                    let layout = use_layout(style);
                    node::center_on_layout_origin();
                    style.on_layout.append(move |_| {
                        seen.borrow_mut()
                            .push((parent_rect.get().unwrap().get(), layout.rect.get()));
                    });
                }
            },
        );
        let child_ref = *child;
        let parent = Node::leaf(
            Style::new()
                .absolute()
                .inset_left(40.)
                .inset_top(25.)
                .width(200.)
                .height(100.)
                .children(child),
            |style| {
                parent_rect.set(Some(use_layout(style).rect));
                node::set_child_offset(*offset);
            },
        );
        let parent_ref = *parent;
        let root = layout_node(Style::new().width(400.).height(300.).children(parent));

        root.prepare_layout(AVAILABLE);

        let expected_parent = Rect::new(40., 25., 200., 100.);
        let expected_child = Rect::new(72., 43., 20., 12.);
        assert_eq!(rect(parent_ref).get(), expected_parent);
        assert_eq!(rect(child_ref).get(), expected_child);
        assert!(!seen.borrow().is_empty());
        assert!(
            seen.borrow()
                .iter()
                .all(|bounds| { *bounds == (expected_parent, expected_child) }),
            "child layout hooks must see the already published absolute parent and child rects"
        );
        assert_eq!(
            hit_path(*root, vec2(80., 48.)),
            vec![child_ref.id, parent_ref.id, root.id]
        );
        assert_eq!(
            hit_path(*root, vec2(98., 60.)),
            vec![parent_ref.id, root.id]
        );
    });
}

#[test]
fn layoutless_nodes_prepare_hooks_in_parent_first_order_without_drawing() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let child = Node::leaf(Style::new(), {
        let calls = calls.clone();
        move |style| {
            style
                .on_layout
                .append(move |_| calls.borrow_mut().push("child"));
            style
                .on_render
                .append(|_| panic!("prepare must not render"));
        }
    });
    let root = Node::leaf(Style::new().children(child), {
        let calls = calls.clone();
        move |style| {
            style
                .on_layout
                .append(move |_| calls.borrow_mut().push("parent"));
            style
                .on_update
                .append(|_| panic!("prepare must not advance time"));
        }
    });

    root.prepare_layout(AVAILABLE);
    root.prepare_layout(AVAILABLE);

    assert!(root.layout_id().is_none());
    assert_eq!(
        calls.borrow().as_slice(),
        &["parent", "child", "parent", "child"]
    );
    assert!(hit_test(*root, vec2(1., 1.)).is_empty());
}

#[test]
fn rect_observer_width_change_is_relaid_out_before_prepare_returns() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(Size::Px(40.));
        let child = layout_node(Style::new().width(Size::Percent(50.)).height(10.));
        let child_ref = *child;
        let root = Node::leaf(
            Style::new().width(width).height(30.).children(child),
            |style| {
                let layout = use_layout(style);
                watch(layout.rect, move |rect| {
                    if rect.get().w == 40. {
                        width.set(Size::Px(80.));
                    }
                });
            },
        );

        root.prepare_layout(AVAILABLE);

        assert_eq!(rect(*root).get(), Rect::new(0., 0., 80., 30.));
        assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 40., 10.));
        assert_eq!(hit_path(*root, vec2(35., 5.)), vec![child_ref.id, root.id]);
    });
}

#[test]
fn rect_observer_offset_change_converges_even_when_taffy_stays_clean() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(Vector::ZERO);
        let observed_clean = Rc::new(Cell::new(false));
        let child = layout_node(
            Style::new()
                .absolute()
                .inset_left(40.)
                .inset_top(20.)
                .width(20.)
                .height(15.),
        );
        let child_ref = *child;
        let root = Node::leaf(Style::new().width(100.).height(60.).children(child), {
            let observed_clean = observed_clean.clone();
            move |style| {
                let layout = use_layout(style);
                node::set_child_offset(*offset);
                watch(layout.rect, move |_| {
                    offset.set(vec2(30., 5.));
                    observed_clean.set(Runtime::with(|rt| {
                        !rt.taffy.borrow().dirty(layout.layout_id).unwrap()
                    }));
                });
            }
        });

        root.prepare_layout(AVAILABLE);

        assert!(
            observed_clean.get(),
            "offset writes alone must leave Taffy clean"
        );
        assert_eq!(rect(child_ref).get(), Rect::new(10., 15., 20., 15.));
        assert_eq!(hit_path(*root, vec2(15., 20.)), vec![child_ref.id, root.id]);
        assert_eq!(hit_path(*root, vec2(55., 25.)), vec![root.id]);
    });
}

#[test]
fn child_layout_hook_can_move_an_already_published_sibling_via_parent_offset() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(Vector::ZERO);
        let target = layout_node(
            Style::new()
                .absolute()
                .inset_left(50.)
                .inset_top(30.)
                .width(20.)
                .height(10.),
        );
        let target_ref = *target;
        let mut changed = false;
        let mutator = layout_node(Style::new().absolute().width(10.).height(10.).on_layout(
            move |_| {
                if !changed {
                    changed = true;
                    offset.set(vec2(12., 8.));
                }
            },
        ));
        let mutator_ref = *mutator;
        let root = Node::leaf(
            Style::new()
                .width(150.)
                .height(90.)
                .children((target, mutator)),
            |style| {
                use_layout(style);
                node::set_child_offset(*offset);
            },
        );

        root.prepare_layout(AVAILABLE);

        assert_eq!(rect(target_ref).get(), Rect::new(38., 22., 20., 10.));
        assert_eq!(rect(mutator_ref).get(), Rect::new(-12., -8., 10., 10.));
        assert_eq!(
            hit_path(*root, vec2(40., 25.)),
            vec![target_ref.id, root.id]
        );
    });
}

#[test]
fn rect_observer_mounts_children_with_geometry_and_inherited_fonts_in_the_same_prepare() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let mounted = Rc::new(Cell::new(None));
        let children = map_keyed(rows, |key| *key, {
            let mounted = mounted.clone();
            move |_| {
                let child = Node::leaf(
                    Style::new()
                        .absolute()
                        .inset_left(30.)
                        .inset_top(15.)
                        .font_size(FontSize::Em(1.5)),
                    |style| {
                        let font_size = node::use_font_size();
                        let layout = use_layout(style);
                        layout.set_measure_fn(move |known: taffy::Size<Option<f32>>, _| {
                            taffy::Size {
                                width: known.width.unwrap_or(font_size.get() * 2.),
                                height: known.height.unwrap_or(font_size.get()),
                            }
                        });
                    },
                );
                mounted.set(Some(*child));
                child
            }
        });
        let root = Node::leaf(
            Style::new()
                .width(160.)
                .height(100.)
                .font_size(FontSize::Px(20.))
                .children(children),
            |style| {
                let layout = use_layout(style);
                watch(layout.rect, move |_| rows.set(Rc::from([1])));
            },
        );
        assert!(root.children().get_clone().is_empty());

        root.prepare_layout(AVAILABLE);

        let child = mounted.get().expect("rect observer mounted its child");
        assert!(root.children().get_clone().as_ref() == [child]);
        assert_eq!(rect(child).get(), Rect::new(30., 15., 60., 30.));
        assert_eq!(hit_path(*root, vec2(80., 35.)), vec![child.id, root.id]);
    });
}

#[test]
fn rect_observer_mounts_layoutless_children_without_dirtying_taffy() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let layouts = Rc::new(Cell::new(0));
        let mounted = Rc::new(Cell::new(None::<NodeRef>));
        let children = map_keyed(rows, |key| *key, {
            let layouts = layouts.clone();
            let mounted = mounted.clone();
            move |_| {
                let layouts = layouts.clone();
                let child = Node::leaf(
                    Style::new().on_layout(move |_| {
                        layouts.set(layouts.get() + 1);
                    }),
                    |_| {},
                );
                mounted.set(Some(*child));
                child
            }
        });
        let wrapper = Node::leaf(Style::new().children(children), |_| {});
        let wrapper_ref = *wrapper;
        let root = Node::leaf(
            Style::new().width(100.).height(50.).children(wrapper),
            |style| {
                let layout = use_layout(style);
                watch(layout.rect, move |_| rows.set(Rc::from([1])));
            },
        );

        root.prepare_layout(AVAILABLE);

        let child = mounted
            .get()
            .expect("rect observer mounted layoutless child");
        assert!(child.layout_id().is_none());
        assert!(wrapper_ref.children().get_clone().as_ref() == [child]);
        assert!(
            layouts.get() > 0,
            "new layoutless child must receive on_layout before prepare returns"
        );
        assert_eq!(hit_path(*root, vec2(5., 5.)), vec![root.id]);
    });
}

#[test]
fn prepare_resolves_font_changes_from_rect_observers_before_returning() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let font = signal(FontSize::Px(10.));
        let child = Node::leaf(Style::new().font_size(FontSize::Em(2.)), |style| {
            let font_size = node::use_font_size();
            let layout = use_layout(style);
            layout.set_measure_fn(move |known: taffy::Size<Option<f32>>, _| taffy::Size {
                width: known.width.unwrap_or(font_size.get() * 2.),
                height: known.height.unwrap_or(font_size.get()),
            });
        });
        let child_ref = *child;
        let root = Node::leaf(
            Style::new()
                .width(160.)
                .height(100.)
                .items_start()
                .font_size(font)
                .children(child),
            |style| {
                let layout = use_layout(style);
                watch(layout.rect, move |_| font.set(FontSize::Px(18.)));
            },
        );

        root.prepare_layout(AVAILABLE);

        assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 72., 36.));
        assert_eq!(hit_path(*root, vec2(65., 30.)), vec![child_ref.id, root.id]);
    });
}

#[test]
fn layout_sync_feedback_converges_before_rects_and_layout_hooks_are_published() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(Size::Px(40.));
        let published = Rc::new(RefCell::new(Vec::new()));
        let root = Node::leaf(Style::new().width(width).height(20.), {
            let published = published.clone();
            move |style| {
                let layout = use_layout(style);
                node::set_layout_sync(move |size| {
                    if size.x == 40. {
                        width.set(Size::Px(80.));
                        true
                    } else {
                        false
                    }
                });
                style
                    .on_layout
                    .append(move |_| published.borrow_mut().push(layout.rect.get()));
            }
        });

        root.prepare_layout(AVAILABLE);

        let expected = Rect::new(0., 0., 80., 20.);
        assert_eq!(rect(*root).get(), expected);
        assert!(!published.borrow().is_empty());
        assert!(published.borrow().iter().all(|rect| *rect == expected));
        assert_eq!(hit_path(*root, vec2(70., 10.)), vec![root.id]);
    });
}

#[test]
fn repeated_prepare_does_not_advance_or_render_and_one_update_reaches_same_frame_geometry() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(Size::Px(40.));
        let updates = Rc::new(RefCell::new(Vec::new()));
        let child = layout_node(
            Style::new()
                .width(width)
                .height(20.)
                .on_update({
                    let updates = updates.clone();
                    move |dt| updates.borrow_mut().push(("child", *dt))
                })
                .on_render(|_| panic!("preparing and updating must not render")),
        );
        let child_ref = *child;
        let root = layout_node(
            Style::new()
                .width(150.)
                .height(60.)
                .children(child)
                .on_update({
                    let updates = updates.clone();
                    move |dt| {
                        updates.borrow_mut().push(("parent", *dt));
                        width.set(Size::Px(40. + dt * 100.));
                    }
                }),
        );

        for _ in 0..3 {
            root.prepare_layout(AVAILABLE);
        }
        assert!(updates.borrow().is_empty());
        assert_eq!(rect(child_ref).get().w, 40.);

        root.advance_frame(0.25);
        root.prepare_layout(AVAILABLE);
        root.prepare_layout(AVAILABLE);

        assert_eq!(
            updates.borrow().as_slice(),
            &[("parent", 0.25), ("child", 0.25)]
        );
        assert_eq!(rect(child_ref).get(), Rect::new(0., 0., 65., 20.));
        assert_eq!(hit_path(*root, vec2(60., 10.)), vec![child_ref.id, root.id]);
    });
}

#[test]
fn frame_updates_skip_disposed_snapshot_nodes_and_tick_new_children_next_frame() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([0]));
        let mounted = Rc::new(Cell::new(None::<NodeRef>));
        let updates = Rc::new(RefCell::new(Vec::new()));
        let children = map_keyed(rows, |key| *key, {
            let mounted = mounted.clone();
            let updates = updates.clone();
            move |key| {
                let key = *key;
                let updates = updates.clone();
                let child = layout_node(Style::new().width(20.).height(10.).on_update(move |_| {
                    updates.borrow_mut().push(key);
                }));
                mounted.set(Some(*child));
                child
            }
        });
        let mut replaced = false;
        let root = layout_node(
            Style::new()
                .width(100.)
                .height(50.)
                .children(children)
                .on_update({
                    let mounted = mounted.clone();
                    move |_| {
                        if !replaced {
                            replaced = true;
                            mounted.get().unwrap().dispose();
                            rows.set(Rc::from([1]));
                        }
                    }
                }),
        );
        root.prepare_layout(AVAILABLE);
        let old_child = mounted.get().unwrap();

        root.advance_frame(0.25);
        root.prepare_layout(AVAILABLE);

        let new_child = mounted.get().unwrap();
        assert_ne!(old_child.id, new_child.id);
        assert!(
            updates.borrow().is_empty(),
            "neither the disposed child nor the newly mounted child ticks in this frame"
        );
        assert_eq!(hit_path(*root, vec2(5., 5.)), vec![new_child.id, root.id]);

        root.advance_frame(0.25);

        assert_eq!(updates.borrow().as_slice(), &[1]);
    });
}

#[test]
fn render_reads_prepared_geometry_without_running_layout_or_update_hooks() {
    let layouts = Rc::new(Cell::new(0));
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let root = Node::leaf(Style::new().width(40.).height(20.), {
        let layouts = layouts.clone();
        let rendered = rendered.clone();
        move |style| {
            let layout = use_layout(style);
            style
                .on_layout
                .append(move |_| layouts.set(layouts.get() + 1));
            style
                .on_update
                .append(|_| panic!("render must not advance time"));
            style
                .on_render
                .append(move |_| rendered.borrow_mut().push(layout.rect.get()));
        }
    });
    root.prepare_layout(AVAILABLE);
    let prepared_layouts = layouts.get();
    assert!(rendered.borrow().is_empty());

    root.render();
    root.render();

    assert_eq!(layouts.get(), prepared_layouts);
    assert_eq!(
        rendered.borrow().as_slice(),
        &[Rect::new(0., 0., 40., 20.); 2]
    );
}
