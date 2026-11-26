//! Renders the screenshots of the Learn guide (`docs/learn`). Each shot is
//! one step of the guide's Focus timer, built with the same code the chapter
//! shows.
//!
//! Run one with `GUIDE_SHOT=timer MUE_CAPTURE=out.png cargo run --manifest-path
//! examples/Cargo.toml --example guide`; without `MUE_CAPTURE` the window stays
//! open. Shots: window, timer, history, layout, ring, components, theme-light,
//! theme-dark, settings, input, tasks, i18n-en, i18n-zh.

#[path = "../../foundation/src/capture.rs"]
mod capture;

use std::rc::Rc;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    font::{self, fontdb},
    math::vec2,
    paint::{
        color::Color,
        shader::{Brush, solid},
    },
};

fn clock(seconds: u32) -> Rc<str> {
    Rc::from(format!("{:02}:{:02}", seconds / 60, seconds % 60))
}

fn page(dark: bool, content: impl IntoNode + 'static) -> Node {
    let canvas = if dark {
        Color::from_rgba(24, 22, 20, 255)
    } else {
        Color::from_rgba(244, 239, 230, 255)
    };
    center()
        .style(style! { width: 100%; height: 100%; })
        .background(canvas)
        .children(content.into_node())
        .into_node()
}

fn window() -> Node {
    page(
        false,
        text("25:00").style(style! { font-size: 64px; color: #2d2a26; }),
    )
}

fn timer() -> Node {
    page(
        false,
        vstack()
            .gap(16.)
            .style(style! { align-items: center; })
            .children((
                text(clock(24 * 60 + 57)).style(style! { font-size: 64px; color: #2d2a26; }),
                button("Pause").primary(true),
            )),
    )
}

fn history() -> Node {
    let rows = (1..=3)
        .map(|id| {
            text(Rc::<str>::from(format!("Session {id}: 25 min")))
                .style(style! { color: #2d2a26; })
                .into_node()
        })
        .collect::<Vec<_>>();
    page(
        false,
        vstack()
            .gap(8.)
            .style(style! { font-size: 18px; color: #2d2a26; })
            .children((
                text("75 minutes today").style(style! { font-weight: 700; color: #2d2a26; }),
                hstack()
                    .gap(8.)
                    .children((button("Finish session"), button("Clear"))),
                vstack().gap(4.).children(rows),
            )),
    )
}

fn stat_row(highlight_first: bool) -> FlexboxBuilder {
    let stat = |label: &'static str, value: &'static str, highlight: bool| {
        let color = if highlight {
            Color::from_rgba(224, 122, 79, 255)
        } else {
            Color::from_rgba(45, 42, 38, 255)
        };
        vstack()
            .style(style! { flex-grow: 1; align-items: center; gap: 2px; })
            .children((
                text(value)
                    .style(style! { font-size: 20px; font-weight: 700; })
                    .color(color),
                text(label).style(style! { font-size: 13px; color: #8a8178; }),
            ))
    };
    hstack().style(style! { width: 100%; }).children((
        stat("today", "75 min", highlight_first),
        stat("sessions", "3", false),
        stat("streak", "4 days", false),
    ))
}

fn layout() -> Node {
    page(
        false,
        vstack()
            .style(style! { width: 320px; padding: 24px; gap: 20px; align-items: center; })
            .children((
                text("25:00").style(style! { font-size: 56px; color: #2d2a26; }),
                stat_row(false),
            )),
    )
}

fn ring_arc(progress: f32) -> PathData {
    let angle = progress.clamp(0.001, 0.999) * std::f32::consts::TAU;
    let (x, y) = (50. + 44. * angle.sin(), 50. - 44. * angle.cos());
    let large = u8::from(progress > 0.5);
    PathData::parse(&format!("M50 6 A44 44 0 {large} 1 {x} {y}")).unwrap()
}

fn ring() -> Node {
    let remaining = 1000;
    page(
        false,
        vstack()
            .style(style! {
                padding: 24px;
                background: #fffaf3;
                border-radius: 24px;
                corner-shape: squircle;
                border: 1px solid #eadfce;
                box-shadow: 0 12px 32px rgba(70, 50, 20, 0.18), 0 2px 6px rgba(70, 50, 20, 0.12);
            })
            .children(
                flexbox()
                    .style(style! { width: 200px; height: 200px; })
                    .children((
                        path(PathData::circle(vec2(50., 50.), 44.))
                            .stroke_current_color()
                            .stroke_style(StrokeStyle::round(8.))
                            .style(style! { position: absolute; width: 100%; height: 100%; color: #efe4d4; }),
                        path(ring_arc(remaining as f32 / 1500.))
                            .stroke_current_color()
                            .stroke_style(StrokeStyle::round(8.))
                            .style(style! { position: absolute; width: 100%; height: 100%; color: #e07a4f; }),
                        center()
                            .style(style! { width: 100%; height: 100%; })
                            .children(
                                text(clock(remaining))
                                    .style(style! { font-size: 40px; color: #2d2a26; }),
                            ),
                    )),
            ),
    )
}

fn components() -> Node {
    page(
        false,
        flexbox()
            .style(style! { width: 320px; })
            .children(stat_row(true)),
    )
}

fn theme_card(dark: bool) -> Node {
    let (card, ink) = if dark {
        (
            Color::from_rgba(38, 35, 32, 255),
            Color::from_rgba(240, 233, 222, 255),
        )
    } else {
        (
            Color::from_rgba(255, 250, 243, 255),
            Color::from_rgba(45, 42, 38, 255),
        )
    };
    page(
        dark,
        center()
            .style(style! { width: 280px; padding: 24px; border-radius: 20px; })
            .background(solid(card))
            .children(
                text("25:00")
                    .style(style! { font-size: 56px; })
                    .color(Brush::Solid(ink)),
            ),
    )
}

fn settings() -> Node {
    vstack()
        .style(style! {
            width: 100%; height: 100%; gap: 16px;
            align-items: center; justify-content: center;
            background: #fffaf3;
        })
        .children((
            text("Session length: 50 min").style(style! { font-size: 22px; color: #2d2a26; }),
            hstack()
                .gap(8.)
                .children((button("15"), button("25"), button("50"))),
            button("Done").primary(true),
        ))
        .into_node()
}

fn input_shot() -> Node {
    let name = signal(String::from("Write chapter 7"));
    page(
        false,
        vstack()
            .gap(8.)
            .style(style! { width: 300px; font-size: 15px; })
            .children((
                input().model(name).placeholder("What are you working on?"),
                text("Now: Write chapter 7").style(style! { font-size: 14px; color: #8a8178; }),
            )),
    )
}

fn tasks() -> Node {
    page(
        false,
        text("3 sessions, 75 minutes in total").style(style! { font-size: 18px; color: #2d2a26; }),
    )
}

/// The i18n shots also return the `I18n`, which must outlive the view.
fn i18n(locale: &str) -> (Node, (mue_i18n::I18n, mue_core::scope::OwnedScope)) {
    use mue_i18n::{CatalogBuilder, I18n, langid, tl};
    let mut catalog = CatalogBuilder::new();
    catalog
        .add_ftl(
            langid!("en-US"),
            "focus",
            "start = Start\npause = Pause\nsessions = { $count ->\n    [one] { $count } session today\n   *[other] { $count } sessions today\n}\n",
        )
        .unwrap();
    catalog
        .add_ftl(
            langid!("zh-CN"),
            "focus",
            "start = 开始\npause = 暂停\nsessions = 今天完成了 { $count } 个专注时段\n",
        )
        .unwrap();
    let i18n = I18n::new(catalog.build().unwrap(), langid!("en-US")).unwrap();
    if locale == "zh" {
        i18n.set_locale(langid!("zh-CN")).unwrap();
    }
    let focus = i18n.translator("focus").unwrap();
    // Translation bindings belong to a scope that lives as long as the view.
    let owner = mue_core::scope::OwnedScope::new();
    let view = owner.run(|| {
        let count = signal(3_u32);
        let start = focus.text("start");
        page(
            false,
            vstack()
                .gap(12.)
                .style(style! { align-items: center; font-size: 20px; color: #2d2a26; })
                .children((
                    text(tl!(focus, "sessions", "count" => count.get()))
                        .style(style! { color: #2d2a26; }),
                    button(start),
                )),
        )
    });
    (view, (i18n, owner))
}

/// The view for a shot, and anything that must live as long as it.
fn shot(name: &str) -> (Node, Option<Box<dyn std::any::Any>>) {
    let view = match name {
        "window" => window(),
        "timer" => timer(),
        "history" => history(),
        "layout" => layout(),
        "ring" => ring(),
        "components" => components(),
        "theme-light" => theme_card(false),
        "theme-dark" => theme_card(true),
        "settings" => settings(),
        "input" => input_shot(),
        "tasks" => tasks(),
        "i18n-en" | "i18n-zh" => {
            let (view, i18n) = i18n(if name == "i18n-zh" { "zh" } else { "en" });
            return (view, Some(Box::new(i18n)));
        }
        other => panic!("unknown guide shot {other:?}"),
    };
    (view, None)
}

fn fonts() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    for bytes in [
        &include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf")[..],
        &include_bytes!("../../playground/assets/fonts/NotoSans-Bold.ttf")[..],
        &include_bytes!("../../playground/assets/fonts/NotoSansCJKsc-Regular.otf")[..],
        &include_bytes!("../../playground/assets/fonts/NotoSansCJKsc-Bold.otf")[..],
    ] {
        db.load_font_data(bytes.to_vec());
    }
    db.set_sans_serif_family("Noto Sans");
    db
}

fn conf() -> Conf {
    Conf {
        window_title: "guide".into(),
        window_width: 560,
        window_height: 320,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    font::initialize("en-US", fonts()).expect("fonts");
    let name = std::env::var("GUIDE_SHOT").unwrap_or_else(|_| "timer".into());
    let mut capture = capture::Capture::from_env();
    if !capture.enabled() {
        eprintln!("Showing shot {name:?}; set MUE_CAPTURE=out.png to save it.");
    }
    let focus_field = name == "input";
    let (view, _keep) = shot(&name);
    let mut app = App::new(view);
    let mut first = true;
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        if std::mem::take(&mut first) && focus_field {
            mue_macroquad::input::focus::move_focus(true);
        }
        if capture.after_draw() {
            break;
        }
        next_frame().await;
    }
}
