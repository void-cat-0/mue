use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    app::runtime::set_timeout,
    motion::transition::{Transition, animated},
    paint::shader::Brush,
    style::{Size, Styleable, StyleableExt, Transform, TransformOp},
};
use mue_components::motion::transition_group::transition_group;
use mue_components::motion::presence::{PresenceAnimation, presence};
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
    let (big, big_control) = option_switch("Size", false);
    let (round, round_control) = option_switch("Radius", false);
    let (warm, warm_control) = option_switch("Color", false);
    let (faded, faded_control) = option_switch("Opacity", false);
    let (moved, moved_control) = option_switch("Translate", false);
    let (turned, turned_control) = option_switch("Rotate", false);
    let (tilted, tilted_control) = option_switch("3D tilt", false);
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
                    border-radius: {mue_components::system::radius::MD};
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
                    border-radius: {mue_components::system::radius::MD};
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
        (
            row((
                button("Add item").primary(true).on_activate(add),
                button("Remove item").on_activate(remove),
                button("Shuffle").on_activate(shuffle),
                button("Sort").on_activate(sort),
            )),
            group,
        ),
    )
}

/// Four balls race across the same track, each with its own curve.
fn springs_card() -> Node {
    let right = signal(false);
    let curves = [
        ("spring(0.6, 0.0) — no bounce", Transition::spring(0.6, 0.), ACCENT_A),
        ("spring(0.6, 0.3)", Transition::spring(0.6, 0.3), ACCENT_B),
        ("spring(0.9, 0.7) — bouncy", Transition::spring(0.9, 0.7), CORAL),
        ("ease-in-out 0.6 s", Transition::ease_in_out(0.6), GREEN),
        ("linear 0.6 s", Transition::linear(0.6), mue_macroquad::paint::color::Color::from_rgba(200, 200, 215, 255)),
    ];
    let lanes = curves
        .into_iter()
        .map(|(label, curve, color)| {
            // The ball's left edge as a fraction of the free track width.
            let progress = animated::<f32>(right.map(|right| if right { 1_f32 } else { 0. }), curve);
            stack()
                .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(4.)}; })
                .children((
                    hint(label),
                    flexbox()
                        .style(mue_macroquad::style! {
                            position: relative;
                            width: 100%;
                            height: {u(28.)};
                            border-radius: 9999px;
                            background: {raised()};
                        })
                        .children(
                            flexbox()
                                .absolute()
                                .inset_top(Size::Px(0.))
                                // Percent insets resolve against the track width.
                                .inset_left(progress.map(|p| Size::Percent(p * 100.)))
                                .style(mue_macroquad::style! {
                                    width: {u(28.)}; height: {u(28.)};
                                    border-radius: 9999px;
                                    background: {color};
                                })
                                .transform_list(progress.map(|p| {
                                    // Keep the ball inside the track: shift it back by its own width at the end.
                                    Transform(vec![TransformOp::Translate(Size::Percent(-100. * p), Size::Px(0.))])
                                })),
                        ),
                ))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Springs and curves",
        (
            row((button(right.map(|right| Rc::<str>::from(if right { "Move left" } else { "Move right" })))
                .primary(true)
                .on_activate(move || right.toggle()),)),
            stack()
                .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(10.)}; })
                .children(lanes),
        ),
    )
}

/// A grid of tiles that appear one after another, from the top left.
fn stagger_card() -> Node {
    const COLUMNS: usize = 8;
    const COUNT: usize = 24;
    let shown: Vec<Signal<bool>> = (0..COUNT).map(|_| signal(true)).collect();
    let (step, step_control) = segmented("Stagger", vec![("40 ms", 0.04_f32), ("15 ms", 0.015), ("90 ms", 0.09)]);
    let play = {
        let shown = shown.clone();
        move || {
            for tile in &shown {
                tile.set(false);
            }
            let step = step.get_untracked();
            for (index, tile) in shown.iter().copied().enumerate() {
                // Diagonal order: tiles on the same diagonal enter together.
                let order = index % COLUMNS + index / COLUMNS;
                set_timeout(0.15 + order as f32 * step, move || tile.set(true));
            }
        }
    };
    let tiles = shown
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            let enter = animated::<f32>(tile.map(|on| if on { 1_f32 } else { 0. }), tile.map(|on| {
                if on { Transition::spring(0.5, 0.35) } else { Transition::Instant }
            }));
            let hue = index as f32 / COUNT as f32;
            let color = ACCENT_A.lerp(CORAL, hue);
            flexbox()
                .style(mue_macroquad::style! {
                    height: {u(36.)};
                    border-radius: {mue_components::system::radius::MD};
                    background: {color};
                })
                .opacity(enter.clone().map(|enter| enter.clamp(0., 1.)))
                .transform_list(enter.map(|enter| {
                    let scale = 0.4 + 0.6 * enter;
                    Transform(vec![
                        TransformOp::Translate(Size::Px(0.), Size::Px((1. - enter) * 14.)),
                        TransformOp::Scale(scale, scale),
                    ])
                }))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Staggered entrance",
        (
            step_control,
            row((button("Play").primary(true).on_activate(play),)),
            grid()
                .style(mue_macroquad::style! {
                    width: 100%;
                    gap: {u(8.)};
                    grid-template-columns: repeat(8, 1fr);
                })
                .children(tiles),
        ),
    )
}

/// Motion computed every frame from the clock: orbiting dots and a wave.
fn clock_card() -> Node {
    let time = super::styles::animation_time();
    let (running, running_control) = option_switch("Running", true);
    // Freeze the clock while paused.
    let frozen = signal(0_f32);
    let offset = signal(0_f32);
    watch(*running, move |running| {
        if running.get() {
            offset.set(offset.get_untracked() + (time.get_untracked() - frozen.get_untracked()));
        } else {
            frozen.set(time.get_untracked());
        }
    });
    let clock = computed(move |_| {
        if running.get() { time.get() - offset.get() } else { frozen.get() - offset.get() }
    });
    let dots = (0..8)
        .map(|index| {
            let phase = index as f32 / 8. * std::f32::consts::TAU;
            let color = ACCENT_A.lerp(ACCENT_B, index as f32 / 7.);
            flexbox()
                .absolute()
                .inset_left(Size::Percent(50.))
                .inset_top(Size::Percent(50.))
                .style(mue_macroquad::style! {
                    width: {u(14.)}; height: {u(14.)};
                    margin-left: {u(-7.)}; margin-top: {u(-7.)};
                    border-radius: 9999px;
                    background: {color};
                })
                .transform_list(clock.map(move |t| {
                    let angle = t * 1.2 + phase;
                    let radius = 46. + 10. * (t * 2. + phase * 2.).sin();
                    let scale = super::responsive::scale();
                    Transform(vec![TransformOp::Translate(
                        Size::Px(angle.cos() * radius * scale),
                        Size::Px(angle.sin() * radius * scale),
                    )])
                }))
                .into_node()
        })
        .collect::<Vec<_>>();
    let bars = (0..16)
        .map(|index| {
            flexbox()
                .style(mue_macroquad::style! {
                    flex: 1;
                    border-radius: 9999px;
                })
                .background(accent_gradient())
                .height(clock.map(move |t| {
                    let wave = (t * 3. - index as f32 * 0.45).sin() * 0.5 + 0.5;
                    Size::Percent(20. + 80. * wave)
                }))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Frame-driven motion",
        (
            running_control,
            row((
                flexbox()
                    .style(mue_macroquad::style! {
                        position: relative;
                        flex: none;
                        width: {u(150.)}; height: {u(150.)};
                        border-radius: {mue_components::system::radius::LG};
                        background: {raised()};
                    })
                    .children(dots),
                hstack()
                    .style(mue_macroquad::style! {
                        flex: 1;
                        min-width: {u(180.)};
                        height: {u(150.)};
                        padding: {(u(14.),)};
                        gap: {u(5.)};
                        align-items: flex-end;
                        border-radius: {mue_components::system::radius::LG};
                        background: {raised()};
                    })
                    .children(bars),
            )),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            transitions_card(),
            springs_card(),
            stagger_card(),
            presence_card(),
            list_card(),
            clock_card(),
            checklist(
                PAGE,
                &[
                    "Switching mid-animation retargets smoothly",
                    "Springs overshoot and turn around mid-flight",
                    "The wave fills the grid diagonally at each stagger",
                    "Presence plays the enter and the exit animation",
                    "Items slide into place after every list change",
                    "Orbit and equalizer freeze when paused",
                    "The fps readout stays steady while animating",
                ],
            ),
        ),
    )
}
