//! An interactive exercise of node options. Run with `cargo run -p playground`.
use mue_components::*;
mod controls;
mod fonts;
mod graphics;
#[path = "../../shared/halftone.rs"]
mod halftone;
mod input;
mod layout;
mod motion;
#[path = "../../shared/responsive.rs"]
mod responsive;
mod scrolling;
mod styles;
mod theming;
mod typography;
use controls::*;
use macroquad::prelude::{clear_background, get_time, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    style::{Styleable, vh, vw},
};

const PAGES: [&str; 10] = [
    "Layout",
    "Typography",
    "Graphics",
    "Motion",
    "Scrolling",
    "Input",
    "Map gestures",
    "Theme",
    "Style declarations",
    "Font licenses",
];

fn destination(index: usize) -> Node {
    let content = match index {
        0 => layout::page(),
        1 => typography::page(),
        2 => graphics::page(),
        3 => motion::page(),
        4 => scrolling::page(),
        5 => input::page(),
        6 => input::map_page(),
        7 => theming::page(),
        8 => styles::page(),
        _ => fonts::licenses(),
    };
    scroll_area()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        background: {background()};
        })
        .options(ScrollOptions {
            touch: true,
            ..Default::default()
        })
        .children(
            container()
                .style(mue_macroquad::style! {
                max-width: 100%;
                min-width: 0px;
                padding: {(vw(3.), vh(3.))};
                gap: {vh(2.5)};
                })
                .children((
                    heading(PAGES[index]).style(mue_macroquad::style! {
                    font-size: {responsive::font(30.)};
                    line-height: {responsive::ui(40.)};
                    }),
                    content,
                )),
        )
        .into_node()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - playground".into(),
            ..Default::default()
        },
        run(),
    );
}

pub async fn run() {
    fonts::initialize();
    responsive::sync();
    let selected = signal(0usize);
    let fps = signal(0u32);
    let zoom = responsive::zoom();
    let mut app = App::new(mue_macroquad::theme::provide_foreground(
        foreground(),
        || {
            let (transition, transition_control) = choice(
                "Transition",
                vec![
                    ("Slide", PagesTransition::Slide),
                    ("Vertical", PagesTransition::SlideVertical),
                    ("None", PagesTransition::None),
                ],
            );
            let page_view = map_keyed(
                transition.map(|value| std::rc::Rc::from([value])),
                |value| *value as u8,
                move |value| {
                    pages(selected, (0..PAGES.len()).map(destination))
                        .page_transition(*value)
                        .transition_curve(SPRING)
                        .into_node()
                },
            );
            stack()
                .style(mue_macroquad::style! {
                width: 100%; height: 100%;
                font-size: {responsive::root_font()};
                transition: 180ms ease-in-out;
                gap: 0px;
                background: {background()};
                })
                .children((
                    scroll_area()
                        .axis(ScrollAxis::Horizontal)
                        .style(mue_macroquad::style! {
                        width: 100%;
                        height: {responsive::ui(56.)};
                        flex: none;
                        background: {panel_brush()};
                        })
                        .options(ScrollOptions {
                            scrollbars: false,
                            axis: ScrollAxis::Horizontal,
                            ..Default::default()
                        })
                        .content_style(mue_macroquad::style! {
                            gap: {vw(1.)};
                            padding-left: {vw(2.)};
                            padding-right: {vw(2.)};
                            align-items: center;
                        })
                        .children(
                            PAGES
                                .into_iter()
                                .enumerate()
                                .map(|(i, title)| tab_button(title, selected, i))
                                .collect::<Vec<_>>(),
                        ),
                    separator(),
                    flexbox()
                        .style(mue_macroquad::style! {
                        width: 100%;
                        flex: 1;
                        min-width: 0px;
                        min-height: 0px;
                        })
                        .children(page_view),
                    wrap()
                        .style(mue_macroquad::style! {
                        width: 100%;
                        flex: none;
                        padding: {(vw(2.), vh(1.25))};
                        gap: {vw(1.)};
                        })
                        .children((
                            button("Previous", move || {
                                selected.update(|index| *index = index.saturating_sub(1));
                            }),
                            button("Next", move || {
                                selected.update(|index| *index = (*index + 1).min(PAGES.len() - 1));
                            }),
                            button(
                                computed(move |_| format!("Scale {:.0}%", zoom.get() * 100.)),
                                responsive::cycle_zoom,
                            ),
                            text(
                                selected
                                    .map(|index| format!("Page {} / {}", index + 1, PAGES.len())),
                            )
                            .style(mue_macroquad::style! {
                            font-size: {responsive::font(16.)};
                            line-height: {responsive::ui(24.)};
                            flex: 1;
                            }),
                            text(fps.map(|fps| format!("{fps} FPS"))).style(
                                mue_macroquad::style! {
                                font-size: {responsive::font(16.)};
                                line-height: {responsive::ui(24.)};
                                min-width: 0px;
                                flex: none;
                                text-wrap: {false};
                                },
                            ),
                            transition_control,
                        )),
                ))
        },
    ));
    let mut frames = 0;
    let mut sampled_at = get_time();
    loop {
        responsive::sync();
        styles::update_animation_time(get_time() as f32);
        clear_background(background_color().into());
        app.frame();
        next_frame().await;
        frames += 1;
        let now = get_time();
        let elapsed = now - sampled_at;
        // Average over half a second so the readout stays readable.
        if elapsed >= 0.5 {
            fps.set((frames as f64 / elapsed).round() as u32);
            frames = 0;
            sampled_at = now;
        }
    }
}
