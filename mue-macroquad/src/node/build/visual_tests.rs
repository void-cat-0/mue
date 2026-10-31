use super::*;
use crate::{
    layout::use_layout,
    math::Vector,
    node::IntoNode,
    runtime::Runtime,
    style::{FontSize, Styleable},
    theme::{Rule, Selector, Specificity},
};
use mue_core::{scope::OwnedScope, signal::Access};

#[mue_macros::node]
#[theme]
fn visual_leaf() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().width(11.).height(12.), |style| {
        use_layout(style);
    })
}

#[test]
fn dynamic_theme_new_fields_reach_layout_font_transform_and_paint_without_remount() {
    let owner = OwnedScope::new();
    let theme = owner.run(|| signal(Theme::default()));
    let leaf = AppBuildContext::with_theme(theme.into(), || {
        let plan = visual_leaf().on_render(|_| {});
        plan.into_node()
    });
    let layout = leaf.layout_id().unwrap();
    let read_width = || Runtime::with(|rt| rt.taffy.borrow().style(layout).unwrap().size.width);
    assert_eq!(read_width(), taffy::Dimension::length(11.));
    let appearance = Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |target| {
            target.is::<VisualLeaf>()
        })],
        || {
            Style::new()
                .width(57.)
                .font_size(FontSize::Px(19.))
                .transform(crate::math::Matrix::from_translation(Vector::new(4., 5.)))
                .opacity(0.4)
        },
    )]);
    theme.set(appearance);
    assert_eq!(read_width(), taffy::Dimension::length(57.));
    leaf.resolve_font_sizes();
    Runtime::with(|rt| {
        let node = rt.node(leaf.id);
        assert_eq!(node.computed_font_size.get(), 19.);
        assert_eq!(
            node.matrix_transform.get(),
            crate::math::Matrix::from_translation(Vector::new(4., 5.))
        );
    });
    theme.set(Theme::default());
    leaf.resolve_font_sizes();
    assert_eq!(read_width(), taffy::Dimension::length(11.));
    Runtime::with(|rt| {
        let node = rt.node(leaf.id);
        assert_eq!(node.computed_font_size.get(), FontSize::DEFAULT_PX);
        assert_eq!(node.matrix_transform.get(), crate::math::Matrix::IDENTITY);
    });
    drop(leaf);
    drop(owner);
}
