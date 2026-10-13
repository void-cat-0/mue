//! Replace a typed navigation destination; the root keeps its local state. Run with `cargo run -p pages`.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::Animation,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt},
};

fn page(index: usize) -> Node {
    let count = signal(0);
    center()
        .size_full()
        .background(
            [
                Color::from_rgba(37, 58, 94, 255),
                Color::from_rgba(46, 82, 63, 255),
                Color::from_rgba(88, 52, 84, 255),
            ][index],
        )
        .children(
            stack().items_center().gap(20.).children((
                heading(["Home", "Notes", "Settings"][index]),
                link(count.map(|n| format!("Count: {n}")))
                    .on_activate(move || count.update(|n| *n += 1)),
            )),
        )
        .into_node()
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - pages".into(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    let path = signal(Vec::<usize>::new());
    let bar = hstack().padding((16.,)).justify_center().children(
        ["Home", "Notes", "Settings"]
            .into_iter()
            .enumerate()
            .map(|(index, title)| {
                link(title)
                    .font_size(20.)
                    .padding((12.,))
                    .on_activate(move || path.set(if index == 0 { vec![] } else { vec![index] }))
                    .into_node()
            })
            .collect::<Vec<_>>(),
    );
    let pages = navigation_stack(path, || page(0), |index| page(*index))
        .transition_animation(Animation::spring(0.5, 0.15))
        .flex_1()
        .min_height(0.);
    let mut app = App::new(
        stack()
            .size_full()
            .gap(0.)
            .children((bar, separator(), pages)),
    );
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        next_frame().await;
    }
}
