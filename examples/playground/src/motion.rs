use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    motion::presence::{PresenceAnimation, presence},
    motion::transition::Transition,
    motion::transition_group::transition_group,
    paint::shader::Brush,
    style::{Size, Styleable, Transform, TransformOp},
};
use std::rc::Rc;

const PAGE: &str = "Motion";

fn transitions_card() -> Node {
    let (curve, curve_control) = segmented(
        "Curve",
        vec![
            ("Spring", Transition::spring(0.65, 0.3)),
            ("Bouncy", Transition::spring(0.8, 0.65)),
            ("Ease", Transition::ease_in_out(0.6)),
            ("Linear", Transition::linear(0.6)),
            ("Instant", Transition::Instant),
        ],
    );
    let (big, big_control) = switch("Size", false);
    let (round, round_control) = switch("Radius", false);
    let (warm, warm_control) = switch("Color", false);
    let (faded, faded_control) = switch("Opacity", false);
    let (moved, moved_control) = switch("Translate", false);
    let (turned, turned_control) = switch("Rotate", false);
    let (tilted, tilted_control) = switch("3D tilt", false);
    let transform = computed(move |_| {
        Transform(vec![
            TransformOp::Perspective(700.),
            TransformOp::Translate(Size::Px(if moved.get() { 60. } else { 0. }), Size::Px(0.)),
            TransformOp::RotateX(if tilted.get() { 0.5 } else { 0. }),
            TransformOp::RotateY(if tilted.get() { -0.4 } else { 0. }),
            TransformOp::Rotate(if turned.get() { 0.4 } else { 0. }),
        ])
    });
    card(
        "Property transitions",
        "Flip any switch; the box animates from its current value, even mid-animation.",
        (
            curve_control,
            row((big_control, round_control, warm_control, faded_control)),
            row((moved_control, turned_control, tilted_control)),
            center()
                .style(mue_macroquad::style! { width: 100%; height: {u(240.)}; })
                .children(
                    center()
                        .style(mue_macroquad::style! {
                            width: {super::responsive::ui(big.select(220., 140.))};
                            height: {super::responsive::ui(big.select(150., 96.))};
                            border-radius: {round.select(60., 10.)};
                            background: {warm.select(Brush::Solid(CORAL), Brush::Solid(BLUE))};
                            opacity: {faded.select(0.35, 1.)};
                            transform: {transform};
                            transition: {curve};
                        })
                        .children(text("mue").style(mue_macroquad::style! {
                            font-size: {f(22.)}; line-height: {u(30.)};
                            font-weight: 700;
                            color: #131720;
                        })),
                ),
        ),
    )
}

fn presence_card() -> Node {
    let (animation, animation_control) = segmented(
        "Animation",
        vec![
            ("Fade", PresenceAnimation::fade()),
            ("Scale", PresenceAnimation::fade().scale(0.85)),
            ("Slide", PresenceAnimation::fade().slide(0., 24.)),
            ("None", PresenceAnimation::none()),
        ],
    );
    let shown = signal(true);
    let panel = keyed(animation, move |animation| {
        presence(shown, || {
            center()
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(72.)};
                    padding: {(u(12.),)};
                    border-radius: {u(10.)};
                    background: {BLUE};
                })
                .children(
                    text("Mounted while shown, unmounted after the exit animation").style(
                        mue_macroquad::style! {
                            font-size: {f(14.)}; line-height: {u(20.)};
                            color: #131720;
                            text-align: center;
                        },
                    ),
                )
        })
        .animation(*animation)
        .style(mue_macroquad::style! { width: 100%; })
        .into_node()
    });
    card(
        "Presence (enter and exit)",
        "Like Vue's <Transition>: content animates in when shown and out before it is removed.",
        (
            animation_control,
            row((
                button(shown.map(|on| Rc::<str>::from(if on { "Hide" } else { "Show" })))
                    .primary(true)
                    .on_activate(move || shown.toggle()),
            )),
            flexbox()
                .style(mue_macroquad::style! { width: 100%; min-height: {u(72.)}; })
                .children(panel),
        ),
    )
}

#[derive(Clone, PartialEq)]
struct Item {
    id: u32,
    hue: u8,
}

fn list_card() -> Node {
    let next = signal(5u32);
    let items: Signal<Rc<[Item]>> = signal(
        (0..5)
            .map(|id| Item {
                id,
                hue: (id * 47 % 255) as u8,
            })
            .collect(),
    );
    let seed = signal(1u32);
    let random = move |bound: usize| {
        let mut x = seed.get_untracked();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        seed.set(x);
        x as usize % bound.max(1)
    };
    let add = move || {
        let id = next.get_untracked();
        next.set(id + 1);
        let mut list = items.get_clone_untracked().to_vec();
        let at = random(list.len() + 1);
        list.insert(
            at,
            Item {
                id,
                hue: (id * 47 % 255) as u8,
            },
        );
        items.set(list.into());
    };
    let remove = move || {
        let mut list = items.get_clone_untracked().to_vec();
        if !list.is_empty() {
            list.remove(random(list.len()));
            items.set(list.into());
        }
    };
    let shuffle = move || {
        let mut list = items.get_clone_untracked().to_vec();
        for i in (1..list.len()).rev() {
            list.swap(i, random(i + 1));
        }
        items.set(list.into());
    };
    let sort = move || {
        let mut list = items.get_clone_untracked().to_vec();
        list.sort_by_key(|item| item.id);
        items.set(list.into());
    };
    let group = transition_group(
        items,
        |item: &Item| item.id,
        |item: &Item| {
            let tone = item.hue;
            hstack()
                .style(mue_macroquad::style! {
                    width: 100%;
                    padding: {(u(12.), u(8.))};
                    gap: {u(10.)};
                    border-radius: {u(8.)};
                    background: {raised()};
                    align-items: center;
                })
                .children((
                    flexbox().style(mue_macroquad::style! {
                        width: {u(14.)}; height: {u(14.)};
                        border-radius: 9999px;
                        background: {mue_macroquad::paint::color::Color::from_rgba(120 + tone / 2, 140, 255 - tone / 2, 255)};
                    }),
                    text(format!("Item #{}", item.id)).style(mue_macroquad::style! {
                        font-size: {f(14.)}; line-height: {u(20.)};
                        color: {foreground()};
                    }),
                ))
        },
    )
    .animation(PresenceAnimation::fade().slide(-16., 0.))
    .style(mue_macroquad::style! { width: 100%; gap: {u(6.)}; flex-direction: column; });
    card(
        "Animated list",
        "Like Vue's <TransitionGroup>: new items fade in, removed items fade out, and the rest slide to their new places.",
        (
            row((
                button("Add").primary(true).on_activate(add),
                button("Remove").on_activate(remove),
                button("Shuffle").on_activate(shuffle),
                button("Sort").on_activate(sort),
            )),
            group,
        ),
    )
}

pub fn page() -> Node {
    page_body(
        "Property transitions, enter and exit animations, and keyed list animations.",
        (
            transitions_card(),
            presence_card(),
            list_card(),
            checklist(
                PAGE,
                &[
                    "Transitions retarget smoothly when switched mid-animation",
                    "Presence plays both the enter and the exit animation",
                    "List items slide into place after add, remove, and shuffle",
                    "Animations stay smooth (see FPS in the top bar)",
                ],
            ),
        ),
    )
}
