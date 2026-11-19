//! Read-only rich paragraphs and Markdown. Run with `cargo run -p document`.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    style::{Size, Styleable, StyleableExt},
};

const INK: Color = Color::from_rgba(30, 40, 55, 255);
const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const ENTRY: &str = r#"# Along the coast

The path turns north at the old lighthouse. Beyond the stone wall, **sea lavender** grows between the rocks and the water settles into narrow pools.

We leave before sunrise, carrying a flask of coffee and a folded map. The air is *cool and still*. By the time we reach the headland, the fishing boats are already out.

## In the field bag

- A small notebook
- A pencil and a spare
- The route saved as `coast.gpx`

> Keep a little room in the day for getting lost.

The afternoon belongs to the village harbour. We take the long way home."#;

const NEXT_ENTRY: &str = r#"# Back at the harbour

Rain arrives just after lunch. We wait beneath the awning, watching the boats move against their moorings.

**Tomorrow**, we will take the inland path.

## A note from home

你好，世界。

مرحبا بالعالم."#;

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - document".into(),
            window_width: 900,
            window_height: 900,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let large = signal(false);
    let alternate = signal(false);
    let source = alternate.select(NEXT_ENTRY, ENTRY);
    responsive::sync();
    let mut app = App::new(
        stack()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            background: {PAPER};
            padding: {(Size::vw(3.5),)};
            })
            .children((
                wrap()
                    .style(mue_macroquad::style! {
                    width: 100%;
                    flex: none;
                    gap: {responsive::ui(12.)};
                    margin-bottom: {responsive::ui(24.)};
                    })
                    .children((
                        heading("Field journal").style(mue_macroquad::style! {
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(44. * 1.45)};
                        color: {BLUE};
                        min-width: 0px;
                        flex-basis: 30%;
                        flex: 1;
                        }),
                        flexbox()
                            .style(mue_macroquad::style! {
                            flex: none;
                            max-width: 100%;
                            padding: {(responsive::ui(12.), responsive::ui(8.))};
                            min-width: {responsive::ui(44.)};
                            min-height: {responsive::ui(44.)};
                            background: {BLUE};
                            })
                            .toggle_on_tap(large)
                            .children(text(large.select("Smaller type", "Larger type")).style(
                                mue_macroquad::style! {
                                font-size: {responsive::font(16.)};
                                line-height: {responsive::ui(44.)};
                                },
                            )),
                        flexbox()
                            .style(mue_macroquad::style! {
                            flex: none;
                            max-width: 100%;
                            padding: {(responsive::ui(12.), responsive::ui(8.))};
                            min-width: {responsive::ui(44.)};
                            min-height: {responsive::ui(44.)};
                            background: {BLUE};
                            })
                            .toggle_on_tap(alternate)
                            .children(
                                text(alternate.select("Previous entry", "Next entry")).style(
                                    mue_macroquad::style! {
                                    font-size: {responsive::font(16.)};
                                    line-height: {responsive::ui(44.)};
                                    },
                                ),
                            ),
                    )),
                scroll_area()
                    .style(mue_macroquad::style! {
                    flex: 1;
                    width: 100%;
                    })
                    .children(
                        stack()
                            .style(mue_macroquad::style! {
                            width: 100%;
                            max-width: {responsive::ui(760.)};
                            margin-left: {Size::auto()};
                                                        margin-right: {Size::auto()};
                            })
                            .children((
                                markdown(source).style(mue_macroquad::style! {
                                width: 100%;
                                font-size: {responsive::font(large.select(22., 18.))};
                                line-height: {responsive::ui(large.select(31., 26.))};
                                color: {INK};
                                flex: none;
                                }),
                                document(Document::rich([
                                    Span::new("Mara Ellis").bold(),
                                    Span::new("\nOctober 13, 2026").italic(),
                                ]))
                                .style(mue_macroquad::style! {
                                margin-top: {responsive::ui(24.)};
                                font-size: {responsive::font(16.)};
                                line-height: {responsive::ui(23.)};
                                color: {BLUE};
                                flex: none;
                                }),
                            )),
                    ),
            )),
    );
    loop {
        clear_background(PAPER.into());
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}
