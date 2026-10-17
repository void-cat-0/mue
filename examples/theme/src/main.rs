//! Reactive themes, factory variants, and a nested theme scope.
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{Owned, prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    animation::{Animation, with_animation},
    color::Color,
    factory::factory,
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
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
    responsive::sync();
    let dark = signal(false);
    let count = signal(0);
    let label = factory(|caption: Prop<Rc<str>>| text(caption)).style(|s, _| {
        s.font_size(responsive::font(22.))
            .line_height(responsive::ui(32.))
    });
    let button = factory(|caption: Prop<Rc<str>>| {
        flexbox().items_center().justify_center().children(
            text(caption)
                .font_size(responsive::font(18.))
                .line_height(responsive::ui(26.))
                .color(WHITE)
                .min_width(0.),
        )
    })
    .defaults(ButtonOptions { primary: true })
    .style(|s, _| {
        s.padding((responsive::ui(19.2), responsive::ui(12.)))
            .min_height(responsive::ui(44.))
            .min_width(0.)
            .max_width(percent(100.))
            .flex_shrink(0.)
            .radius(responsive::ui(12.))
    });
    let canvas = factory(|children: Owned<Children>| {
        scroll_area()
            .size_full()
            .font_size(responsive::root_font())
            .children(
                stack()
                    .w_full()
                    .padding((vw(6.), vh(6.)))
                    .children(children),
            )
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
                    .font_size(responsive::font(32.))
                    .line_height(responsive::ui(44.)),
                label
                    .create(count.map(|n| format!("{n} sessions complete")))
                    .margin_top(vh(5.))
                    .margin_bottom(vh(6.5)),
                wrap().column_gap(vw(2.)).row_gap(vh(2.)).children((
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
                    .margin_top(vh(6.5))
                    .on_activate(move || {
                        with_animation(Animation::ease_in_out(0.3), || dark.toggle());
                    }),
            )
                .into_children(),
        )
    }));

    loop {
        responsive::sync();
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}
