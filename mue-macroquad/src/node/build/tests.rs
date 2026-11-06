use std::{cell::RefCell, rc::Rc};

use mue_core::{
    prop::{Prop, PropValue},
    scope::{OwnedScope, current_scope},
    signal::{Access, ReadSignal, signal},
};

use super::*;
use crate::{
    layout::use_layout,
    node::{Component, IntoNode, map_keyed},
    runtime::Runtime,
    style::{Size, Styleable, StyleableExt},
};

#[derive(Default)]
struct Trace {
    builds: usize,
    inits: usize,
    components: usize,
    width: Option<Prop<Size>>,
    height: Option<Prop<Size>>,
    resources: Vec<ReadSignal<i32>>,
    early_hover: Vec<ReadSignal<bool>>,
    aliases: Vec<ReadSignal<bool>>,
    defaults: usize,
    clones: usize,
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

fn reset() {
    TRACE.set(Trace::default());
}
fn dimensions() -> (Size, Size) {
    TRACE.with_borrow(|trace| {
        (
            trace.width.as_ref().expect("width consumer").get(),
            trace.height.as_ref().expect("height consumer").get(),
        )
    })
}

#[mue_macros::node]
fn probe_leaf(ctx: NodeContext) -> Node {
    TRACE.with_borrow_mut(|trace| {
        trace.builds += 1;
        trace.resources.push(*signal(1));
    });
    ctx.finish(Style::new().width(10.).height(20.), move |style| {
        TRACE.with_borrow_mut(|trace| {
            trace.inits += 1;
            trace.width = Some(style.take_width());
            trace.height = Some(style.take_height());
            trace.resources.push(*signal(2));
        });
    })
}

#[mue_macros::component]
fn probe_outer() -> impl Component {
    TRACE.with_borrow_mut(|trace| {
        trace.components += 1;
        trace.resources.push(*signal(3));
    });
    probe_leaf().width(30.)
}

#[test]
fn outer_explicit_styles_precede_inner_styles_and_node_defaults() {
    reset();
    let outer = probe_outer().width(50.).into_node();
    assert_eq!(dimensions(), (Size::Px(50.), Size::Px(20.)));
    drop(outer);
    reset();
    let inner = probe_outer().into_node();
    assert_eq!(dimensions(), (Size::Px(30.), Size::Px(20.)));
    drop(inner);
    reset();
    let fallback = probe_leaf().into_node();
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    drop(fallback);
}

#[test]
fn configuration_is_lazy_and_into_node_builds_and_initializes_once_synchronously() {
    reset();
    let builder = probe_outer().width(50.).height(60.);
    TRACE.with_borrow(|trace| {
        assert_eq!((trace.components, trace.builds, trace.inits), (0, 0, 0));
        assert!(trace.resources.is_empty());
        assert!(trace.width.is_none());
    });
    let node = builder.into_node();
    assert_eq!(dimensions(), (Size::Px(50.), Size::Px(60.)));
    TRACE.with_borrow(|trace| {
        assert_eq!((trace.components, trace.builds, trace.inits), (1, 1, 1));
        assert_eq!(trace.resources.len(), 3);
        assert!(trace.resources.iter().all(|value| !value.is_disposed()));
    });
    drop(node);
    TRACE.with_borrow(|trace| {
        assert!(trace.resources.iter().all(|value| value.is_disposed()));
    });
}

#[test]
fn node_build_mount_borrows_local_data_and_retains_only_its_own_resources() {
    let parent = OwnedScope::new();
    parent.provide(17u32);
    let external = parent.run(|| signal(99));
    let ctx = parent.run(NodeContext::new);
    let body_resource = ctx.scope().run(|| signal(1));
    let label = String::from("local init");
    let mut calls = 0;
    let mut init_resource = None;
    let node = ctx.finish(Style::new(), |_| {
        calls += 1;
        assert_eq!(label.as_str(), "local init");
        assert_eq!(current_scope().inject::<u32>(), Some(17));
        init_resource = Some(signal(2));
    });
    assert_eq!(calls, 1);
    assert_eq!(body_resource.get(), 1);
    let init_resource = init_resource.unwrap();
    assert_eq!(init_resource.get(), 2);
    drop(node);
    assert!(body_resource.is_disposed());
    assert!(init_resource.is_disposed());
    assert_eq!(external.get(), 99);
}

#[derive(PartialEq)]
struct CloneProbe(i32);
impl Clone for CloneProbe {
    fn clone(&self) -> Self {
        TRACE.with_borrow_mut(|trace| trace.clones += 1);
        Self(self.0)
    }
}
impl PropValue for CloneProbe {}
fn default_probe() -> CloneProbe {
    TRACE.with_borrow_mut(|trace| trace.defaults += 1);
    CloneProbe(7)
}

#[mue_macros::node]
fn option_leaf(ctx: NodeContext, #[default(default_probe())] option: CloneProbe) -> Node {
    ctx.finish(Style::new().width(10.), move |style| {
        let _ = &option;
        TRACE.with_borrow_mut(|trace| trace.width = Some(style.take_width()));
    })
}

#[test]
fn defaults_are_lazy_evaluated_once_and_skipped_for_explicit_options_without_cloning() {
    reset();
    let builder = option_leaf();
    TRACE.with_borrow(|trace| assert_eq!((trace.defaults, trace.clones), (0, 0)));
    let node = builder.into_node();
    TRACE.with_borrow(|trace| assert_eq!((trace.defaults, trace.clones), (1, 0)));
    drop(node);
    reset();
    let node = option_leaf()
        .option(Prop::Static(CloneProbe(11)))
        .into_node();
    TRACE.with_borrow(|trace| assert_eq!((trace.defaults, trace.clones), (0, 0)));
    drop(node);
}

#[mue_macros::component]
fn hover_outer() -> impl Component {
    probe_leaf().with_hovered(|builder, hovered| {
        TRACE.with_borrow_mut(|trace| trace.aliases.push(hovered));
        builder
    })
}

#[test]
fn early_hover_aliases_survive_configuration_scope_then_drop_with_the_node() {
    reset();
    let configuration = OwnedScope::new();
    let builder = configuration.run(|| {
        hover_outer().with_hovered(|builder, hovered| {
            TRACE.with_borrow_mut(|trace| trace.early_hover.push(hovered));
            let width = hovered.map(|value| if value { 73. } else { 13. });
            builder.width(width)
        })
    });
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.early_hover.len(), 1);
        assert!(trace.aliases.is_empty());
        assert!(!trace.early_hover[0].get());
    });
    drop(configuration);
    let node = builder.into_node();
    assert_eq!(dimensions().0, Size::Px(13.));
    TRACE.with_borrow(|trace| {
        assert!(!trace.early_hover[0].is_disposed());
        assert!(!trace.aliases[0].is_disposed());
    });
    let aliases = Runtime::with(|rt| {
        let state = rt.nodes.borrow();
        let mounted = &state[node.id];
        std::iter::once(mounted.hovered.unwrap())
            .chain(mounted.hovered_aliases.iter().copied())
            .collect::<Vec<_>>()
    });
    for alias in aliases {
        alias.set(true);
    }
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].get());
        assert!(trace.aliases[0].get());
    });
    assert_eq!(dimensions().0, Size::Px(73.));
    drop(node);
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].is_disposed());
        assert!(trace.aliases[0].is_disposed());
    });
}

#[test]
fn unbuilt_hover_builder_drop_releases_early_resources_without_executing_bodies() {
    reset();
    let builder = probe_leaf().with_hovered(|builder, hovered| {
        TRACE.with_borrow_mut(|trace| trace.early_hover.push(hovered));
        builder.width(hovered.map(|value| if value { 73. } else { 13. }))
    });
    drop(builder);
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].is_disposed());
        assert_eq!((trace.builds, trace.inits), (0, 0));
        assert!(trace.resources.is_empty());
    });
}

#[mue_macros::node]
fn keyed_leaf(ctx: NodeContext, key: i32) -> Node {
    TRACE.with_borrow_mut(|trace| {
        trace.builds += 1;
        trace.resources.push(*signal(key.get()));
    });
    ctx.finish(Style::new().width(key.get() as f32), move |style| {
        TRACE.with_borrow_mut(|trace| trace.inits += 1);
        use_layout(style);
    })
}

#[test]
fn dynamic_subtrees_build_new_keys_synchronously_and_dispose_only_removed_nodes() {
    reset();
    let owner = OwnedScope::new();
    let keys = owner.run(|| signal(Rc::<[i32]>::from([10, 20])));
    let builder = crate::node::flexbox().width(77.).children(map_keyed(
        keys,
        |key| *key,
        |key| keyed_leaf(*key),
    ));
    TRACE.with_borrow(|trace| assert_eq!(trace.builds, 0));
    let node = builder.into_node();
    let initial = node.children().get_clone();
    assert_eq!(initial.len(), 2);
    let widths = |children: &[crate::node::NodeRef]| {
        Runtime::with(|rt| {
            children
                .iter()
                .map(|child| {
                    let layout = rt.node(child.id).layout_id.unwrap();
                    rt.taffy.borrow().style(layout).unwrap().size.width
                })
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        widths(&initial),
        vec![taffy::Dimension::length(10.), taffy::Dimension::length(20.)]
    );
    keys.set(Rc::from([20, 30]));
    let updated = node.children().get_clone();
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0].id, initial[1].id);
    assert_ne!(updated[1].id, initial[0].id);
    assert_eq!(
        widths(&updated),
        vec![taffy::Dimension::length(20.), taffy::Dimension::length(30.)]
    );
    TRACE.with_borrow(|trace| {
        assert_eq!((trace.builds, trace.inits), (3, 3));
        assert!(trace.resources[0].is_disposed());
        assert!(!trace.resources[1].is_disposed());
        assert!(!trace.resources[2].is_disposed());
    });
    drop(node);
    TRACE.with_borrow(|trace| assert!(trace.resources.iter().all(|value| value.is_disposed())));
    assert!(!keys.is_disposed());
}
