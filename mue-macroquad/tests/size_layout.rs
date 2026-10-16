use mue_core::{
    effect::computed,
    signal::{Access, signal},
};
use mue_macroquad::style::{Size, SizeExt};
use taffy::{
    AlignItems, AvailableSpace, FlexDirection, NodeId, Size as TaffySize, Style, TaffyTree,
};

fn parent_style(width: f32, height: f32) -> Style {
    Style {
        size: TaffySize {
            width: Size::px(width).to_dimension(),
            height: Size::px(height).to_dimension(),
        },
        flex_direction: FlexDirection::Column,
        align_items: Some(AlignItems::Start),
        ..Style::default()
    }
}

fn layout(tree: &mut TaffyTree<()>, root: NodeId) {
    tree.compute_layout(
        root,
        TaffySize {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();
}

#[test]
fn padding_top_percent_uses_containing_block_width_not_height_or_own_width() {
    let padding = signal(Size::percent(10.0));
    let lowered = computed(move |_| Style {
        size: TaffySize {
            width: Size::px(100.0).to_dimension(),
            height: Size::auto().to_dimension(),
        },
        padding: taffy::Rect {
            top: padding.get().to_length_percentage("padding_top"),
            left: Size::px(0.0).to_length_percentage("padding_left"),
            right: Size::px(0.0).to_length_percentage("padding_right"),
            bottom: Size::px(0.0).to_length_percentage("padding_bottom"),
        },
        flex_shrink: 0.0,
        ..Style::default()
    });
    let mut tree = TaffyTree::<()>::new();
    let content = tree.new_leaf(parent_style(10.0, 10.0)).unwrap();
    let child = tree
        .new_with_children(lowered.get_clone(), &[content])
        .unwrap();
    let root = tree
        .new_with_children(parent_style(400.0, 300.0), &[child])
        .unwrap();
    layout(&mut tree, root);
    assert_eq!(tree.layout(child).unwrap().padding.top, 40.0);
    assert_eq!(tree.layout(content).unwrap().location.y, 40.0);

    tree.set_style(root, parent_style(400.0, 700.0)).unwrap();
    layout(&mut tree, root);
    assert_eq!(tree.layout(child).unwrap().padding.top, 40.0);
    tree.set_style(root, parent_style(600.0, 700.0)).unwrap();
    layout(&mut tree, root);
    assert_eq!(tree.layout(child).unwrap().padding.top, 60.0);
    assert_eq!(tree.layout(content).unwrap().location.y, 60.0);
    assert_eq!(padding.get(), Size::percent(10.0));

    padding.set(Size::px(8.0));
    tree.set_style(child, lowered.get_clone()).unwrap();
    layout(&mut tree, root);
    assert_eq!(tree.layout(child).unwrap().padding.top, 8.0);
    assert_eq!(tree.layout(content).unwrap().location.y, 8.0);
}
