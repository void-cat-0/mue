#[path = "../../palette.rs"]
mod palette;

use std::path::Path;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    font::{FamilyOwned, Weight, load_font_file},
    math::{Matrix, Vector},
    node::*,
    shader::SharedTexture,
    style::{Styleable, StyleableExt},
};
use taffy::Dimension;

/// Vendored fonts (SIL OFL 1.1, see `assets/fonts/OFL-SpaceMono.txt`).
const FONTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts");

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - basics".into(),
            window_width: 973,
            window_height: 608,
            ..Default::default()
        },
        async {
            run().await;
        },
    );
}

#[mue_macros::component]
fn view(texture: SharedTexture, time: f32, mono: FamilyOwned) -> impl Component {
    let sprites =
        map_keyed(
            computed(move |_| {
                let time = time.get();
                let count = ((time * 2.) as usize + 1).min(10);
                (0..count).collect()
            }),
            |&value| value,
            move |&i| {
                image(texture.clone())
                    .min_width(0.)
                    .height(time.map(move |t: f32| {
                        Dimension::percent((t * (i + 1) as f32).sin() * 0.5 + 0.5)
                    }))
                    .flex_grow(1.)
                    .margin((5.,))
            },
        );

    let row = flexbox()
        .children(sprites)
        .flex_row()
        .w_full()
        .h_auto()
        .flex_1()
        .items_center();

    let mono_line = |label: &'static str| {
        text(label)
            .font_family(mono.clone())
            .font_size(20.)
            .line_height(26.)
    };

    flexbox()
        .children((
            text("hello 🐦🚀"),
            // 默认族是系统无衬线（SansSerif），这两行换成注册进来的 Space Mono
            mono_line("Space Mono · 400"),
            // 同族的 Bold 字面在 run 里也注册了，所以这里能命中它
            mono_line("Space Mono · 700").font_weight(Weight::BOLD),
            // 字重是响应式的
            mono_line("weight follows the clock").font_weight(computed(move |_| {
                if (time.get() as usize) % 2 == 0 {
                    Weight::NORMAL
                } else {
                    Weight::BOLD
                }
            })),
            row,
            circle()
                .color(palette::BLUE)
                .h_auto()
                .flex_1()
                .transform(time.map(|t| {
                    Matrix::from_translation(Vector::new(
                        (t * 20.).cos() * 20.,
                        (t * 20.).sin() * 20.,
                    ))
                }))
                .opacity(time.map(|t| t.sin() * 0.5 + 0.5))
                .show_if(time.map(|t| t >= 0.2)),
        ))
        .flex_column()
}

async fn run() {
    let time = signal(0.);
    let texture: SharedTexture = Texture2D::from_file_with_format(
        include_bytes!("../assets/ferris.png"),
        Some(ImageFormat::Png),
    )
    .into();

    // 字体路径用 CARGO_MANIFEST_DIR 解析，不受工作目录影响。
    let fonts = Path::new(FONTS);
    let mono = load_family(&fonts.join("SpaceMono-Regular.ttf"));
    // 这里返回的族名和上面一样（都是 "Space Mono"），只需要把 Bold 字面注册进去，
    // 之后 font_weight(Weight::BOLD) 才能命中它。
    let _bold = load_family(&fonts.join("SpaceMono-Bold.ttf"));

    let root = view(texture, *time, mono).size_full();
    let mut app = App::new(root);

    loop {
        clear_background(palette::CANVAS.into());

        time.set(get_time() as f32);

        app.frame();

        next_frame().await;
    }
}

fn load_family(path: &Path) -> FamilyOwned {
    load_font_file(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{} does not contain a font face", path.display()))
}
