//! Vulkan-only shadow stress test. Select a policy, then replay the same scroll.
//!
//! Compare runs on the same device, viewport, and refresh setting. The replay
//! drives offsets by rendered-frame index (not wall time), warms the selected
//! policy at the top, and then reports the mean interval across a fixed path.
//! The result includes presentation pacing; a refresh cap can mask a GPU gain.

use std::{rc::Rc, sync::Arc, time::Instant};

use macroquad::window::{
    Conf, clear_background, get_internal_gl, next_frame, screen_dpi_scale, screen_height,
    screen_width,
};
use miniquad::conf::{Platform, RenderingBackend};
use mue_components::{ScrollOptions, flexbox, hstack, scroll_area, text, wrap};
use mue_core::prelude::{Access, Prop, ReadSignal, Signal, computed, signal};
use mue_macroquad::{
    App, Shadow, ShadowCache,
    color::Color,
    font::{self, fontdb},
    math::Vector,
    shader::Brush,
    shadow::{ShadowMaskCacheStats, clear_shadow_mask_cache, shadow_mask_cache_stats},
    style::{Style, Styleable, StyleableExt},
};

const POLICIES: [(ShadowCache, &str); 3] = [
    (ShadowCache::Auto, "Auto"),
    (ShadowCache::Enabled, "Enabled (forced)"),
    (ShadowCache::Disabled, "Disabled (analytic)"),
];
const ROWS: usize = 24;
const COLUMNS: usize = 3;
const CARD_WIDTH: f32 = 116.;
const CARD_HEIGHT: f32 = 60.;
const CARD_RADIUS: f32 = 14.;
const CARD_GAP: f32 = 16.;
const PANEL_PADDING: f32 = 24.;
const MAX_PANEL_HEIGHT: f32 = 1200.;
const WARMUP_FRAMES: u32 = 90;
const SAMPLE_FRAMES: u32 = 480;
const SAMPLE_LAPS: u32 = 2;

const BG: Color = Color::from_rgba(217, 227, 241, 255);
const PANEL: Color = Color::from_rgba(16, 29, 49, 255);
const PICKED: Color = Color::from_rgba(24, 88, 185, 255);
const CONTROL: Color = Color::from_rgba(39, 55, 77, 255);
const MUTED: Color = Color::from_rgba(188, 207, 226, 255);
const CARD_COLORS: [Color; 4] = [
    Color::from_rgba(249, 252, 255, 255),
    Color::from_rgba(207, 227, 251, 255),
    Color::from_rgba(204, 233, 228, 255),
    Color::from_rgba(232, 226, 249, 255),
];

#[derive(Clone, Copy, PartialEq)]
struct Viewport {
    width: f32,
    height: f32,
    dpi: f32,
}

impl Viewport {
    fn current() -> Self {
        Self {
            width: screen_width(),
            height: screen_height(),
            dpi: screen_dpi_scale(),
        }
    }

    fn ui(self) -> f32 {
        (self.width.min(self.height) / 480.).clamp(0.5, 3.)
    }

    fn cards(self) -> f32 {
        // Stay within the 2x mask budget on high-resolution phones as well.
        self.ui().min(2.1 / self.dpi.max(1.))
    }

    fn travel(self) -> f32 {
        let row = CARD_HEIGHT + CARD_GAP;
        let content = (ROWS as f32 * row - CARD_GAP + 2. * PANEL_PADDING) * self.cards();
        // The actual scroll viewport is shorter than the whole window.
        let available = self.height.min(MAX_PANEL_HEIGHT * self.cards());
        (content - available).clamp(0., 9. * row * self.cards())
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Live {
    fps: Option<f64>,
    hits: u64,
    bakes: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Manual,
    Warmup,
    Measuring,
    Finished,
    Interrupted,
}

struct Replay {
    frame: u32,
    distance: f32,
    sampled_at: Instant,
}

impl Replay {
    fn offset(&self) -> Vector {
        let step = self.frame.saturating_sub(WARMUP_FRAMES);
        let period = SAMPLE_FRAMES / SAMPLE_LAPS;
        let t = (step % period) as f32 / period as f32;
        Vector::new(0., self.distance * (1. - (2. * t - 1.).abs()))
    }
}

pub(crate) fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Vulkan shadow probe".into(),
            window_width: 960,
            window_height: 760,
            platform: Platform {
                // Do not use the shared Vulkan-or-GL chooser for this comparison.
                rendering_backend: RenderingBackend::Vulkan,
                ..Default::default()
            },
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    // Miniquad rejects Vulkan-only startup on unsupported builds/platforms.
    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    assert_eq!(
        unsafe { get_internal_gl() }.quad_context.info().backend,
        miniquad::Backend::Vulkan
    );

    // One existing 97 KiB face, not the playground's multilingual bundle.
    // SIL OFL 1.1 notice: examples/basics/assets/fonts/OFL-SpaceMono.txt.
    let bytes = include_bytes!("../../basics/assets/fonts/SpaceMono-Regular.ttf");
    let mut db = fontdb::Database::new();
    assert!(
        !db.load_font_source(fontdb::Source::Binary(Arc::new(&bytes[..])))
            .is_empty()
    );
    db.set_sans_serif_family("Space Mono");
    font::initialize("en-US", db).expect("initialize the probe's bundled font");

    let mut viewport = Viewport::current();
    let layout = signal(viewport);
    let selected = signal(ShadowCache::Auto);
    let phase = signal(Phase::Manual);
    let replay_toggle = signal(false);
    let offset = signal(Vector::ZERO);
    let live = signal(Live::default());
    let results = signal([None::<f64>; 3]);
    let mut app = App::new(view(
        layout,
        selected,
        phase,
        replay_toggle,
        offset,
        live,
        results,
    ));

    let mut active_policy = selected.get();
    let mut replay: Option<Replay> = None;
    let mut baseline = shadow_mask_cache_stats();
    let mut last_frame = Instant::now();
    let mut sampled_at = last_frame;
    let mut frames = 0u32;
    loop {
        let now = Instant::now();
        let interrupted = now.duration_since(last_frame).as_secs_f64() > 1.;
        last_frame = now;
        let next_viewport = Viewport::current();
        clear_background(BG.into());
        app.frame_with_update(|| {
            let resized = next_viewport != viewport;
            if resized {
                viewport = next_viewport;
                layout.set(viewport);
                results.set([None; 3]);
            }
            let policy = selected.get();
            let changed = policy != active_policy;
            let toggle = replay_toggle.get();
            replay_toggle.set(false);
            if resized || interrupted || changed || toggle {
                let was_replaying = replay.take().is_some();
                let start = toggle && !was_replaying;
                if changed || start {
                    // Start from a cold pool even if another policy just ran.
                    // This is before App drawing, never in the sampled interval.
                    clear_shadow_mask_cache();
                    baseline = shadow_mask_cache_stats();
                }
                if start {
                    replay = Some(Replay {
                        frame: 0,
                        distance: viewport.travel(),
                        sampled_at: now,
                    });
                    phase.set(Phase::Warmup);
                } else if was_replaying && (resized || interrupted) {
                    phase.set(Phase::Interrupted);
                } else {
                    phase.set(Phase::Manual);
                }
                active_policy = policy;
                sampled_at = now;
                frames = 0;
                live.set(live_stats(None, baseline));
            }

            if let Some(run) = &mut replay {
                if run.frame == WARMUP_FRAMES {
                    run.sampled_at = now;
                    phase.set(Phase::Measuring);
                }
                if run.frame == WARMUP_FRAMES + SAMPLE_FRAMES {
                    let mean_ms = now.duration_since(run.sampled_at).as_secs_f64() * 1000.
                        / SAMPLE_FRAMES as f64;
                    let index = POLICIES
                        .iter()
                        .position(|(mode, _)| *mode == policy)
                        .unwrap();
                    results.update(|values| values[index] = Some(mean_ms));
                    phase.set(Phase::Finished);
                    offset.set(Vector::ZERO);
                    replay = None;
                } else {
                    // Apply after input dispatch, so manual wheel/touch cannot alter the replay path.
                    offset.set(run.offset());
                    run.frame += 1;
                }
            }

            let elapsed = now.duration_since(sampled_at).as_secs_f64();
            if elapsed >= 0.5 {
                live.set(live_stats(Some(frames as f64 / elapsed), baseline));
                sampled_at = now;
                frames = 0;
            }
        });
        next_frame().await;
        frames += 1;
    }
}

fn live_stats(fps: Option<f64>, baseline: ShadowMaskCacheStats) -> Live {
    let stats = shadow_mask_cache_stats();
    Live {
        fps,
        hits: stats.hits.saturating_sub(baseline.hits),
        bakes: stats.bakes.saturating_sub(baseline.bakes),
    }
}

fn label(
    caption: impl Into<Prop<Rc<str>>>,
    size: f32,
    ui: ReadSignal<f32>,
) -> impl mue_components::Component {
    text(caption)
        .font_size(ui.map(move |s| size * s))
        .line_height(ui.map(move |s| (size + 5.) * s))
        .text_wrap(false)
        .flex_none()
}

fn control(
    caption: impl Into<Prop<Rc<str>>> + 'static,
    ui: ReadSignal<f32>,
    background: impl Into<Prop<Brush>>,
    on_tap: impl FnMut() + 'static,
) -> impl mue_components::Component {
    flexbox()
        .flex_none()
        .items_center()
        .justify_center()
        .min_height(ui.map(|s| 44. * s))
        .padding_x(ui.map(|s| 10. * s))
        .radius(ui.map(|s| 8. * s))
        .background(background)
        .on_activate(on_tap)
        .children(label(caption, 14., ui))
}

fn view(
    dims: Signal<Viewport>,
    selected: Signal<ShadowCache>,
    phase: Signal<Phase>,
    replay_toggle: Signal<bool>,
    offset: Signal<Vector>,
    live: Signal<Live>,
    results: Signal<[Option<f64>; 3]>,
) -> impl mue_components::Component {
    let ui = dims.map(Viewport::ui);
    let cards = dims.map(Viewport::cards);
    let gap = ui.map(|s| 8. * s);
    let card_gap = cards.map(|s| CARD_GAP * s);
    let card_width = cards.map(|s| CARD_WIDTH * s);
    let card_height = cards.map(|s| CARD_HEIGHT * s);
    let card_radius = cards.map(|s| CARD_RADIUS * s);
    let shadow = cards
        .map(|s| Shadow::new(22. * s, CARD_RADIUS * s).color(Color::from_rgba(15, 27, 56, 184)));

    let modes = POLICIES.map(|(mode, name)| {
        control(
            name,
            ui,
            computed(move |_| {
                Brush::Solid(if selected.get() == mode {
                    PICKED
                } else {
                    CONTROL
                })
            }),
            move || selected.set(mode),
        )
    });
    let replay_label = phase.map(|phase| match phase {
        Phase::Warmup | Phase::Measuring => "Stop replay",
        _ => "Replay scroll",
    });
    let phase_label = phase.map(|phase| match phase {
        Phase::Manual => "Drag or wheel to scroll",
        Phase::Warmup => "Warming up...",
        Phase::Measuring => "Measuring 2 round trips...",
        Phase::Finished => "Done. Select another mode and replay.",
        Phase::Interrupted => "View changed. Replay to measure.",
    });
    let score = results.map(|values| {
        let ms = |i: usize| values[i].map_or("--".into(), |ms| format!("{ms:.1}"));
        format!("Mean ms: Auto {} | On {} | Off {}", ms(0), ms(1), ms(2))
    });
    let delta = results.map(|values| match (values[1], values[2]) {
        (Some(on), Some(off)) if off > 0. => {
            format!(
                "Forced vs analytic: {:+.1}% less frame time",
                (off - on) / off * 100.
            )
        }
        _ => "Replay Enabled and Disabled for A/B".into(),
    });

    let rows = (0..ROWS)
        .map(|row| {
            let children = (0..COLUMNS)
                .map(|column| {
                    flexbox()
                        .flex_none()
                        .width(card_width)
                        .height(card_height)
                        .background(CARD_COLORS[(row + column) % CARD_COLORS.len()])
                        .radius(card_radius)
                        .shadow(shadow)
                        .shadow_cache_policy(selected)
                })
                .collect::<Vec<_>>();
            hstack().flex_none().gap(card_gap).children(children)
        })
        .collect::<Vec<_>>();

    flexbox()
        .size_full()
        .flex_column()
        .background(BG)
        .children((
            flexbox()
                .w_full()
                .flex_column()
                .flex_none()
                .background(PANEL)
                .padding((ui.map(|s| 12. * s),))
                .gap(ui.map(|s| 5. * s))
                .children((
                    hstack().justify_between().children((
                        label("Vulkan shadow probe", 20., ui),
                        label(
                            live.map(|live| match live.fps {
                                Some(fps) => format!("{fps:.1} FPS"),
                                None => "-- FPS".into(),
                            }),
                            15.,
                            ui,
                        ),
                    )),
                    wrap().gap(gap).children(modes),
                    wrap().gap(gap).children((
                        control(replay_label, ui, CONTROL, move || replay_toggle.set(true)),
                        label(phase_label, 12., ui).color(MUTED),
                    )),
                    label(
                        live.map(|live| {
                            format!("Cache hits: {} | Bakes: {}", live.hits, live.bakes)
                        }),
                        13.,
                        ui,
                    )
                    .color(MUTED),
                    label(score, 13., ui).color(MUTED),
                    label(delta, 13., ui).color(MUTED),
                )),
            scroll_area()
                .w_full()
                .flex_1()
                .flex_basis(0.)
                .min_height(0.)
                .max_height(cards.map(|s| MAX_PANEL_HEIGHT * s))
                .offset(offset)
                .options(ScrollOptions {
                    mouse_drag: true,
                    momentum: false,
                    ..ScrollOptions::default()
                })
                .content_style(
                    Style::new()
                        .items_center()
                        .gap(card_gap)
                        .padding_y(cards.map(|s| PANEL_PADDING * s)),
                )
                .children(rows),
        ))
}
