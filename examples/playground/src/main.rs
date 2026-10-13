//! An interactive exercise of node options. Run with `cargo run -p playground`.
mod controls;
mod graphics;
#[path = "../../shared/halftone.rs"]
mod halftone;
mod input;
mod layout;
mod motion;
mod scrolling;
mod theming;
mod typography;
use controls::*;
use macroquad::prelude::{Conf, clear_background, get_time, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    node::*,
    style::{Styleable, StyleableExt},
};

const PAGES: [&str; 7] = [
    "Layout",
    "Typography",
    "Graphics",
    "Motion",
    "Scrolling",
    "Input",
    "Theme",
];
fn destination(index: usize) -> Node {
    let content = match index {
        0 => layout::page(),
        1 => typography::page(),
        2 => graphics::page(),
        3 => motion::page(),
        4 => scrolling::page(),
        5 => input::page(),
        _ => theming::page(),
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
                .max_width(1000.)
                .padding((24.,))
                .gap(20.)
                .children((
                    heading(PAGES[index]).font_size(30.).line_height(40.),
                    content,
                )),
        )
        .into_node()
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - playground".into(),
            window_width: 1120,
            window_height: 800,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    let path = signal(Vec::<usize>::new());
    let fps = signal(0u32);
    let (transition, transition_control) = choice(
        "Transition",
        vec![
            ("Slide", NavigationTransition::Slide),
            ("Vertical", NavigationTransition::SlideVertical),
            ("None", NavigationTransition::None),
        ],
    );
    // Rebuild only the navigation configuration; each active stack retains its own screen scopes.
    let (split, split_control) = toggle("Split");
    let revision = signal(0u32);
    watch(*split, move |_| revision.update(|n| *n += 1));
    watch(transition, move |_| revision.update(|n| *n += 1));
    let stacks = map_keyed(
        revision.map(|n| std::rc::Rc::from([n])),
        |n| *n,
        move |_| {
            navigation_stack(path, move || destination(0), |index| destination(*index))
                .transition(transition.get())
                .transition_animation(SPRING)
                .split_view(300., if split.get() { 800. } else { 10000. })
                .into_node()
        },
    );
    let mut app = App::new(
        stack().size_full().gap(0.).background(BG).children((
            wrap().padding((18., 12.)).gap(10.).children(
                PAGES
                    .into_iter()
                    .enumerate()
                    .map(|(i, title)| {
                        button(title, move || {
                            path.set(if i == 0 { vec![] } else { vec![i] })
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
            separator(),
            flexbox().flex_1().min_height(0.).children(stacks),
            hstack().padding((18., 10.)).children((
                button("Back", move || {
                    path.update(|p| {
                        p.pop();
                    });
                }),
                button("Next", move || {
                    let next = path.get_clone().last().map_or(1, |n| (n + 1) % PAGES.len());
                    path.update(|p| p.push(next));
                }),
                text(path.map(|p| format!("Depth {}", p.len())))
                    .font_size(16.)
                    .flex_1(),
                text(fps.map(|fps| format!("{fps} FPS")))
                    .font_size(16.)
                    .width(90.)
                    .flex_none()
                    .text_wrap(false),
                split_control,
                transition_control,
            )),
        )),
    );
    let mut frames = 0;
    let mut sampled_at = get_time();
    loop {
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
