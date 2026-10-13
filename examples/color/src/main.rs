//! Shared brushes, image fitting, and an animated GPU material.
#[path = "../../shared/halftone.rs"]
mod halftone;
use macroquad::prelude::{Conf, Texture2D, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::{Animation, animated_with},
    color::Color,
    math::vec2,
    node::*,
    shader::{Brush, ColorStop, LinearGradient, RadialGradient, SharedTexture},
    style::{Style, Styleable, StyleableExt},
};

fn swatch(label: &str, brush: impl Into<Brush>) -> Node {
    stack()
        .width(220.)
        .gap(8.)
        .children((
            center().height(100.).background(brush.into()).radius(12.),
            text(label).font_size(16.).line_height(24.),
        ))
        .into_node()
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - color".into(),
            window_width: 920,
            window_height: 760,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    let blue = Color::from_rgba(64, 99, 181, 255);
    let coral = Color::from_rgba(249, 168, 107, 255);
    let linear = LinearGradient::new(
        vec2(0., 0.),
        vec2(1., 1.),
        vec![ColorStop::new(0., blue), ColorStop::new(1., coral)],
    );
    let radial = RadialGradient::new(
        vec2(0.5, 0.5),
        0.7,
        vec![ColorStop::new(0., coral), ColorStop::new(1., blue)],
    );
    // Downloaded once; the example works offline. Source credit is beside the asset.
    let photo: SharedTexture =
        Texture2D::from_file_with_format(include_bytes!("../assets/photo.png"), None).into();
    let moving = signal(true);
    let dense = signal(false);
    let density = animated_with(dense.select(0.3, 0.1), Animation::spring(0.6, 0.1)).value;
    let mut app = App::new(
        scroll_area().size_full().children(
            container()
                .max_width(850.)
                .padding((24.,))
                .gap(20.)
                .children((
                    heading("Color"),
                    wrap().gap(20.).children((
                        swatch("Solid", blue),
                        swatch("Linear", linear.clone()),
                        swatch("Radial", radial),
                    )),
                    heading("Shared foreground").font_size(32.).color(linear),
                    image(photo).height(230.).w_full().object_cover(),
                    hstack().children((
                        link("Pause / play").font_size(18.).toggle_on_tap(moving),
                        link("Density").font_size(18.).toggle_on_tap(dense),
                    )),
                    halftone::halftone(
                        Style::new().height(180.).w_full().flex_none(),
                        density,
                        *moving,
                    ),
                )),
        ),
    );
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        next_frame().await;
    }
}
