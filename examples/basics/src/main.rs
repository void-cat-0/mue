//! Signals, keyed children, fonts, and images in a small view.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;
use macroquad::prelude::{Conf, Texture2D, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    font::{Weight, load_font},
    shader::SharedTexture,
    style::{Size, Styleable, StyleableExt},
    transition::Transition,
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
                .style(mue_macroquad::style! {
                width: {large.select(Size::vw(20.), Size::vw(12.5))};
                max-width: {responsive::ui(large.select(160., 100.))};
                height: {Size::vh(20.)};
                max-height: {responsive::ui(120.)};
                flex: none;
                })
                .object_contain()
                .style(mue_macroquad::style! { transition: {Transition::spring(0.6, 0.3)}; })
        },
    );
    responsive::sync();
    let mut app = App::new(
        scroll_area()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            })
            .children(
                container()
                    .style(mue_macroquad::style! {
                    max-width: {responsive::ui(800.)};
                    padding: {(Size::vw(4.),)};
                    gap: {responsive::ui(22.)};
                    })
                    .children((
                        heading("Hello 🐦🚀").style(mue_macroquad::style! {
                        font-size: {responsive::font(32. * 1.45)};
                        line-height: {responsive::ui(44. * 1.45)};
                        }),
                        text("Space Mono · 400").style(mue_macroquad::style! {
                        font-family: {mono.clone()};
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(28.)};
                        }),
                        text("Space Mono · 700").style(mue_macroquad::style! {
                        font-family: {mono};
                        font-weight: {Weight::BOLD};
                        font-size: {responsive::font(20.)};
                        line-height: {responsive::ui(28.)};
                        }),
                        wrap().children((
                            link("Add")
                                .style(mue_macroquad::style! {
                                font-size: {responsive::font(20.)};
                                line-height: {responsive::ui(44.)};
                                min-width: {responsive::ui(44.)};
                                min-height: {responsive::ui(44.)};
                                })
                                .on_activate(move || count.update(|n| *n = (*n + 1).min(12))),
                            link("Remove")
                                .style(mue_macroquad::style! {
                                font-size: {responsive::font(20.)};
                                line-height: {responsive::ui(44.)};
                                min-width: {responsive::ui(44.)};
                                min-height: {responsive::ui(44.)};
                                })
                                .on_activate(move || count.update(|n| *n = n.saturating_sub(1))),
                            link("Resize")
                                .style(mue_macroquad::style! {
                                font-size: {responsive::font(20.)};
                                line-height: {responsive::ui(44.)};
                                min-width: {responsive::ui(44.)};
                                min-height: {responsive::ui(44.)};
                                })
                                .toggle_on_tap(large),
                        )),
                        separator(),
                        wrap()
                            .style(mue_macroquad::style! { gap: {responsive::ui(12.)}; })
                            .children(images),
                        center()
                            .style(mue_macroquad::style! {
                            height: {Size::vh(15.)};
                            max-height: {responsive::ui(90.)};
                            })
                            .children(circle().style(mue_macroquad::style! {
                            width: {Size::vw(8.75)};
                            height: {Size::vw(8.75)};
                            max-width: {responsive::ui(70.)};
                            max-height: {responsive::ui(70.)};
                            background: {Color::from_rgba(80, 130, 220, 255)};
                            }))
                            .show_if(count.map(|n| n > 0)),
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
