use mue_components::prelude::*;
use mue_core::{
    prelude::*,
    scope::{OwnedScope, inject, provide},
};
use mue_macroquad::{
    color::Color,
    inherit::{inherited_foreground, provide_foreground},
    layout::with_taffy,
    node::Node,
    shader::Brush,
    style::Size,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct TestNode<F>(F);

fn test_node<F: FnOnce(&mut Style)>(body: F) -> TestNode<F> {
    TestNode(body)
}

impl<F: FnOnce(&mut Style)> BuildNode for TestNode<F> {
    fn build(self, ctx: NodeContext) -> Node {
        ctx.finish(Style::new(), self.0)
    }
}

#[node]
fn counted_node(ctx: NodeContext, #[model] bodies: usize, #[model] finishes: usize) -> Node {
    bodies.set(bodies.get() + 1);
    ctx.finish(Style::new(), move |style| {
        mue_macroquad::node::flexbox::use_container(style);
        finishes.set(finishes.get() + 1);
    })
}

#[component]
fn provided_stack() -> impl Component {
    provide(17u32);
    stack()
}

#[test]
fn constructing_node_builder_does_nothing_and_into_node_executes_once() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let bodies = signal(0usize);
        let finishes = signal(0usize);
        drop(counted_node(bodies, finishes));
        assert_eq!((bodies.get(), finishes.get()), (0, 0));

        let builder = counted_node(bodies, finishes).width(24.);
        assert_eq!((bodies.get(), finishes.get()), (0, 0));
        let node = builder.into_node();
        assert_eq!((bodies.get(), finishes.get()), (1, 1));
        assert!(node.layout_id().is_some());
        drop(node);
        assert_eq!((bodies.get(), finishes.get()), (1, 1));
        assert!(!bodies.is_disposed());
        assert!(!finishes.is_disposed());

        let initialized = Cell::new(false);
        let node = test_node(|_| initialized.set(true)).into_node();
        assert!(
            initialized.get(),
            "finish must run the borrowed body synchronously"
        );
        drop(node);
    });
}

#[test]
fn ordinary_children_wait_for_the_parent_component_context() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let finishes = Rc::new(Cell::new(0));
        let view = provided_stack().children({
            let finishes = finishes.clone();
            test_node(move |_| {
                assert_eq!(inject::<u32>(), Some(17));
                finishes.set(finishes.get() + 1);
            })
        });
        assert_eq!(finishes.get(), 0);
        assert_eq!(inject::<u32>(), None);
        let node = view.into_node();
        assert_eq!(finishes.get(), 1);
        assert_eq!(inject::<u32>(), None);
        drop(node);
    });
}

#[test]
fn node_defaults_remain_reactive_and_explicit_styles_win() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let orientation = signal(Orientation::Horizontal);
        let thickness = signal(Size::Px(2.));
        let node = separator()
            .orientation(orientation)
            .thickness(thickness)
            .into_node();
        let id = node.layout_id().unwrap();
        let dimensions = || with_taffy(|tree| tree.style(id).unwrap().size);
        assert_eq!(dimensions().height, taffy::Dimension::length(2.));
        orientation.set(Orientation::Vertical);
        assert_eq!(dimensions().width, taffy::Dimension::length(2.));
        thickness.set(Size::Px(5.));
        assert_eq!(dimensions().width, taffy::Dimension::length(5.));
        assert_eq!(node.layout_id(), Some(id));

        let explicit = separator().thickness(thickness).height(9.).into_node();
        let explicit_id = explicit.layout_id().unwrap();
        thickness.set(Size::Px(7.));
        assert_eq!(
            with_taffy(|tree| tree.style(explicit_id).unwrap().size.height),
            taffy::Dimension::length(9.)
        );
        let stack = stack().into_node();
        assert_eq!(
            with_taffy(|tree| tree
                .style(stack.layout_id().unwrap())
                .unwrap()
                .flex_direction),
            taffy::FlexDirection::Column
        );
    });
}

#[component]
fn row_stack() -> impl Component {
    stack().flex_row().gap(5.)
}

#[test]
fn layout_wrappers_keep_their_presets_below_outer_and_caller_styles() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let centered = center().into_node();
        let centered_style =
            with_taffy(|tree| tree.style(centered.layout_id().unwrap()).unwrap().clone());
        assert_eq!(centered_style.align_items, Some(taffy::AlignItems::Center));
        assert_eq!(
            centered_style.justify_content,
            Some(taffy::JustifyContent::Center)
        );

        let wrapped = wrap().into_node();
        let wrapped_style =
            with_taffy(|tree| tree.style(wrapped.layout_id().unwrap()).unwrap().clone());
        assert_eq!(wrapped_style.flex_wrap, taffy::FlexWrap::Wrap);
        assert_eq!(wrapped_style.gap.width, taffy::LengthPercentage::length(8.));
        assert_eq!(
            wrapped_style.gap.height,
            taffy::LengthPercentage::length(8.)
        );

        let column = container().into_node();
        let column_style =
            with_taffy(|tree| tree.style(column.layout_id().unwrap()).unwrap().clone());
        assert_eq!(column_style.flex_direction, taffy::FlexDirection::Column);
        assert_eq!(column_style.max_size.width, taffy::Dimension::length(1280.));
        assert_eq!(
            column_style.padding.left,
            taffy::LengthPercentage::length(24.)
        );
        let overlay = link_overlay().into_node();
        assert_eq!(
            with_taffy(|tree| tree
                .style(overlay.layout_id().unwrap())
                .unwrap()
                .flex_direction),
            taffy::FlexDirection::Column
        );

        let outer = row_stack().into_node();
        let outer_style =
            with_taffy(|tree| tree.style(outer.layout_id().unwrap()).unwrap().clone());
        assert_eq!(outer_style.flex_direction, taffy::FlexDirection::Row);
        assert_eq!(outer_style.gap.width, taffy::LengthPercentage::length(5.));

        let caller = row_stack().flex_column().gap(3.).into_node();
        let caller_style =
            with_taffy(|tree| tree.style(caller.layout_id().unwrap()).unwrap().clone());
        assert_eq!(caller_style.flex_direction, taffy::FlexDirection::Column);
        assert_eq!(caller_style.gap.width, taffy::LengthPercentage::length(3.));
    });
}

#[test]
fn pages_defer_children_and_retain_them_across_selection_changes() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let finishes = Rc::new(Cell::new(0));
        let selected = signal(0usize);
        let page = {
            let finishes = finishes.clone();
            test_node(move |_| finishes.set(finishes.get() + 1))
        };
        let view = pages(selected, [page]).page_transition(PagesTransition::None);
        assert_eq!(finishes.get(), 0);
        let node = view.into_node();
        assert_eq!(finishes.get(), 1);
        selected.set(1);
        selected.set(0);
        assert_eq!(finishes.get(), 1);
        drop(node);
        assert!(!selected.is_disposed());
    });
}

#[test]
fn delayed_navigation_inherits_context_and_retains_common_screens() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let path = signal(Vec::<u8>::new());
        let foreground = signal(Brush::Solid(Color::WHITE));
        let mounted = Rc::new(RefCell::new(Vec::<(u8, Prop<Brush>)>::new()));
        let node = provide_foreground(foreground, {
            let mounted = mounted.clone();
            move || {
                provide(17u32);
                navigation_stack(
                    path,
                    || stack(),
                    move |&value| {
                        let mounted = mounted.clone();
                        test_node(move |_| {
                            assert_eq!(inject::<u32>(), Some(17));
                            mounted
                                .borrow_mut()
                                .push((value, inherited_foreground().unwrap()));
                        })
                    },
                )
                .navigation_transition(NavigationTransition::None)
            }
        })
        .into_node();
        assert!(mounted.borrow().is_empty());
        path.set(vec![1]);
        path.set(vec![1, 2]);
        assert_eq!(
            mounted
                .borrow()
                .iter()
                .map(|(value, _)| *value)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(
            mounted
                .borrow()
                .iter()
                .all(|(_, color)| color.get_clone() == Brush::Solid(Color::WHITE))
        );
        foreground.set(Brush::Solid(Color::BLACK));
        assert!(
            mounted
                .borrow()
                .iter()
                .all(|(_, color)| color.get_clone() == Brush::Solid(Color::BLACK))
        );
        path.set(vec![1]);
        assert_eq!(mounted.borrow().len(), 2);
        drop(node);
        assert!(!path.is_disposed());
    });
}
