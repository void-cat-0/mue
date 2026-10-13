//! Signals, keyed children, fonts, and images in a small view.
use macroquad::prelude::{Conf, Texture2D, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::Animation,
    color::Color,
    font::{Weight, load_font},
    node::*,
    shader::SharedTexture,
    style::{Styleable, StyleableExt},
};

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - basics".into(),
            window_width: 800,
            window_height: 620,
            ..Default::default()
        },
        run(),
    );
}
async fn run() {
    // Vendored Space Mono fonts use SIL OFL 1.1; see assets/fonts.
    let mono =
        load_font(include_bytes!("../assets/fonts/SpaceMono-Regular.ttf").to_vec())[0].clone();
    load_font(include_bytes!("../assets/fonts/SpaceMono-Bold.ttf").to_vec());
    let texture: SharedTexture =
        Texture2D::from_file_with_format(include_bytes!("../assets/ferris.png"), None).into();
    let count = signal(3usize);
    let large = signal(false);
    // Keys preserve each image node when the list grows or shrinks.
    let images = map_keyed(
        count.map(|n| (0..n).collect::<std::rc::Rc<[usize]>>()),
        |n| *n,
        move |_| {
            image(texture.clone())
                .width(large.select(160., 100.))
                .height(120.)
                .object_contain()
                .animation(Animation::spring(0.6, 0.3))
        },
    );
    let mut app = App::new(
        scroll_area().size_full().children(
            container().padding((32.,)).gap(22.).children((
                heading("Hello 🐦🚀"),
                text("Space Mono · 400")
                    .font_family(mono.clone())
                    .font_size(20.)
                    .line_height(28.),
                text("Space Mono · 700")
                    .font_family(mono)
                    .font_weight(Weight::BOLD)
                    .font_size(20.)
                    .line_height(28.),
                hstack().children((
                    link("Add")
                        .font_size(20.)
                        .on_activate(move || count.update(|n| *n = (*n + 1).min(12))),
                    link("Remove")
                        .font_size(20.)
                        .on_activate(move || count.update(|n| *n = n.saturating_sub(1))),
                    link("Resize").font_size(20.).toggle_on_tap(large),
                )),
                separator(),
                wrap().gap(12.).children(images),
                center()
                    .height(90.)
                    .children(
                        circle()
                            .width(70.)
                            .height(70.)
                            .background(Color::from_rgba(80, 130, 220, 255)),
                    )
                    .show_if(count.map(|n| n > 0)),
            )),
        ),
    );
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        next_frame().await;
    }
}
