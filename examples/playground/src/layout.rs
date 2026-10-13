use super::controls::*;
use mue_macroquad::{
    node::*,
    style::{Styleable, StyleableExt},
};
use taffy::{AlignItems, FlexDirection, FlexWrap, JustifyContent};

pub fn page() -> Node {
    let (direction, a) = choice(
        "Direction",
        vec![
            ("Row", FlexDirection::Row),
            ("Column", FlexDirection::Column),
            ("Row reverse", FlexDirection::RowReverse),
            ("Column reverse", FlexDirection::ColumnReverse),
        ],
    );
    let (wrapping, b) = choice(
        "Wrap",
        vec![
            ("Wrap", FlexWrap::Wrap),
            ("No wrap", FlexWrap::NoWrap),
            ("Reverse", FlexWrap::WrapReverse),
        ],
    );
    let (align, c) = choice(
        "Align",
        vec![
            ("Center", Some(AlignItems::Center)),
            ("Start", Some(AlignItems::FlexStart)),
            ("End", Some(AlignItems::FlexEnd)),
            ("Stretch", Some(AlignItems::Stretch)),
            ("Baseline", Some(AlignItems::Baseline)),
        ],
    );
    let (justify, d) = choice(
        "Justify",
        vec![
            ("Start", Some(JustifyContent::FlexStart)),
            ("Center", Some(JustifyContent::Center)),
            ("End", Some(JustifyContent::FlexEnd)),
            ("Between", Some(JustifyContent::SpaceBetween)),
            ("Around", Some(JustifyContent::SpaceAround)),
            ("Evenly", Some(JustifyContent::SpaceEvenly)),
        ],
    );
    let (gap, e) = toggle("Gap");
    let (ratio, f) = toggle("Ratio");
    let (thick, g) = toggle("Thickness");
    let (placement, h) = choice(
        "Float",
        vec![
            ("Top end", FloatPlacement::TopEnd),
            ("Top start", FloatPlacement::TopStart),
            ("Top center", FloatPlacement::TopCenter),
            ("Middle start", FloatPlacement::MiddleStart),
            ("Middle center", FloatPlacement::MiddleCenter),
            ("Middle end", FloatPlacement::MiddleEnd),
            ("Bottom start", FloatPlacement::BottomStart),
            ("Bottom center", FloatPlacement::BottomCenter),
            ("Bottom end", FloatPlacement::BottomEnd),
        ],
    );
    let (orientation, i) = choice(
        "Separator",
        vec![
            ("Horizontal", Orientation::Horizontal),
            ("Vertical", Orientation::Vertical),
        ],
    );
    let (offset, j) = toggle("Float offset");
    let (count, k) = choice("Children", vec![("6", 6usize), ("2", 2), ("12", 12)]);
    let children = map_keyed(
        count.map(|n| (0..n).collect::<std::rc::Rc<[usize]>>()),
        |n| *n,
        |n| {
            center()
                .width(90.)
                .height(50.)
                .background(if n % 2 == 0 { BLUE } else { CORAL })
                .radius(8.)
                .children(text(format!("{n}")).font_size(20.))
                .into_node()
        },
    );
    stack()
        .gap(24.)
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j, k]),
            flexbox()
                .height(250.)
                .w_full()
                .background(PANEL)
                .padding((16.,))
                .flex_direction(direction)
                .flex_wrap(wrapping)
                .align_items(align)
                .justify_content(justify)
                .gap(gap.select(24., 8.))
                .animation(SPRING)
                .children(children),
            aspect_ratio()
                .ratio(ratio.select(4., 2.))
                .w_full()
                .background(PANEL)
                .children((
                    center()
                        .size_full()
                        .children(text("Aspect ratio").font_size(22.)),
                    float()
                        .placement(placement)
                        .offset_x(offset.select(-20., 0.))
                        .offset_y(offset.select(20., 0.))
                        .children(circle().width(36.).height(36.).background(CORAL)),
                )),
            center().height(50.).children(
                separator()
                    .orientation(orientation)
                    .thickness(thick.select(6., 1.))
                    .background(BLUE),
            ),
            section(
                "Stack / wrap / div",
                hstack().items_start().children((
                    vstack().flex_1().children((code("stack"), code("vstack"))),
                    wrap().flex_1().children((code("wrap"), code("hstack"))),
                    div().flex_1().children((
                        text("block A").font_size(20.),
                        text("block B").font_size(20.),
                    )),
                )),
            ),
        ))
        .into_node()
}
