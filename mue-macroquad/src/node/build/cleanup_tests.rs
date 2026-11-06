use super::*;
use crate::{
    layout::use_layout,
    node::{self},
    runtime::Runtime,
    style::{Styleable, StyleableExt},
};
use mue_core::runtime::resource_counts;

#[mue_macros::node]
fn checked_leaf(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().width(12.), move |style| {
        use_layout(style);
    })
}

fn ui_counts() -> (usize, usize, usize, usize) {
    Runtime::with(|rt| {
        (
            rt.nodes.borrow().len(),
            rt.gestures.borrow().len(),
            rt.taffy.borrow().total_node_count(),
            node::CONTEXT.with_borrow(Vec::len),
        )
    })
}

#[test]
fn unbuilt_builder_drop_disposes_hover_and_derived_resources_without_creating_a_node() {
    let before = resource_counts();
    let ui_before = ui_counts();
    let mut alias = None;
    let mut derived = None;
    let builder = checked_leaf().with_hovered(|builder, hovered| {
        alias = Some(hovered);
        let width = hovered.map(|value| if value { 17. } else { 12. });
        derived = Some(width);
        builder.width(width)
    });
    assert!(resource_counts().signals > before.signals);
    assert_eq!(ui_counts(), ui_before);
    drop(builder);
    assert!(alias.unwrap().is_disposed());
    assert!(derived.unwrap().is_disposed());
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}

#[test]
fn dropping_node_build_disposes_its_scope_and_accumulated_hover_styles() {
    let before = resource_counts();
    let ui_before = ui_counts();
    let mut ctx = NodeContext::new();
    let local = ctx.scope().run(|| signal(17));
    let mut alias = None;
    let style = Style::new().with_hovered(|style, hovered| {
        alias = Some(hovered);
        style.width(hovered.map(|value| if value { 17. } else { 12. }))
    });
    ctx.push_style(style);
    assert!(resource_counts().signals > before.signals);
    assert_eq!(ui_counts(), ui_before);
    drop(ctx);
    assert!(local.is_disposed());
    assert!(alias.unwrap().is_disposed());
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}
