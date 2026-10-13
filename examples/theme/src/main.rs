//! Factories, option selectors, nested themes and keyed children.
//! Run with `cargo run -p theme`.

#[path = "../../palette.rs"]
mod palette;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{Owned, prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    factory::factory,
    node::{Children, IntoChildren, flexbox, map_keyed, text},
    style::{Styleable, StyleableExt},
    theme::{provide_theme, theme},
};
use std::rc::Rc;

#[derive(Clone)]
struct ButtonOptions {
    primary: bool,
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - theme".into(),
            window_width: 800,
            window_height: 640,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let label = factory(|caption: Prop<Rc<str>>| text(caption))
        .style(|s, _| s.color(palette::INK).font_size(18.).line_height(26.));
    let button = {
        let label = label.clone();
        factory(move |caption: String| {
            flexbox()
                .items_center()
                .justify_center()
                .children(label.create(caption))
        })
        .defaults(ButtonOptions { primary: true })
        .style(|s, _| s.height(44.).padding_x(18.).margin_right(8.))
    };
    let panel = factory(|children: Owned<Children>| flexbox().flex_column().children(children))
        .style(|s, _| s.padding((24.,)));

    // Both themes select the same factories; options choose the button variant.
    let make_theme = |accent| {
        theme!(|c, s| match c {
            button(o) if o.primary => s.background(accent),
            button => s.background(palette::HOVER),
            panel => s.background(palette::SURFACE),
            _ => s,
        })
    };
    let blue = make_theme(palette::BLUE);
    let plum = make_theme(palette::PLUM);
    let active = signal(blue.clone());
    let count = signal(2usize);
    let completed = signal(0u8);

    // A nested theme changes only these buttons. Instance styles still win.
    let completion_theme = theme!(|c, s| match c {
        button => s.background(palette::GREEN),
        _ => s,
    });

    let mut app = App::new(provide_theme(active, move || {
        let row_label = label.clone();
        let row_button = button.clone();
        let rows = map_keyed(
            computed(move |_| (0..count.get()).collect::<Vec<_>>().into()),
            |index| *index,
            move |&index| {
                let done = computed(move |_| completed.get() & (1 << index) != 0);
                let titles = [
                    "Sketch the layout",
                    "Choose the palette",
                    "Build the controls",
                    "Review the result",
                ];
                flexbox()
                    .flex_row()
                    .items_center()
                    .margin_bottom(12.)
                    .children((
                        row_label
                            .create(titles[index])
                            .flex_1()
                            .opacity(done.map(|done| if done { 0.5 } else { 1. })),
                        provide_theme(completion_theme.clone(), {
                            let button = row_button.clone();
                            move || {
                                button
                                    .create("Complete")
                                    .on_tap(move |_| completed.update(|bits| *bits ^= 1 << index))
                            }
                        }),
                        row_label
                            .create(done.map(|done| Rc::from(if done { "Done" } else { "Open" })))
                            .width(52.),
                    ))
            },
        );

        flexbox()
            .size_full()
            .flex_column()
            .padding((32.,))
            .children((
                flexbox()
                    .flex_row()
                    .items_center()
                    .margin_bottom(28.)
                    .children((
                        label
                            .create("Studio tasks")
                            .font_size(34.)
                            .line_height(44.)
                            .flex_1(),
                        button
                            .create("Blue")
                            .background(palette::BLUE)
                            .on_tap(move |_| active.set(blue.clone())),
                        button
                            .create("Plum")
                            .background(palette::PLUM)
                            .margin_right(0.)
                            .on_tap(move |_| active.set(plum.clone())),
                    )),
                panel.create(
                    (
                        flexbox()
                            .flex_row()
                            .items_center()
                            .margin_bottom(24.)
                            .children((
                                label
                                    .create("Today")
                                    .font_size(24.)
                                    .line_height(32.)
                                    .flex_1(),
                                label.create(computed(move |_| {
                                    let mask = (1u8 << count.get()) - 1;
                                    format!(
                                        "{} of {} complete",
                                        (completed.get() & mask).count_ones(),
                                        count.get()
                                    )
                                    .into()
                                })),
                            )),
                        flexbox().flex_column().min_height(224.).children(rows),
                        flexbox().flex_row().children((
                            button
                                .create("Add task")
                                .on_tap(move |_| count.update(|n| *n = (*n + 1).min(4)))
                                .opacity(count.map(|n| if n == 4 { 0.45 } else { 1. })),
                            button
                                .create("Reset")
                                .options(|o| o.primary = false)
                                .on_tap(move |_| {
                                    count.set(2);
                                    completed.set(0);
                                }),
                        )),
                    )
                        .into_children(),
                ),
            ))
    }));

    loop {
        clear_background(palette::CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
