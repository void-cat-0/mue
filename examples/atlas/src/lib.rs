//! Glyph atlas stress test with deterministic embedded fonts.
//!
//! Interactive mode keeps the original controls and triangle-wave animation:
//! `cargo run -p atlas`. Finite checks are useful for CI, for example:
//! `--glyphs 4000 --max-size 28 --frames 120 --verify`.
//!
//! The default maximum size is intentionally 56px. At that size, 4000 CJK
//! glyphs are an intended overload case, not a bounded verification budget;
//! use the command above to exercise the 8-page/32MiB budget without overload.
use mue_components::*;

#[path = "../../shared/responsive.rs"]
mod responsive;

use std::{env, sync::Arc, time::Instant};

use macroquad::{
    color::BLACK,
    prelude::{clear_background, next_frame},
    texture::Image,
};
use mue_core::{prelude::*, prop::Prop};
use mue_macroquad::{
    App,
    font::{self, GlyphAtlasStats, glyph_atlas_stats},
    style::{FontSize, Size, Style, Styleable, StyleableExt},
};

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog. 你好，世界。";
const DEFAULT_GLYPHS: u32 = 600;
const DEFAULT_MAX_SIZE: f32 = 56.0;
const DEFAULT_FRAMES: u32 = 0;

const LATIN: &[u8] = include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf");
const CJK: &[u8] = include_bytes!("../../playground/assets/fonts/NotoSansCJKsc-Regular.otf");

const FONT_NOTICE: &str = "ATLAS EMBEDDED FONT NOTICES\n\nNoto Sans 2.015 Regular\nCopyright 2022 The Noto Project Authors\nhttps://github.com/notofonts/latin-greek-cyrillic\nLicense: SIL Open Font License, Version 1.1 (OFL-NotoLatinGreekCyrillic.txt)\n\nNoto Sans CJK SC 2.004 Regular\n© 2014-2021 Adobe (http://www.adobe.com/).\nhttps://github.com/notofonts/noto-cjk\nLicense: SIL Open Font License, Version 1.1 (OFL-NotoCJK.txt)";
const LICENSE_LATIN: &str =
    include_str!("../../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt");
const LICENSE_CJK: &str = include_str!("../../playground/assets/fonts/OFL-NotoCJK.txt");

#[derive(Clone, Copy, Debug)]
pub struct Options {
    frames: u32,
    glyphs: u32,
    max_size: f32,
    verify: bool,
    verify_upload: bool,
    licenses: bool,
}

fn usage() {
    println!(
        "atlas options:\n  --frames N       finite run (default: interactive)\n  --glyphs N       CJK glyph count (default: 600)\n  --max-size FLOAT maximum animated font size (default: 56)\n  --verify [BOOL]  fail if atlas rejects/exhausts/rasterizes a glyph\n  --verify-upload  check first uploaded text frame against the next frame\n  --licenses       print embedded font notices and OFL licenses\n  -h, --help       show this help"
    );
}

impl Default for Options {
    fn default() -> Self {
        Self {
            frames: DEFAULT_FRAMES,
            glyphs: DEFAULT_GLYPHS,
            max_size: DEFAULT_MAX_SIZE,
            verify: false,
            verify_upload: false,
            licenses: false,
        }
    }
}

impl Options {
    /// Print embedded font licenses and report whether the app should exit.
    pub fn print_licenses_if_requested(&self) -> bool {
        if self.licenses {
            print_licenses();
        }
        self.licenses
    }
}

pub fn options() -> Options {
    let mut result = Options::default();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let value = |name: &str, i: &mut usize| -> String {
            *i += 1;
            args.get(*i)
                .unwrap_or_else(|| panic!("{name} needs a value"))
                .clone()
        };
        match arg {
            "--frames" => result.frames = value(arg, &mut i).parse().expect("invalid --frames"),
            "--glyphs" => result.glyphs = value(arg, &mut i).parse().expect("invalid --glyphs"),
            "--max-size" => {
                result.max_size = value(arg, &mut i).parse().expect("invalid --max-size")
            }
            "--verify" => {
                // Accept both `--verify` and the explicit `--verify true|false`
                // form used by scripted finite checks.
                if let Some(next) = args.get(i + 1).map(String::as_str) {
                    if matches!(next, "true" | "false") {
                        i += 1;
                        result.verify = next == "true";
                    } else {
                        result.verify = true;
                    }
                } else {
                    result.verify = true;
                }
            }
            "--verify-upload" => result.verify_upload = true,
            "--licenses" => result.licenses = true,
            "-h" | "--help" => {
                usage();
                std::process::exit(0);
            }
            _ => panic!("unknown option {arg}; use --help"),
        }
        i += 1;
    }
    assert!(
        result.max_size.is_finite() && result.max_size > 0.,
        "--max-size must be positive"
    );
    if result.frames == 0 {
        if result.verify_upload {
            result.frames = 4;
        } else if result.verify {
            result.frames = 120;
        }
    }
    result
}

fn database() -> font::fontdb::Database {
    let mut db = font::fontdb::Database::new();
    for bytes in [LATIN, CJK] {
        let ids = db.load_font_source(font::fontdb::Source::Binary(Arc::new(bytes)));
        assert_eq!(ids.len(), 1, "invalid embedded font");
    }
    // Explicit generic-family mappings avoid whichever system fonts happen to be installed.
    db.set_sans_serif_family("Noto Sans");
    db.set_serif_family("Noto Sans");
    db.set_monospace_family("Noto Sans");
    db.set_cursive_family("Noto Sans");
    db.set_fantasy_family("Noto Sans");
    db
}

fn print_licenses() {
    println!("{FONT_NOTICE}\n\n{LICENSE_LATIN}\n\n{LICENSE_CJK}");
}

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

/// A deliberate post-text draw. It forces a queued text batch through the GL
/// command stream before the framebuffer is captured by `get_screen_data`.
fn flush_node() -> Node {
    Node::build_with_style(Style::default(), |style| {
        style
            .on_render
            .append(|_| unsafe { macroquad::window::get_internal_gl() }.flush());
    })
}

fn has_nonblack_glyph_pixels(image: &Image) -> bool {
    image
        .bytes
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
}

fn same_image(a: &Image, b: &Image) -> bool {
    a.width == b.width && a.height == b.height && a.bytes == b.bytes
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - glyph atlas stress".into(),
            ..Default::default()
        },
        run(Options::default()),
    );
}

pub async fn run(options: Options) {
    // Must happen before any text node or other font access. Only these two
    // embedded faces are loaded; no host font database is consulted.
    font::initialize("zh-CN", database()).expect("initialize embedded fonts before nodes");
    responsive::sync();

    let count = signal(options.glyphs);
    let size = signal(28.0f32);
    let stats = signal(String::from("atlas: waiting for the first frame"));
    let body = count.map(|n| format!("{}\n\n{SAMPLE}", cjk_sweep(n)));
    let size_prop: Prop<f32> = size.into();
    let font_size: Prop<FontSize> = size_prop.map(FontSize::Px);
    let line_height: Prop<Size> = size_prop.map(|value| Size::Px(value * 1.35));
    let stats_text = stats.map(|value| value.clone());

    let content = scroll_area()
        .style(mue_macroquad::style! { width: 100%; height: 100%; })
        .children(
            container()
                .style(mue_macroquad::style! {
                max-width: {responsive::ui(860.)};
                padding: {(responsive::ui(24.),)};
                gap: {responsive::ui(14.)};
                })
                .children((
                    heading("Glyph atlas stress").style(mue_macroquad::style! {
                    font-size: {responsive::font(30.)};
                    line-height: {responsive::ui(40.)};
                    }),
                    text(stats_text).style(mue_macroquad::style! {
                    font-size: {responsive::font(15.)};
                    line-height: {responsive::ui(22.)};
                    text-wrap: {false};
                    }),
                    wrap()
                        .style(mue_macroquad::style! { gap: {responsive::ui(12.)}; })
                        .children((
                            link("600 glyphs")
                                .style(
                                    mue_macroquad::style! { font-size: {responsive::font(18.)}; },
                                )
                                .style(
                                    mue_macroquad::style! { line-height: {responsive::ui(32.)}; },
                                )
                                .on_activate(move || count.set(600)),
                            link("2000 glyphs")
                                .style(
                                    mue_macroquad::style! { font-size: {responsive::font(18.)}; },
                                )
                                .style(
                                    mue_macroquad::style! { line-height: {responsive::ui(32.)}; },
                                )
                                .on_activate(move || count.set(2000)),
                            link("4000 glyphs")
                                .style(
                                    mue_macroquad::style! { font-size: {responsive::font(18.)}; },
                                )
                                .style(
                                    mue_macroquad::style! { line-height: {responsive::ui(32.)}; },
                                )
                                .on_activate(move || count.set(4000)),
                        )),
                    text(body).style(mue_macroquad::style! {
                    font-size: {font_size};
                    line-height: {line_height};
                    }),
                )),
        );
    let root = if options.verify_upload {
        // Keep the flush node after text in the same stack, exercising the
        // upload/geometry ordering before get_screen_data reads the framebuffer.
        stack()
            .style(mue_macroquad::style! { width: 100%; height: 100%; })
            .children((content, flush_node()))
    } else {
        stack()
            .style(mue_macroquad::style! { width: 100%; height: 100%; })
            .children((content,))
    };
    let mut app =
        App::new(root.style(mue_macroquad::style! { font-size: {responsive::root_font()}; }));
    if options.verify_upload && options.frames != 0 {
        assert!(
            options.frames >= 2 && options.frames.is_multiple_of(2),
            "--verify-upload needs an even number of frames, at least two"
        );
    }

    let verbose = env::var_os("ATLAS_STATS").is_some() || options.frames != 0;
    let start = Instant::now();
    let mut last_report = 0.0;
    let mut first_image: Option<Image> = None;
    let mut frame = 0;
    loop {
        responsive::sync();
        // Preserve the old triangle animation while allowing finite runs to
        // reach their requested max size deterministically.
        let ramp = if options.verify_upload {
            if (frame / 2) % 2 == 0 { 1.0 } else { 0.37 }
        } else if options.frames == 0 {
            let phase = (macroquad::time::get_time() * 0.5).fract();
            if phase < 0.5 {
                phase * 2.0
            } else {
                (1.0 - phase) * 2.0
            }
        } else if options.frames <= 1 {
            1.0
        } else {
            frame as f64 / (options.frames - 1) as f64
        };
        let min_size = options.max_size.min(14.0);
        size.set(min_size + (options.max_size - min_size) * ramp as f32);

        clear_background(BLACK);
        app.frame();
        let snapshot = glyph_atlas_stats().unwrap_or_default();
        let line = stats_line(snapshot);
        let elapsed = start.elapsed().as_secs_f64();
        if verbose && (elapsed - last_report >= 1.0 || options.frames != 0) {
            last_report = elapsed;
            println!("frame {frame}: {line}");
        }
        if !options.verify_upload {
            stats.set(line.clone());
        }

        if options.verify_upload {
            let image = macroquad::texture::get_screen_data();
            if frame % 2 == 0 {
                assert!(
                    has_nonblack_glyph_pixels(&image),
                    "first uploaded text frame is black"
                );
                first_image = Some(image);
            } else {
                assert!(
                    same_image(first_image.as_ref().unwrap(), &image),
                    "cold-cache text frame differs from its next frame"
                );
            }
        }

        frame += 1;
        if options.frames != 0 && frame >= options.frames {
            let final_stats = glyph_atlas_stats().unwrap_or_default();
            println!("final: {line}");
            println!(
                "timing: {} frames in {:.3}s ({:.2} fps)",
                frame,
                start.elapsed().as_secs_f64(),
                frame as f64 / start.elapsed().as_secs_f64().max(f64::MIN_POSITIVE)
            );
            if options.verify {
                assert_eq!(
                    final_stats.rejected_oversized, 0,
                    "oversized glyphs rejected"
                );
                assert_eq!(final_stats.rejected_exhausted, 0, "atlas slots exhausted");
                assert_eq!(
                    final_stats.rejected_raster, 0,
                    "glyph rasterization rejected"
                );
            }
            return;
        }
        next_frame().await;
    }
}
