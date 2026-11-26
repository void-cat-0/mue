use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;
use taffy::{AlignItems, FlexDirection, FlexWrap, JustifyContent};

const PAGE: &str = "Layout";

fn tile(n: usize) -> Node {
    center()
        .style(mue_macroquad::style! {
            flex: none;
            width: {u(64.)};
            height: {u(44.)};
            border-radius: {u(8.)};
            background: {if n.is_multiple_of(2) { BLUE } else { CORAL }};
        })
        .children(text(format!("{}", n + 1)).style(mue_macroquad::style! {
            font-size: {f(16.)}; line-height: {u(22.)};
            color: #131720;
        }))
        .into_node()
}

fn flex_card() -> Node {
    let (direction, direction_control) = segmented(
        "Direction",
        vec![
            ("Row", FlexDirection::Row),
            ("Column", FlexDirection::Column),
            ("Row rev", FlexDirection::RowReverse),
            ("Column rev", FlexDirection::ColumnReverse),
        ],
    );
    let (wrapping, wrap_control) = segmented(
        "Wrap",
        vec![
            ("Wrap", FlexWrap::Wrap),
            ("No wrap", FlexWrap::NoWrap),
            ("Reverse", FlexWrap::WrapReverse),
        ],
    );
    let (justify, justify_control) = segmented(
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
    let (align, align_control) = segmented(
        "Align items",
        vec![
            ("Start", Some(AlignItems::FlexStart)),
            ("Center", Some(AlignItems::Center)),
            ("End", Some(AlignItems::FlexEnd)),
            ("Stretch", Some(AlignItems::Stretch)),
        ],
    );
    let (count, count_control) = segmented("Children", vec![("3", 3usize), ("6", 6), ("12", 12)]);
    let (gap, gap_control) = segmented("Gap", vec![("8", 8.), ("0", 0.), ("24", 24.)]);
    let children = map_keyed(
        count.map(|n| (0..n).collect::<Rc<[usize]>>()),
        |n| *n,
        |n| tile(*n),
    );
    card(
        "Flexbox",
        "Every option is applied live and the tiles animate to their new places.",
        (
            direction_control,
            wrap_control,
            justify_control,
            align_control,
            count_control,
            gap_control,
            flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    height: {u(220.)};
                    padding: {(u(12.),)};
                    border-radius: {u(10.)};
                    background: {raised()};
                    flex-direction: {direction};
                    flex-wrap: {wrapping};
                    align-items: {align};
                    align-content: flex-start;
                    justify-content: {justify};
                    gap: {gap.map(mue_macroquad::style::Size::Px)};
                    transition: {SPRING};
                })
                .children(children),
        ),
    )
}

fn grid_card() -> Node {
    let (columns, columns_control) = segmented(
        "Columns",
        vec![("Auto fill 120px", 0usize), ("2", 2), ("4", 4)],
    );
    let cell = |label: &'static str, accent: bool| {
        use mue_macroquad::shader::Brush;
        let fill: Prop<Brush> = if accent {
            Brush::Solid(BLUE).into()
        } else {
            raised()
        };
        let ink: Prop<Brush> = if accent {
            Brush::Solid(Color::from_rgba(19, 23, 32, 255)).into()
        } else {
            foreground()
        };
        center()
            .style(mue_macroquad::style! {
                min-height: {u(44.)};
                padding: {(u(6.),)};
                border-radius: {u(8.)};
                background: {fill};
            })
            .children(text(label).style(mue_macroquad::style! {
                font-size: {f(13.)}; line-height: {u(18.)};
                color: {ink};
                text-align: center;
            }))
    };
    let grid_view = keyed(columns, move |columns| {
        let template = match *columns {
            2 => mue_macroquad::style! { grid-template-columns: repeat(2, 1fr); },
            4 => mue_macroquad::style! { grid-template-columns: repeat(4, 1fr); },
            _ => {
                mue_macroquad::style! { grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); }
            }
        };
        grid()
            .style(mue_macroquad::style! { width: 100%; gap: 8px; grid-auto-rows: minmax(44px, auto); })
            .style(template)
            .children((
                cell("grid-column: 1 / -1", true).style(mue_macroquad::style! { grid-column: 1 / -1; }),
                cell("span 2", false).style(mue_macroquad::style! { grid-column: span 2; }),
                cell("auto", false),
                cell("auto", false),
                cell("auto", false),
                cell("span 2 rows", false).style(mue_macroquad::style! { grid-row: span 2; }),
                cell("auto", false),
                cell("auto", false),
            ))
            .into_node()
    });
    card(
        "Grid",
        "CSS grid tracks with repeat, minmax, fr, and spans.",
        (columns_control, grid_view),
    )
}

fn ratio_card() -> Node {
    let (ratio, ratio_control) = segmented(
        "Ratio",
        vec![
            ("16:9", 16. / 9.),
            ("4:3", 4. / 3.),
            ("1:1", 1.),
            ("3:1", 3.),
        ],
    );
    let (placement, placement_control) = segmented(
        "Float",
        vec![
            ("Top start", FloatPlacement::TopStart),
            ("Top end", FloatPlacement::TopEnd),
            ("Center", FloatPlacement::MiddleCenter),
            ("Bottom start", FloatPlacement::BottomStart),
            ("Bottom end", FloatPlacement::BottomEnd),
        ],
    );
    card(
        "Aspect ratio and float",
        "The box keeps its ratio at any width; the badge floats on one of its corners, centered on the edge.",
        (
            ratio_control,
            placement_control,
            flexbox()
                .style(mue_macroquad::style! { width: 100%; padding: {(u(16.),)}; })
                .children(
                    aspect_ratio()
                        .ratio(ratio)
                        .style(mue_macroquad::style! {
                            width: 70%;
                            border-radius: {u(10.)};
                            background: {raised()};
                        })
                        .children((
                            center()
                                .style(mue_macroquad::style! { width: 100%; height: 100%; })
                                .children(
                                    text(ratio.map(|r| Rc::<str>::from(format!("ratio {r:.2}"))))
                                        .style(mue_macroquad::style! {
                                            font-size: {f(16.)}; line-height: {u(22.)};
                                            color: {muted()};
                                        }),
                                ),
                            float().placement(placement).children(
                                center()
                                    .style(mue_macroquad::style! {
                                        width: {u(28.)}; height: {u(28.)};
                                        border-radius: 9999px;
                                        background: {CORAL};
                                    })
                                    .children(text("3").style(mue_macroquad::style! {
                                        font-size: {f(12.)}; line-height: {u(16.)};
                                        color: #131720;
                                    })),
                            ),
                        )),
                ),
        ),
    )
}

fn touch_target_card() -> Node {
    let (slop, slop_control) = segmented(
        "Hit area",
        vec![
            ("Visual only", 0.),
            ("min-hit-size 44", 44.),
            ("min-hit-size 64", 64.),
        ],
    );
    let taps = signal(0u32);
    let target = keyed(slop, move |slop: &f32| {
        let slop = *slop;
        center()
            .style(mue_macroquad::style! {
                width: {u(120.)}; height: {u(120.)};
            })
            .children((
                // The outline shows the hit area; the dot is what is drawn.
                flexbox().style(mue_macroquad::style! {
                    position: absolute;
                    width: {mue_macroquad::style::Size::Px(slop.max(16.))};
                    height: {mue_macroquad::style::Size::Px(slop.max(16.))};
                    border-radius: 9999px;
                    border-width: 1px;
                    border-color: {CORAL};
                }),
                link_overlay()
                    .style(mue_macroquad::style! {
                        width: 16px; height: 16px;
                        border-radius: 8px;
                        background: {BLUE};
                    })
                    .min_hit_size(slop)
                    .on_activate(move || taps.update(|n| *n += 1)),
            ))
            .into_node()
    });
    card(
        "Touch targets",
        "A 16 px dot with a larger invisible hit area (the outline). Tap just outside the dot.",
        (
            slop_control,
            row((
                target,
                para(taps.map(|n| Rc::<str>::from(format!("{n} taps")))),
            )),
        ),
    )
}

fn stacks_card() -> Node {
    let label = |name: &'static str| {
        code(name).style(mue_macroquad::style! {
            font-size: {f(13.)}; line-height: {u(20.)};
            padding: {(u(6.), u(2.))};
            border-radius: {u(4.)};
        })
    };
    card(
        "Stacks, wrap, and blocks",
        "vstack lays out vertically, hstack horizontally, wrap flows onto new lines, and div is block layout.",
        row((
            surface()
                .style(mue_macroquad::style! { width: auto; flex: 1; min-width: {u(140.)}; padding: {(u(8.),)}; })
                .children(vstack().children((label("vstack"), label("a"), label("b")))),
            surface()
                .style(mue_macroquad::style! { width: auto; flex: 1; min-width: {u(140.)}; padding: {(u(8.),)}; })
                .children(hstack().children((label("hstack"), label("a"), label("b")))),
            surface()
                .style(mue_macroquad::style! { width: auto; flex: 1; min-width: {u(140.)}; padding: {(u(8.),)}; })
                .children(wrap().children((label("wrap"), label("one"), label("two"), label("three"), label("four")))),
            surface()
                .style(mue_macroquad::style! { width: auto; flex: 1; min-width: {u(140.)}; padding: {(u(8.),)}; })
                .children(div().children((para("div block A"), para("div block B")))),
        )),
    )
}

pub fn page() -> Node {
    page_body(
        "Flexbox, grid, aspect ratio, and touch targets. Resize the window to test wrapping.",
        (
            flex_card(),
            grid_card(),
            ratio_card(),
            touch_target_card(),
            stacks_card(),
            checklist(
                PAGE,
                &[
                    "Flexbox options move the tiles as labeled",
                    "Grid spans and auto-fill columns are correct",
                    "The aspect box keeps its ratio when the window is resized",
                    "Touch-target taps register inside the outline but not beyond it",
                ],
            ),
        ),
    )
}
