use super::{controls::*, responsive};
use mue_components::*;
use mue_macroquad::style::{Styleable, vh, vw};
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
                .style(mue_macroquad::style! {
                width: 18%;
                height: {vh(6.25)};
                min-height: {responsive::ui(44.)};
                min-width: 0px;
                })
                .style(
                    mue_macroquad::style! { background: {if n % 2 == 0 { BLUE } else { CORAL }}; },
                )
                .style(mue_macroquad::style! { border-radius: {8.}; })
                .children(text(format!("{n}")).style(mue_macroquad::style! {
                font-size: {responsive::font(20.)};
                line-height: {responsive::ui(28.)};
                }))
                .into_node()
        },
    );
    stack()
        .style(mue_macroquad::style! {
width: 100%;
min-width: 0px;
gap: {vh(3.)};
})
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j, k]),
            flexbox()
                .style(mue_macroquad::style! {
height: {vh(31.25)};
min-height: {responsive::ui(180.)};
width: 100%;
background: {PANEL};
padding: {(vw(2.), vh(2.))};
flex-direction: {direction};
                    flex-wrap: {wrapping};
align-items: {align};
justify-content: {justify};
gap: {gap.select(vw(3.), vw(1.))};
transition: {SPRING};
})
                .children(children),
            aspect_ratio()
                .ratio(ratio.select(4., 2.))
                .style(mue_macroquad::style! {
width: 100%;
min-height: {responsive::ui(64.)};
background: {PANEL};
})
                .children((
                    center().style(mue_macroquad::style! { width: 100%; height: 100%; }).children(
                        text("Aspect ratio")
                            .style(mue_macroquad::style! {
font-size: {responsive::font(22.)};
line-height: {responsive::ui(32.)};
}),
                    ),
                    float()
                        .placement(placement)
                        .offset_x(offset.select(-20., 0.))
                        .offset_y(offset.select(20., 0.))
                        .children(circle().style(mue_macroquad::style! {
width: {36.};
height: {36.};
background: {CORAL};
})),
                )),
            center()
                .style(mue_macroquad::style! {
width: 100%;
height: {vh(6.25)};
min-height: {responsive::ui(44.)};
})
                .children(
                    separator()
                        .orientation(orientation)
                        .thickness(thick.select(6., 1.))
                        .style(mue_macroquad::style! { background: {BLUE}; }),
                ),
            section(
                "Stack / wrap / div",
                wrap()
                    .style(mue_macroquad::style! { width: 100%; align-items: flex-start; gap: {vw(2.)}; })
                    .children((
                    vstack()
                        .style(mue_macroquad::style! {
flex: 1;
flex-basis: 30%;
min-width: 0px;
})
                        .children(["stack", "vstack"].map(|label| {
                            code(label)
                                .style(mue_macroquad::style! {
font-size: {responsive::font(32.)};
line-height: {responsive::ui(44.)};
padding: {(responsive::ui(6.), responsive::ui(2.))};
border-radius: {responsive::ui(4.)};
})
                                .into_node()
                        })),
                    wrap()
                        .style(mue_macroquad::style! {
flex: 1;
flex-basis: 30%;
min-width: 0px;
})
                        .children(["wrap", "hstack"].map(|label| {
                            code(label)
                                .style(mue_macroquad::style! {
font-size: {responsive::font(32.)};
line-height: {responsive::ui(44.)};
padding: {(responsive::ui(6.), responsive::ui(2.))};
border-radius: {responsive::ui(4.)};
})
                                .into_node()
                        })),
                    div()
                        .style(mue_macroquad::style! {
flex: 1;
flex-basis: 30%;
min-width: 0px;
})
                        .children((
                            text("block A")
                                .style(mue_macroquad::style! {
font-size: {responsive::font(20.)};
line-height: {responsive::ui(28.)};
}),
                            text("block B")
                                .style(mue_macroquad::style! {
font-size: {responsive::font(20.)};
line-height: {responsive::ui(28.)};
}),
                        )),
                )),
            ),
        ))
        .into_node()
}
