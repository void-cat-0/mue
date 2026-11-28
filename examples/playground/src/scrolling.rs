use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    app::runtime::set_timeout,
    math::Vector,
    node::NodeHandle,
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;

const PAGE: &str = "Scrolling";

fn stripe(n: usize, wide: bool) -> Node {
    center()
        .style(mue_macroquad::style! {
            flex: none;
            height: {u(48.)};
            border-radius: {u(8.)};
            background: {if n.is_multiple_of(2) { BLUE } else { CORAL }};
        })
        .style(if wide {
            mue_macroquad::style! { width: {u(900.)}; }
        } else {
            mue_macroquad::style! { width: 100%; }
        })
        .children(text(format!("Row {}", n + 1)).style(mue_macroquad::style! {
            font-size: {f(15.)}; line-height: {u(22.)};
            color: #131720;
        }))
        .into_node()
}

fn options_card() -> Node {
    let (axis, axis_control) = segmented(
        "Axis",
        vec![
            ("Vertical", ScrollAxis::Vertical),
            ("Horizontal", ScrollAxis::Horizontal),
            ("Both", ScrollAxis::Both),
        ],
    );
    let (friction, friction_control) =
        segmented("Friction", vec![("Normal", 8.), ("Low", 3.), ("High", 16.)]);
    let (touch, touch_control) = option_switch("Touch drag", true);
    let (mouse, mouse_control) = option_switch("Mouse drag", false);
    let (wheel, wheel_control) = option_switch("Wheel", true);
    let (momentum, momentum_control) = option_switch("Momentum", true);
    let (bars, bars_control) = option_switch("Scrollbars", true);
    let (chain, chain_control) = option_switch("Chain to page", true);
    let offset = signal(Vector::ZERO);
    let key = computed(move |_| {
        (
            axis.get(),
            friction.get(),
            [
                touch.get(),
                mouse.get(),
                wheel.get(),
                momentum.get(),
                bars.get(),
                chain.get(),
            ],
        )
    });
    // Options configure input recognizers when the area is built.
    let area = keyed(key, move |(axis, friction, flags)| {
        let [touch, mouse_drag, wheel, momentum, scrollbars, chain] = *flags;
        let wide = *axis != ScrollAxis::Vertical;
        scroll_area()
            .offset(offset)
            .options(ScrollOptions {
                axis: *axis,
                friction: *friction,
                touch,
                mouse_drag,
                wheel,
                momentum,
                scrollbars,
                chain,
                ..Default::default()
            })
            .style(mue_macroquad::style! {
                width: 100%;
                height: {u(240.)};
                border-radius: {u(10.)};
                background: {raised()};
            })
            .content_style(mue_macroquad::style! { gap: {u(8.)}; padding: {(u(12.),)}; })
            .children((0..20).map(|n| stripe(n, wide)).collect::<Vec<_>>())
            .into_node()
    });
    card(
        "Scroll options",
        (
            axis_control,
            friction_control,
            row((touch_control, mouse_control, wheel_control)),
            row((momentum_control, bars_control, chain_control)),
            area,
            row((
                button("Scroll to start").on_activate(move || offset.set(Vector::ZERO)),
                mono(offset.map(|v| Rc::<str>::from(format!("offset {:.0}, {:.0}", v.x, v.y)))),
            )),
        ),
    )
}

fn handle_card() -> Node {
    let list = ScrollAreaHandle::new();
    let target = NodeHandle::new();
    let (animated, animated_control) = option_switch("Animated", true);
    let l = list.clone();
    let top = move || l.scroll_to_top(animated.get_untracked());
    let l = list.clone();
    let end = move || l.scroll_to_end(animated.get_untracked());
    let l = list.clone();
    let down = move || l.scroll_by(Vector::new(0., 200.), animated.get_untracked());
    let l = list.clone();
    let reveal = move || {
        if let Some(rect) = target.rect() {
            l.scroll_into_view(rect, animated.get_untracked());
        }
    };
    let rows = (0..40)
        .map(|n| {
            let row = stripe(n, false);
            if n == 29 {
                center()
                    .node_handle(target)
                    .style(mue_macroquad::style! {
                        width: 100%; flex: none;
                        height: {u(48.)};
                        border-radius: {u(8.)};
                        background: #7fd1a8;
                    })
                    .children(text("Row 30, the target").style(mue_macroquad::style! {
                        font-size: {f(15.)}; line-height: {u(22.)};
                        color: #131720;
                    }))
                    .into_node()
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    card(
        "Scrolling from code",
        (
            animated_control,
            row((
                button("Scroll to top").on_activate(top),
                button("Scroll down 200").on_activate(down),
                button("Reveal row 30").on_activate(reveal),
                button("Scroll to end").on_activate(end),
            )),
            scroll_area()
                .handle(list)
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(220.)};
                    border-radius: {u(10.)};
                    background: {raised()};
                })
                .content_style(mue_macroquad::style! { gap: {u(8.)}; padding: {(u(12.),)}; })
                .children(rows),
        ),
    )
}

#[derive(Clone, PartialEq)]
struct Mail {
    id: u32,
}

fn feed_card() -> Node {
    let next = signal(12u32);
    let mails: Signal<Rc<[Mail]>> = signal((0..12).map(|id| Mail { id }).collect());
    let refreshing = signal(false);
    let loading = signal(false);
    let status = signal(String::from(
        "Pull down to refresh, scroll to the end to load more",
    ));
    let row_view = move |mail: &Mail| {
        let id = mail.id;
        let remove = move || {
            let list: Rc<[Mail]> = mails
                .get_clone_untracked()
                .iter()
                .filter(|m| m.id != id)
                .cloned()
                .collect();
            mails.set(list);
            log_event(format!("message {id} deleted"));
        };
        swipe_actions(
            stack()
                .style(mue_macroquad::style! {
                    width: 100%;
                    padding: {(u(14.), u(10.))};
                    gap: {u(2.)};
                    background: {panel_brush()};
                    border-bottom-width: 1px;
                    border-color: {border()};
                })
                .children((
                    text(format!("Message {id}")).style(mue_macroquad::style! {
                        font-size: {f(15.)}; line-height: {u(21.)};
                        color: {foreground()};
                    }),
                    hint("Swipe left to delete, right to archive"),
                )),
        )
        .leading(
            button("Archive")
                .style(mue_macroquad::style! { border-radius: 0px; min-width: {u(88.)}; background: #2d6cdf; })
                .on_activate(move || log_event(format!("message {id} archived"))),
        )
        .trailing(
            button("Delete")
                .style(mue_macroquad::style! { border-radius: 0px; min-width: {u(88.)}; background: #c0392b; })
                .on_activate(remove),
        )
        .on_full_swipe_trailing(remove)
        .mouse(true)
    };
    card(
        "Pull to refresh, load more, swipe actions",
        (
            mono(status.map(|text| Rc::<str>::from(text.as_str()))),
            scroll_area()
                .options(ScrollOptions {
                    mouse_drag: true,
                    ..Default::default()
                })
                .on_refresh(refreshing, move || {
                    status.set("Refreshing…".into());
                    set_timeout(1.0, move || {
                        let first = next.get_untracked();
                        next.set(first + 2);
                        let mut list: Vec<Mail> =
                            (first..first + 2).map(|id| Mail { id }).collect();
                        list.extend(mails.get_clone_untracked().iter().cloned());
                        mails.set(list.into());
                        refreshing.set(false);
                        status.set("Two new messages".into());
                    });
                })
                .on_reach_end(120., move || {
                    if loading.get_untracked() {
                        return;
                    }
                    loading.set(true);
                    status.set("Loading more…".into());
                    set_timeout(0.6, move || {
                        let first = next.get_untracked();
                        next.set(first + 6);
                        let mut list = mails.get_clone_untracked().to_vec();
                        list.extend((first..first + 6).map(|id| Mail { id }));
                        mails.set(list.into());
                        loading.set(false);
                        status.set(format!("{} messages", mails.get_clone_untracked().len()));
                    });
                })
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(320.)};
                    border-radius: {u(10.)};
                    background: {raised()};
                })
                .children(map_keyed(mails, |mail| mail.id, row_view)),
        ),
    )
}

fn nested_card() -> Node {
    let card_view = |n: usize| {
        center()
            .style(mue_macroquad::style! {
                flex: none;
                width: {u(150.)};
                height: {u(100.)};
                border-radius: {u(10.)};
                background: {if n.is_multiple_of(2) { BLUE } else { CORAL }};
            })
            .children(
                text(format!("Card {}", n + 1)).style(mue_macroquad::style! {
                    font-size: {f(15.)}; line-height: {u(22.)};
                    color: #131720;
                }),
            )
            .into_node()
    };
    card(
        "Nested scrolling",
        scroll_area()
            .axis(ScrollAxis::Horizontal)
            .options(ScrollOptions {
                axis: ScrollAxis::Horizontal,
                mouse_drag: true,
                scrollbars: false,
                ..Default::default()
            })
            .style(mue_macroquad::style! { width: 100%; height: {u(116.)}; })
            .content_style(mue_macroquad::style! { gap: {u(10.)}; padding: {(u(0.), u(8.))}; })
            .children((0..12).map(card_view).collect::<Vec<_>>()),
    )
}

pub fn page() -> Node {
    page_body(
        (
            options_card(),
            handle_card(),
            feed_card(),
            nested_card(),
            card("Events", event_log()),
            checklist(
                PAGE,
                &[
                    "Touch and wheel scrolling glide with momentum",
                    "At the end of the box, the page takes over the scroll",
                    "Each scroll button lands in the right place",
                    "Pull to refresh adds two rows",
                    "The end of the list loads more rows once",
                    "Swipes archive and delete, one open row at a time",
                    "The carousel scrolls sideways without moving the page",
                ],
            ),
        ),
    )
}
