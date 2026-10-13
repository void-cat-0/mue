//! Reactive themes, factory variants, and a nested theme scope.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{Owned, prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    animation::{Animation, with_animation},
    color::Color,
    factory::factory,
    node::*,
    style::{Styleable, StyleableExt},
    theme::{provide_theme, theme},
};
use std::rc::Rc;

const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const NIGHT: Color = Color::from_rgba(22, 30, 44, 255);
const INK: Color = Color::from_rgba(30, 40, 55, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const MUTED: Color = Color::from_rgba(118, 130, 149, 255);
const GREEN: Color = Color::from_rgba(46, 110, 76, 255);
const WHITE: Color = Color::WHITE;

#[derive(Clone)]
struct ButtonOptions {
    primary: bool,
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - theme".into(),
            window_width: 640,
            window_height: 480,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let dark = signal(false);
    let count = signal(0);
    let label = factory(|caption: Prop<Rc<str>>| text(caption))
        .style(|s, _| s.font_size(22.).line_height(32.));
    let button = factory(|caption: Prop<Rc<str>>| {
        flexbox()
            .items_center()
            .justify_center()
            .children(text(caption).font_size(18.).line_height(26.).color(WHITE))
    })
    .defaults(ButtonOptions { primary: true })
    .style(|s, _| s.padding((20., 12.)).radius(12.).margin_right(12.));
    let canvas = factory(|children: Owned<Children>| {
        flexbox()
            .size_full()
            .flex_column()
            .padding((40.,))
            .children(children)
    });

    let make_theme = |paper, ink| {
        theme!(|c, s| match c {
            canvas => s.background(paper),
            label => s.color(ink),
            button(o) if o.primary => s.background(BLUE),
            button => s.background(MUTED),
            _ => s,
        })
    };
    let active = dark.select(make_theme(NIGHT, WHITE), make_theme(PAPER, INK));
    let success = theme!(|c, s| match c {
        button => s.background(GREEN),
        _ => s,
    });

    let mut app = App::new(provide_theme(active, move || {
        canvas.create(
            (
                label
                    .create("Daily practice")
                    .font_size(32.)
                    .line_height(44.),
                label
                    .create(count.map(|n| format!("{n} sessions complete")))
                    .margin_top(24.)
                    .margin_bottom(32.),
                flexbox().flex_row().children((
                    provide_theme(success, {
                        let button = button.clone();
                        move || {
                            button
                                .create("Complete session")
                                .on_activate(move || count.update(|n| *n += 1))
                        }
                    }),
                    button
                        .create("Reset")
                        .options(|o| o.primary = false)
                        .on_activate(move || count.set(0)),
                )),
                button
                    .create(dark.select("Light theme", "Dark theme"))
                    .margin_top(32.)
                    .on_activate(move || {
                        with_animation(Animation::ease_in_out(0.3), || dark.toggle());
                    }),
            )
                .into_children(),
        )
    }));

    loop {
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}
