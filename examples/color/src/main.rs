//! Shared brushes, image fitting, and an animated GPU material.
#[path = "../../shared/halftone.rs"]
mod halftone;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, Texture2D, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::{Animation, animated_with},
    color::Color,
    math::vec2,
    node::*,
    shader::{Brush, ColorStop, LinearGradient, RadialGradient, SharedTexture},
    style::{Size, Style, Styleable, StyleableExt},
};

fn swatch(label: &str, brush: impl Into<Brush>) -> Node {
    stack()
        .width(Size::percent(100.))
        .max_width(responsive::ui(220.))
        .flex_none()
        .gap(responsive::ui(8.))
        .children((
            center()
                .height(Size::vh(14.))
                .max_height(responsive::ui(100.))
                .background(brush.into())
                .radius(responsive::ui(12.)),
            text(label)
                .font_size(responsive::font(16.))
                .line_height(responsive::ui(24.)),
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
    responsive::sync();
    let mut app = App::new(
        scroll_area()
            .size_full()
            .font_size(responsive::root_font())
            .children(
                container()
                    .max_width(responsive::ui(850.))
                    .padding((Size::vw(2.5),))
                    .gap(responsive::ui(20.))
                    .children((
                        heading("Color")
                            .font_size(responsive::font(32. * 1.45))
                            .line_height(responsive::ui(44. * 1.45)),
                        wrap().gap(responsive::ui(20.)).children((
                            swatch("Solid", blue),
                            swatch("Linear", linear.clone()),
                            swatch("Radial", radial),
                        )),
                        heading("Shared foreground")
                            .font_size(responsive::font(32.))
                            .line_height(responsive::ui(44. * 1.45))
                            .color(linear),
                        image(photo)
                            .height(Size::vh(30.))
                            .max_height(responsive::ui(230.))
                            .w_full()
                            .object_cover(),
                        wrap().children((
                            link("Pause / play")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(44.))
                                .min_width(responsive::ui(44.))
                                .min_height(responsive::ui(44.))
                                .toggle_on_tap(moving),
                            link("Density")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(44.))
                                .min_width(responsive::ui(44.))
                                .min_height(responsive::ui(44.))
                                .toggle_on_tap(dense),
                        )),
                        halftone::halftone(
                            Style::new()
                                .height(Size::vh(24.))
                                .max_height(responsive::ui(180.))
                                .w_full()
                                .flex_none(),
                            density,
                            *moving,
                        ),
                    )),
            ),
    );
    loop {
        clear_background(macroquad::color::BLACK);
        responsive::sync();
        app.frame();
        next_frame().await;
    }
}
