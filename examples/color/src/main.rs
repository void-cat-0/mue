//! The same Brush types work for background fills and foreground color.
//! Run with `cargo run -p color`.

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_macroquad::{
    App,
    color::Color as UiColor,
    math::vec2,
    node::{IntoNode, Node, flexbox, text},
    shader::{Brush, ColorStop, LinearGradient, RadialGradient},
    style::{Styleable, StyleableExt},
};

const BLUE: UiColor = UiColor::from_rgba(37, 58, 94, 255);
const CANVAS: UiColor = UiColor::BLACK;
const GREEN: UiColor = UiColor::from_rgba(46, 82, 63, 255);
const HIGHLIGHT: UiColor = UiColor::from_rgba(100, 135, 180, 255);
const INK: UiColor = UiColor::WHITE;
const PLUM: UiColor = UiColor::from_rgba(88, 52, 84, 255);
const SURFACE: UiColor = UiColor::from_rgba(25, 30, 40, 255);

fn swatch(title: &'static str, fill: impl Into<Brush>) -> Node {
    flexbox()
        .flex_column()
        .flex_1()
        .margin((8.,))
        .children((
            flexbox()
                .height(140.)
                .background(fill.into())
                .margin_bottom(12.),
            text(title).font_size(18.).line_height(26.).color(INK),
        ))
        .into_node()
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - color".into(),
            window_width: 880,
            window_height: 660,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let linear = LinearGradient::new(
        vec2(0., 0.),
        vec2(1., 0.),
        vec![
            ColorStop::new(0., BLUE),
            ColorStop::new(0.5, HIGHLIGHT),
            ColorStop::new(1., PLUM),
        ],
    );
    let diagonal = LinearGradient::new(
        vec2(0., 0.),
        vec2(1., 1.),
        vec![ColorStop::new(0., GREEN), ColorStop::new(1., HIGHLIGHT)],
    );
    let radial = RadialGradient::new(
        vec2(0.5, 0.5),
        0.65,
        vec![
            ColorStop::new(0., HIGHLIGHT),
            ColorStop::new(0.5, BLUE),
            ColorStop::new(1., SURFACE),
        ],
    );
    let mut app = App::new(
        flexbox()
            .size_full()
            .flex_column()
            .padding((24.,))
            .children((
                text("Fills and gradients")
                    .font_size(34.)
                    .line_height(44.)
                    .margin((8.,)),
                flexbox().flex_row().children((
                    swatch("Blue", BLUE),
                    swatch("Green", GREEN),
                    swatch("Plum", PLUM),
                )),
                flexbox().flex_row().children((
                    swatch("Linear · three stops", linear.clone()),
                    swatch("Diagonal", diagonal),
                    swatch("Radial", radial),
                )),
                text("Color is a fill, too.")
                    .font_size(40.)
                    .line_height(52.)
                    .color(linear)
                    .margin((8.,)),
            )),
    );
    loop {
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
