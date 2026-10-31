use mue_components::prelude::*;
use mue_core::{prelude::*, prop::PropValue, scope::OwnedScope};
use mue_macroquad::{
    layout::with_taffy,
    node::{PrepareNode, RootBuild},
    style::Size,
    theme::ComponentRef,
};
use std::{cell::Cell, collections::HashSet, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Density {
    Compact,
    #[default]
    Regular,
}
impl PropValue for Density {}

#[component]
#[theme(primary, density)]
fn themed_control(#[default] primary: bool, #[default] density: Density) -> impl Component {
    let _ = (primary, density);
    stack()
}

#[test]
fn component_builders_opt_into_declaration_themes() {
    let appearance = theme! {
        ThemedControl { padding: 8px; }
        ThemedControl[$primary] { background: blue; }
        ThemedControl.compact[$density == Density::Compact]:hover { opacity: 80%; }
    };
    assert_eq!(appearance.rules().len(), 3);

    let view = themed_control()
        .primary(true)
        .density(Density::Compact)
        .classes(HashSet::from([class!("compact")]))
        .enable_hovered();
    drop(view.prepare(RootBuild::new()));
}

#[test]
fn catalog_nodes_expose_theme_identities_and_semantic_options() {
    fn unit<T: ThemeTarget<Options = ()>>() {}
    unit::<Stack>();
    unit::<Center>();
    unit::<Container>();
    unit::<Wrap>();
    unit::<Circle>();
    unit::<Blockquote>();
    unit::<Code>();
    unit::<CodeBlock>();
    unit::<Em>();
    unit::<Link>();
    unit::<LinkOverlay>();
    unit::<Markdown>();
    unit::<DocumentView>();
    unit::<ScrollArea>();
    unit::<Pages<Vec<StackBuilder>>>();
    fn options<T: ThemeTarget<Options = ThemedControlOptions>>() {}
    options::<ThemedControl>();
    fn heading_options<T: ThemeTarget<Options = HeadingOptions>>() {}
    heading_options::<Heading>();

    let appearance = theme! {
        Heading[$level == HeadingLevel::H1] { font-size: 60px; }
        List[$marker == ListMarker::None] { gap: 0px; }
        Image[$object_fit == ObjectFit::Contain] { opacity: 90%; }
        AspectRatio[$ratio > 1.] { padding: 2px; }
        Float[$placement == FloatPlacement::TopEnd] { opacity: 80%; }
        Separator[$orientation == Orientation::Vertical && $thickness == Size::Px(2.)] {
            width: 3px;
        }
    };
    assert_eq!(appearance.rules().len(), 6);
}

#[test]
fn builtin_selectors_resolve_classes_hover_and_source_order() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let appearance = theme! {
            * { padding: 1px; }
            Stack { padding: 8px; }
            Stack.compact.!disabled:hover { padding: 3px; opacity: 50%; }
            Stack.compact { padding: 6px; }
            CodeBlock, LinkOverlay { padding: 12px; }
        };
        let classes = HashSet::from([class!("compact")]);
        let hovered = signal(false);
        let resolve = || {
            appearance.resolve(&[ComponentRef::new(
                Stack::key(),
                &(),
                &classes,
                Some(*hovered),
            )])
        };
        assert_eq!(resolve().take_padding().get().left, Size::Px(6.));
        hovered.set(true);
        assert_eq!(resolve().take_padding().get().left, Size::Px(3.));
        assert_eq!(resolve().take_opacity().get(), 0.5);

        let disabled = HashSet::from([class!("compact"), class!("disabled")]);
        let mut style = appearance.resolve(&[ComponentRef::new(
            Stack::key(),
            &(),
            &disabled,
            Some(*hovered),
        )]);
        assert_eq!(style.take_padding().get().left, Size::Px(6.));
        let mut style = appearance.resolve(&[ComponentRef::new(
            CodeBlock::key(),
            &(),
            &HashSet::new(),
            None,
        )]);
        assert_eq!(style.take_padding().get().left, Size::Px(12.));
    });
}

#[test]
fn prepare_defaults_remain_reactive_and_explicit_styles_win() {
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

#[test]
fn pages_defer_children_and_retain_them_across_selection_changes() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let mounts = Rc::new(Cell::new(0));
        let selected = signal(0usize);
        let page = {
            let mounts = mounts.clone();
            NodePlan::new(Style::new(), move |_| mounts.set(mounts.get() + 1))
        };
        let view = pages(selected, [page]).page_transition(PagesTransition::None);
        let prepared = view.prepare(RootBuild::new());
        assert_eq!(mounts.get(), 0);
        let node = prepared.mount();
        assert_eq!(mounts.get(), 1);
        selected.set(1);
        selected.set(0);
        assert_eq!(mounts.get(), 1);
        drop(node);
        assert!(!selected.is_disposed());
    });
}
