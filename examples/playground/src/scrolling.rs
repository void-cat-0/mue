use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    math::Vector,
    style::{Style, Styleable, vh, vw},
};

pub fn page() -> Node {
    let (axis, a) = choice(
        "Axis",
        vec![
            ("Vertical", ScrollAxis::Vertical),
            ("Horizontal", ScrollAxis::Horizontal),
            ("Both", ScrollAxis::Both),
        ],
    );
    let (touch, b) = toggle("Disable touch");
    let (mouse, c) = toggle("Mouse drag");
    let (wheel, d) = toggle("Disable wheel");
    let (speed, e) = choice("Wheel speed", vec![("40", 40.), ("10", 10.), ("80", 80.)]);
    let (threshold, f) = choice("Threshold", vec![("8", 8.), ("2", 2.), ("20", 20.)]);
    let (lock, g) = toggle("Unlock axes");
    let (momentum, h) = toggle("No momentum");
    let (friction, i) = choice("Friction", vec![("8", 8.), ("3", 3.), ("16", 16.)]);
    let (velocity, j) = choice(
        "Max velocity",
        vec![("6000", 6000.), ("800", 800.), ("2000", 2000.)],
    );
    let (chain, k) = toggle("Contain");
    let (bars, l) = toggle("Hide bars");
    let offset = signal(Vector::ZERO);
    let options = computed(move |_| {
        (
            axis.get(),
            touch.get(),
            mouse.get(),
            wheel.get(),
            speed.get(),
            threshold.get(),
            lock.get(),
            momentum.get(),
            friction.get(),
            velocity.get(),
            chain.get(),
            bars.get(),
        )
    });
    // Options configure input recognizers at build time. A keyed revision replaces only this viewport.
    let revision = signal(0u32);
    watch(options, move |_| revision.update(|v| *v += 1));
    let areas = map_keyed(
        revision.map(|n| std::rc::Rc::from([n])),
        |n| *n,
        move |_| {
            let (
                axis,
                touch,
                mouse,
                wheel,
                speed,
                threshold,
                lock,
                momentum,
                friction,
                velocity,
                chain,
                bars,
            ) = options.get();
            scroll_area()
                .style(mue_macroquad::style! {
                height: {vh(38.75)};
                width: 100%;
                })
                .offset(offset)
                .style(mue_macroquad::style! { background: {PANEL}; })
                .options(ScrollOptions {
                    axis,
                    touch: !touch,
                    mouse_drag: mouse,
                    wheel: !wheel,
                    wheel_speed: speed,
                    drag_threshold: threshold,
                    axis_lock: !lock,
                    momentum: !momentum,
                    friction,
                    max_velocity: velocity,
                    chain: !chain,
                    scrollbars: !bars,
                })
                .content_style(Style::new().style(mue_macroquad::style! {
                gap: {vh(1.5)};
                padding: {(vw(2.), vh(2.))};
                }))
                .children(
                    (0..20)
                        .map(|n| {
                            center()
                                .style(mue_macroquad::style! {
                                width: {responsive::ui(1200.)};
                                height: {responsive::ui(54.)};
                                flex: none;
                                background: {if n % 2 == 0 { BLUE } else { CORAL }};
                                })
                                .children(text(format!("Row {n}")).style(mue_macroquad::style! {
                                font-size: {responsive::font(22.)};
                                line-height: {responsive::ui(32.)};
                                }))
                                .into_node()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_node()
        },
    );
    let page = signal(0usize);
    let rows = map_keyed(
        page.map(|p| (p * 5..p * 5 + 5).collect::<std::rc::Rc<[usize]>>()),
        |n| *n,
        |n| {
            text(format!("Item {}", n + 1))
                .style(mue_macroquad::style! {
                font-size: {responsive::font(20.)};
                line-height: {responsive::ui(28.)};
                })
                .into_node()
        },
    );
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(2.5)};
        })
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j, k, l]),
            areas,
            wrap()
                .style(mue_macroquad::style! {
                width: 100%;
                gap: {vw(1.)};
                })
                .children((
                    button("Reset", move || offset.set(Vector::ZERO)),
                    text(offset.map(|v| format!("{:.0}, {:.0}", v.x, v.y))).style(
                        mue_macroquad::style! {
                        font-size: {responsive::font(18.)};
                        line-height: {responsive::ui(28.)};
                        },
                    ),
                )),
            section(
                "Pagination",
                stack().children((
                    list()
                        .style(mue_macroquad::style! {
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(28.)};
                        gap: {responsive::ui(8.)};
                        })
                        .children(rows),
                    wrap()
                        .style(mue_macroquad::style! {
                        width: 100%;
                        gap: {vw(1.)};
                        })
                        .children((
                            button("Previous", move || {
                                page.update(|p| *p = p.saturating_sub(1))
                            }),
                            text(page.map(|p| format!("{} / 8", p + 1)))
                                .style(
                                    mue_macroquad::style! { font-size: {responsive::font(18.)}; },
                                )
                                .style(
                                    mue_macroquad::style! { line-height: {responsive::ui(28.)}; },
                                ),
                            button("Next", move || page.update(|p| *p = (*p + 1).min(7))),
                        )),
                )),
            ),
        ))
        .into_node()
}
