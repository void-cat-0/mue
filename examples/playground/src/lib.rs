//! An interactive exercise of node options. Run with `cargo run -p playground`.
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
mod theming;
mod typography;
use controls::*;
use macroquad::prelude::{clear_background, get_time, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
};

const PAGES: [&str; 8] = [
    "Layout",
    "Typography",
    "Graphics",
    "Motion",
    "Scrolling",
    "Input",
    "Theme",
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
        6 => theming::page(),
        _ => fonts::licenses(),
    };
    scroll_area()
        .size_full()
        .background(BG)
        // Leave touch drags to the recognizers on the input page; its scrollbar still works.
        .options(ScrollOptions {
            touch: index != 5,
            ..Default::default()
        })
        .children(
            container()
                .max_width(percent(100.))
                .min_width(0.)
                .padding((vw(3.), vh(3.)))
                .gap(vh(2.5))
                .children((
                    heading(PAGES[index])
                        .font_size(responsive::font(30.))
                        .line_height(responsive::ui(40.)),
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
                .transition(*value)
                .transition_animation(SPRING)
                .into_node()
        },
    );
    let mut app = App::new(
        stack()
            .size_full()
            .font_size(responsive::root_font())
            .gap(0.)
            .background(BG)
            .children((
                wrap()
                    .w_full()
                    .flex_none()
                    .padding((vw(2.), vh(1.5)))
                    .gap(vw(1.))
                    .children(
                        PAGES
                            .into_iter()
                            .enumerate()
                            .map(|(i, title)| button(title, move || selected.set(i)))
                            .collect::<Vec<_>>(),
                    ),
                separator(),
                flexbox()
                    .w_full()
                    .flex_1()
                    .min_width(0.)
                    .min_height(0.)
                    .children(page_view),
                wrap()
                    .w_full()
                    .flex_none()
                    .padding((vw(2.), vh(1.25)))
                    .gap(vw(1.))
                    .children((
                        button("Previous", move || {
                            selected.update(|index| *index = index.saturating_sub(1));
                        }),
                        button("Next", move || {
                            selected.update(|index| *index = (*index + 1).min(PAGES.len() - 1));
                        }),
                        text(selected.map(|index| format!("Page {} / {}", index + 1, PAGES.len())))
                            .font_size(responsive::font(16.))
                            .line_height(responsive::ui(24.))
                            .flex_1(),
                        text(fps.map(|fps| format!("{fps} FPS")))
                            .font_size(responsive::font(16.))
                            .line_height(responsive::ui(24.))
                            .min_width(0.)
                            .flex_none()
                            .text_wrap(false),
                        transition_control,
                    )),
            )),
    );
    let mut frames = 0;
    let mut sampled_at = get_time();
    loop {
        responsive::sync();
        clear_background(BG.into());
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
