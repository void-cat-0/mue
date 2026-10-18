//! Glyph atlas stress: a large CJK document whose font size changes every
//! frame, with live atlas counters. Run with `cargo run -p atlas`.
//!
//! This is the atlas policy's real-scenario check. While the size animates, a
//! new cache key appears every frame, so the atlas is pushed through page
//! growth and eviction continuously. What to watch:
//!
//! * no glyphs disappear, even at the 4000-glyph setting;
//! * `rasterized` climbs while new glyphs are seen, then settles;
//! * `evicted` only moves once the working set stops fitting;
//! * `uploads` stays well below `rasterized` when allocations are clustered;
//! * `oversized`/`exhausted` stay at zero, and no atlas warning is printed.
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    font::{GlyphAtlasStats, glyph_atlas_stats},
    node::*,
    style::{FontSize, Size, Styleable, StyleableExt},
};

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog. 你好，世界。";

/// Distinct CJK code points, so the working set can exceed one atlas page.
fn cjk_sweep(count: u32) -> String {
    (0..count)
        .filter_map(|index| char::from_u32(0x4E00 + index))
        .collect()
}

fn stats_line(stats: GlyphAtlasStats) -> String {
    format!(
        "pages {} · rasterized {} · evicted {} · uploads {} · uploaded {}px · oversized {} · exhausted {} · raster {}",
        stats.pages,
        stats.rasterized,
        stats.evicted,
        stats.uploads,
        stats.uploaded_pixels,
        stats.rejected_oversized,
        stats.rejected_exhausted,
        stats.rejected_raster,
    )
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - glyph atlas stress".into(),
            window_width: 900,
            window_height: 700,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();

    let count = signal(600u32);
    let size = signal(28.0f32);
    let stats = signal(String::from("atlas: waiting for the first frame"));

    let body = count.map(|n| format!("{}\n\n{SAMPLE}", cjk_sweep(n)));
    let size_prop: Prop<f32> = size.into();
    let font_size: Prop<FontSize> = size_prop.map(FontSize::Px);
    let line_height: Prop<Size> = size_prop.map(|value| Size::Px(value * 1.35));
    let stats_text = stats.map(|value| value.clone());

    let mut app = App::new(
        scroll_area()
            .size_full()
            .font_size(responsive::root_font())
            .children(
                container()
                    .max_width(responsive::ui(860.))
                    .padding((responsive::ui(24.),))
                    .gap(responsive::ui(14.))
                    .children((
                        heading("Glyph atlas stress")
                            .font_size(responsive::font(30.))
                            .line_height(responsive::ui(40.)),
                        text(stats_text)
                            .font_size(responsive::font(15.))
                            .line_height(responsive::ui(22.))
                            .text_wrap(false),
                        wrap().gap(responsive::ui(12.)).children((
                            link("600 glyphs")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(32.))
                                .on_activate(move || count.set(600)),
                            link("2000 glyphs")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(32.))
                                .on_activate(move || count.set(2000)),
                            link("4000 glyphs")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(32.))
                                .on_activate(move || count.set(4000)),
                        )),
                        text(body).font_size(font_size).line_height(line_height),
                    )),
            ),
    );

    // Set ATLAS_STATS=1 to also print the counters once a second, which makes
    // this example usable as a headless smoke check.
    let verbose = std::env::var_os("ATLAS_STATS").is_some();
    let mut last_report = 0.0;
    let start = macroquad::time::get_time();
    loop {
        responsive::sync();
        // A triangle wave over 14..56 px. Every frame is a new cache key, so
        // this maximises atlas churn on purpose.
        let elapsed = macroquad::time::get_time() - start;
        let phase = (elapsed * 0.5).fract();
        let ramp = (if phase < 0.5 {
            phase * 2.0
        } else {
            (1.0 - phase) * 2.0
        }) as f32;
        size.set(14.0 + 42.0 * ramp);

        clear_background(macroquad::color::BLACK);
        app.frame();
        if let Some(snapshot) = glyph_atlas_stats() {
            let line = stats_line(snapshot);
            if verbose && elapsed - last_report >= 1.0 {
                last_report = elapsed;
                println!("{line}");
            }
            stats.set(line);
        }
        next_frame().await;
    }
}
