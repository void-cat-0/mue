use super::{controls::*, responsive};
use mue_macroquad::{
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
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
                .width(percent(18.))
                .height(vh(6.25))
                .min_height(responsive::ui(44.))
                .min_width(0.)
                .background(if n % 2 == 0 { BLUE } else { CORAL })
                .radius(8.)
                .children(
                    text(format!("{n}"))
                        .font_size(responsive::font(20.))
                        .line_height(responsive::ui(28.)),
                )
                .into_node()
        },
    );
    stack()
        .w_full()
        .min_width(0.)
        .gap(vh(3.))
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j, k]),
            flexbox()
                .height(vh(31.25))
                .min_height(responsive::ui(180.))
                .w_full()
                .background(PANEL)
                .padding((vw(2.), vh(2.)))
                .flex_direction(direction)
                .flex_wrap(wrapping)
                .align_items(align)
                .justify_content(justify)
                .gap(gap.select(vw(3.), vw(1.)))
                .animation(SPRING)
                .children(children),
            aspect_ratio()
                .ratio(ratio.select(4., 2.))
                .w_full()
                .min_height(responsive::ui(64.))
                .background(PANEL)
                .children((
                    center().size_full().children(
                        text("Aspect ratio")
                            .font_size(responsive::font(22.))
                            .line_height(responsive::ui(32.)),
                    ),
                    float()
                        .placement(placement)
                        .offset_x(offset.select(-20., 0.))
                        .offset_y(offset.select(20., 0.))
                        .children(circle().width(36.).height(36.).background(CORAL)),
                )),
            center()
                .w_full()
                .height(vh(6.25))
                .min_height(responsive::ui(44.))
                .children(
                    separator()
                        .orientation(orientation)
                        .thickness(thick.select(6., 1.))
                        .background(BLUE),
                ),
            section(
                "Stack / wrap / div",
                wrap().w_full().items_start().gap(vw(2.)).children((
                    vstack()
                        .flex_1()
                        .flex_basis(percent(30.))
                        .min_width(0.)
                        .children(["stack", "vstack"].map(|label| {
                            code(label)
                                .font_size(responsive::font(32.))
                                .line_height(responsive::ui(44.))
                                .padding((responsive::ui(6.), responsive::ui(2.)))
                                .radius(responsive::ui(4.))
                                .into_node()
                        })),
                    wrap()
                        .flex_1()
                        .flex_basis(percent(30.))
                        .min_width(0.)
                        .children(["wrap", "hstack"].map(|label| {
                            code(label)
                                .font_size(responsive::font(32.))
                                .line_height(responsive::ui(44.))
                                .padding((responsive::ui(6.), responsive::ui(2.)))
                                .radius(responsive::ui(4.))
                                .into_node()
                        })),
                    div()
                        .flex_1()
                        .flex_basis(percent(30.))
                        .min_width(0.)
                        .children((
                            text("block A")
                                .font_size(responsive::font(20.))
                                .line_height(responsive::ui(28.)),
                            text("block B")
                                .font_size(responsive::font(20.))
                                .line_height(responsive::ui(28.)),
                        )),
                )),
            ),
        ))
        .into_node()
}
