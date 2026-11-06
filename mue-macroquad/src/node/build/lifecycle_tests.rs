use super::*;
use crate::{
    layout::use_layout,
    math::{Matrix, Vector},
    node::{self, ChildrenPlan, IntoChildren, IntoNode},
    paint::{self, Paint},
    runtime::Runtime,
    style::{FontSize, Size, Styleable, StyleableExt},
    transition::{self, Transition},
};
use mue_core::{
    Disposable,
    effect::computed,
    prop::Prop,
    runtime::resource_counts,
    scope::OwnedScope,
    signal::{Access, signal},
};
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WIDTHS: RefCell<Option<(Prop<Size>, Prop<Size>)>> = const { RefCell::new(None) };
    static PAINTS: RefCell<Vec<Paint>> = const { RefCell::new(Vec::new()) };
    static RENDERED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn ui_counts() -> (usize, usize, usize, usize) {
    Runtime::with(|rt| {
        (
            rt.nodes.borrow().len(),
            rt.taffy.borrow().total_node_count(),
            rt.gestures.borrow().len(),
            node::CONTEXT.with_borrow(Vec::len),
        )
    })
}

#[mue_macros::node]
fn width_consumer(ctx: NodeContext, value: f32) -> Node {
    ctx.finish(
        Style::new()
            .width(value.map(|value| value * 2.))
            .transition(Transition::linear(1.)),
        move |style| {
            assert_eq!(
                style.transition.as_ref().unwrap().get(),
                Transition::linear(1.)
            );
            let declared = style.width.clone().expect("declared inline width");
            let taken = style.take_width();
            WIDTHS.set(Some((declared, taken)));
        },
    )
}

#[test]
fn inline_width_consumers_share_the_default_transition_and_mount_is_synchronous() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let source = signal(5.0f32);
        let node = width_consumer(source).into_node();
        let (declared, taken) = WIDTHS
            .take()
            .expect("body completed before into_node returned");
        assert_eq!(declared.get(), Size::Px(10.));
        assert_eq!(taken.get(), Size::Px(10.));
        source.set(25.0);
        assert_eq!(declared.get(), Size::Px(10.));
        assert_eq!(taken.get(), Size::Px(10.));
        transition::advance(0.25);
        assert_eq!(declared.get(), Size::Px(20.));
        assert_eq!(taken.get(), Size::Px(20.));
        transition::advance(0.75);
        assert_eq!(declared.get(), Size::Px(50.));
        assert_eq!(taken.get(), Size::Px(50.));
        drop(node);
        assert!(!source.is_disposed());
    });
    drop(owner);
    assert_eq!(resource_counts(), before);

    let local_label = String::from("borrowed by mount");
    let mut calls = Vec::new();
    let mounted = NodeContext::new().finish(Style::new().height(12.), |style| {
        calls.push(local_label.as_str());
        assert_eq!(style.take_height().get(), Size::Px(12.));
    });
    assert_eq!(calls, vec!["borrowed by mount"]);
    drop(mounted);
    assert_eq!(resource_counts(), before);
}

fn default_matrix() -> Matrix {
    Matrix::from_translation(Vector::new(4., 9.))
}

#[mue_macros::node]
fn default_visuals(ctx: NodeContext) -> Node {
    ctx.finish(
        Style::new()
            .font_size(FontSize::Px(23.))
            .transform(default_matrix())
            .width(11.)
            .height(13.),
        move |style| {
            use_layout(style);
        },
    )
}

#[test]
fn author_default_font_and_transform_reach_the_mounted_node() {
    let before = resource_counts();
    let node = default_visuals().into_node();
    node.resolve_font_sizes();
    Runtime::with(|rt| {
        let inner = rt.node(node.id);
        assert_eq!(inner.font_size.as_ref().unwrap().get(), FontSize::Px(23.));
        assert_eq!(inner.computed_font_size.get(), 23.);
        assert_eq!(inner.matrix_transform.get(), default_matrix());
        assert_eq!(inner.effective_transform(), default_matrix());
        let taffy = rt.taffy.borrow();
        let style = taffy.style(inner.layout_id.unwrap()).unwrap();
        assert_eq!(style.size.width, taffy::Dimension::length(11.));
        assert_eq!(style.size.height, taffy::Dimension::length(13.));
    });
    drop(node);
    assert_eq!(resource_counts(), before);
}

#[mue_macros::node]
fn paint_child(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().opacity(0.25), move |style| {
        let first = paint::use_paint(style);
        let second = paint::use_paint(style);
        assert_eq!(first.effective_opacity(), 0.125);
        assert_eq!(second.effective_opacity(), 0.125);
        PAINTS.with_borrow_mut(|paints| paints.extend([first, second]));
    })
}

#[mue_macros::node]
fn opacity_parent(ctx: NodeContext, manual: bool, #[model] alpha: f32) -> Node {
    ctx.finish(Style::new().opacity(alpha), move |_| {
        if manual.get() {
            let child = paint_child().into_node();
            node::set_children(child.into_children());
        }
    })
}

#[test]
fn parent_default_opacity_precedes_declared_and_manually_constructed_children() {
    for manual in [false, true] {
        let before = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let alpha = signal(0.5);
            let builder = opacity_parent(manual, alpha);
            let parent = if manual {
                builder.into_node()
            } else {
                builder.children(paint_child()).into_node()
            };
            assert_eq!(parent.children().get_clone().len(), 1);
            alpha.set(0.75);
            PAINTS.with_borrow(|paints| {
                assert_eq!(paints.len(), 2);
                for paint in paints {
                    assert_eq!(paint.effective_opacity(), 0.1875);
                }
            });
            drop(parent);
            PAINTS.with_borrow_mut(Vec::clear);
            assert!(!alpha.is_disposed());
        });
        drop(owner);
        assert_eq!(resource_counts(), before, "manual={manual}");
    }
}

#[test]
fn hover_defaults_configured_before_mount_live_until_the_node_drops() {
    let before = resource_counts();
    let mut hovered = None;
    let mut width = None;
    let defaults = Style::new().with_hovered(|style, value| {
        hovered = Some(value);
        style.width(value.map(|value| if value { 30. } else { 10. }))
    });
    let node = NodeContext::new().finish(defaults, |style| {
        width = Some(style.take_width());
    });
    let hovered = hovered.unwrap();
    let width = width.unwrap();
    assert_eq!(width.get(), Size::Px(10.));
    Runtime::with(|rt| rt.node(node.id).hovered.unwrap().set(true));
    assert!(hovered.get());
    assert_eq!(width.get(), Size::Px(30.));
    drop(node);
    assert!(hovered.is_disposed());
    assert_eq!(resource_counts(), before);
}

#[mue_macros::node]
fn layout_child(ctx: NodeContext, width: f32) -> Node {
    ctx.finish(Style::new().width(width).height(5.), move |style| {
        use_layout(style);
    })
}

#[mue_macros::node]
fn wrapping_parent(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().width(100.), move |style| {
        let original = node::take_children();
        let reversed = computed(move |_| {
            original
                .get_clone()
                .iter()
                .rev()
                .copied()
                .collect::<Rc<[_]>>()
        })
        .owned();
        let wrapper = node::flexbox().children(reversed).into_node();
        node::set_children(wrapper.into_children());
        use_layout(style);
    })
}

#[test]
fn take_children_reorganization_is_the_real_tree_seen_by_use_layout() {
    let before = resource_counts();
    let parent = wrapping_parent()
        .children((layout_child(10.), layout_child(20.)))
        .into_node();
    let children = parent.children().get_clone();
    assert_eq!(children.len(), 1);
    let wrapper = children[0];
    let leaves = wrapper.children().get_clone();
    assert_eq!(leaves.len(), 2);
    Runtime::with(|rt| {
        let parent_layout = rt.node(parent.id).layout_id.unwrap();
        let wrapper_layout = rt.node(wrapper.id).layout_id.unwrap();
        let leaf_layouts: Vec<_> = leaves
            .iter()
            .map(|leaf| rt.node(leaf.id).layout_id.unwrap())
            .collect();
        let taffy = rt.taffy.borrow();
        assert_eq!(taffy.children(parent_layout).unwrap(), vec![wrapper_layout]);
        assert_eq!(taffy.children(wrapper_layout).unwrap(), leaf_layouts);
        let widths: Vec<_> = leaf_layouts
            .iter()
            .map(|id| taffy.style(*id).unwrap().size.width)
            .collect();
        assert_eq!(
            widths,
            vec![taffy::Dimension::length(20.), taffy::Dimension::length(10.)]
        );
    });
    drop(parent);
    assert_eq!(resource_counts(), before);
}

#[mue_macros::node]
fn mounted_hook(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().width(12.), move |style| {
        let _ = style.take_width();
        style
            .on_render
            .append(|_| RENDERED.with_borrow_mut(|rendered| rendered.push("init")));
    })
}

#[test]
fn init_hooks_append_after_inline_hooks_and_are_retained_by_the_node() {
    let before = resource_counts();
    let ui_before = ui_counts();
    RENDERED.with_borrow_mut(Vec::clear);
    let node = mounted_hook()
        .on_render(|_| RENDERED.with_borrow_mut(|rendered| rendered.push("inline")))
        .into_node();
    Runtime::with(|rt| rt.nodes.borrow_mut()[node.id].hooks.render.invoke(&()));
    RENDERED.with_borrow(|rendered| assert_eq!(rendered.as_slice(), &["inline", "init"]));
    drop(node);
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}

#[mue_macros::node]
fn empty_parent(ctx: NodeContext) -> Node {
    ctx.finish(Style::new(), move |_| {})
}

#[mue_macros::node]
fn hooks_only_parent(ctx: NodeContext) -> Node {
    ctx.finish(Style::new(), move |style| {
        style
            .on_render
            .append(|_| RENDERED.with_borrow_mut(|rendered| rendered.push("only")));
    })
}

#[test]
fn empty_and_hooks_only_nodes_mount_their_declared_children() {
    let before = resource_counts();
    let ui_before = ui_counts();
    let empty = empty_parent().children(layout_child(15.)).into_node();
    let hooks = hooks_only_parent().children(layout_child(16.)).into_node();
    for node in [&empty, &hooks] {
        assert!(node.layout_id().is_none());
        let children = node.children().get_clone();
        assert_eq!(children.len(), 1);
        assert!(children[0].layout_id().is_some());
    }
    RENDERED.with_borrow_mut(Vec::clear);
    Runtime::with(|rt| rt.nodes.borrow_mut()[hooks.id].hooks.render.invoke(&()));
    RENDERED.with_borrow(|rendered| assert_eq!(rendered.as_slice(), &["only"]));
    drop((empty, hooks));
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}

#[test]
fn declared_children_plan_is_deferred_until_finish_and_materializes_before_the_callback() {
    let before = resource_counts();
    let ui_before = ui_counts();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let child_calls = calls.clone();
    let defaults = Style::new().children(ChildrenPlan::new(move || {
        child_calls.borrow_mut().push("children");
        layout_child(4.).into_children()
    }));
    assert!(calls.borrow().is_empty());
    let mounted = NodeContext::new().finish(defaults, |style| {
        calls.borrow_mut().push("init");
        let children = node::take_children();
        let refs = children.get_clone();
        assert_eq!(refs.len(), 1);
        assert!(refs[0].layout_id().is_some());
        node::set_children(children);
        use_layout(style);
    });
    assert_eq!(calls.borrow().as_slice(), &["children", "init"]);
    assert_eq!(mounted.children().get_clone().len(), 1);
    drop(mounted);
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}
