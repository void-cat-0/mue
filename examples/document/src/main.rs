//! Read-only rich paragraphs and Markdown. Run with `cargo run -p document`.
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    node::*,
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
            .size_full()
            .font_size(responsive::root_font())
            .background(PAPER)
            .padding((Size::vw(3.5),))
            .children((
                wrap()
                    .w_full()
                    .flex_none()
                    .gap(responsive::ui(12.))
                    .margin_bottom(responsive::ui(24.))
                    .children((
                        heading("Field journal")
                            .font_size(responsive::font(20.))
                            .line_height(responsive::ui(44. * 1.45))
                            .color(BLUE)
                            .min_width(0.)
                            .flex_basis(Size::percent(30.))
                            .flex_1(),
                        flexbox()
                            .flex_none()
                            .max_width(Size::percent(100.))
                            .padding((responsive::ui(12.), responsive::ui(8.)))
                            .min_width(responsive::ui(44.))
                            .min_height(responsive::ui(44.))
                            .background(BLUE)
                            .toggle_on_tap(large)
                            .children(
                                text(large.select("Smaller type", "Larger type"))
                                    .font_size(responsive::font(16.))
                                    .line_height(responsive::ui(44.)),
                            ),
                        flexbox()
                            .flex_none()
                            .max_width(Size::percent(100.))
                            .padding((responsive::ui(12.), responsive::ui(8.)))
                            .min_width(responsive::ui(44.))
                            .min_height(responsive::ui(44.))
                            .background(BLUE)
                            .toggle_on_tap(alternate)
                            .children(
                                text(alternate.select("Previous entry", "Next entry"))
                                    .font_size(responsive::font(16.))
                                    .line_height(responsive::ui(44.)),
                            ),
                    )),
                scroll_area().flex_1().w_full().children(
                    stack()
                        .w_full()
                        .max_width(responsive::ui(760.))
                        .margin_x(Size::auto())
                        .children((
                            markdown(source)
                                .w_full()
                                .font_size(responsive::font(large.select(22., 18.)))
                                .line_height(responsive::ui(large.select(31., 26.)))
                                .color(INK)
                                .flex_none(),
                            document(Document::rich([
                                Span::new("Mara Ellis").bold(),
                                Span::new("\nOctober 13, 2026").italic(),
                            ]))
                            .margin_top(responsive::ui(24.))
                            .font_size(responsive::font(16.))
                            .line_height(responsive::ui(23.))
                            .color(BLUE)
                            .flex_none(),
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
