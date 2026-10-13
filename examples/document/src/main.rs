//! Read-only rich paragraphs and Markdown. Run with `cargo run -p document`.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt},
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
    let mut app = App::new(
        flexbox()
            .size_full()
            .background(PAPER)
            .padding((32.,))
            .flex_column()
            .children((
                flexbox()
                    .flex_row()
                    .items_center()
                    .margin_bottom(24.)
                    .children((
                        text("Field journal").font_size(20.).color(BLUE).flex_1(),
                        flexbox()
                            .padding((12., 8.))
                            .background(BLUE)
                            .toggle_on_tap(large)
                            .children(
                                text(large.select("Smaller type", "Larger type")).font_size(16.),
                            ),
                        flexbox()
                            .margin_left(12.)
                            .padding((12., 8.))
                            .background(BLUE)
                            .toggle_on_tap(alternate)
                            .children(
                                text(alternate.select("Previous entry", "Next entry"))
                                    .font_size(16.),
                            ),
                    )),
                markdown(source)
                    .w_full()
                    .max_width(760.)
                    .font_size(large.select(22., 18.))
                    .line_height(large.select(31., 26.))
                    .color(INK)
                    .flex_none(),
                document(Document::rich([
                    Span::new("Mara Ellis").bold(),
                    Span::new("\nOctober 13, 2026").italic(),
                ]))
                .margin_top(24.)
                .font_size(16.)
                .line_height(23.)
                .color(BLUE)
                .flex_none(),
            )),
    );
    loop {
        clear_background(PAPER.into());
        app.frame();
        next_frame().await;
    }
}
