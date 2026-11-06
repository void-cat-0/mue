use super::*;
use crate::{
    layout::use_layout,
    math::{Matrix, Vector},
    node::IntoNode,
    runtime::Runtime,
    style::{FontSize, Styleable},
};
use mue_core::{runtime::resource_counts, signal::Access};

#[mue_macros::node]
fn visual_leaf(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().width(11.).height(12.), move |style| {
        use_layout(style);
    })
}

#[test]
fn explicit_visual_fields_reach_layout_font_and_transform() {
    let before = resource_counts();
    let transform = Matrix::from_translation(Vector::new(4., 5.));
    let leaf = visual_leaf()
        .width(57.)
        .font_size(FontSize::Px(19.))
        .transform(transform)
        .on_render(|_| {})
        .into_node();
    let layout = leaf.layout_id().unwrap();
    leaf.resolve_font_sizes();
    Runtime::with(|rt| {
        let style = rt.taffy.borrow().style(layout).unwrap().clone();
        assert_eq!(style.size.width, taffy::Dimension::length(57.));
        assert_eq!(style.size.height, taffy::Dimension::length(12.));
        let node = rt.node(leaf.id);
        assert_eq!(node.computed_font_size.get(), 19.);
        assert_eq!(node.matrix_transform.get(), transform);
    });
    drop(leaf);
    assert_eq!(resource_counts(), before);
}
