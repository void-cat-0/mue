//! Compare input hit-testing before and after advancing a visibly slow UI.
//! The host keeps pumping events; the entire displayed image is cached between
//! UI frames. See ../README.md for the experiment and its limits.

use macroquad::{
    miniquad::{EventHandler, MouseButton, TouchPhase},
    prelude::*,
};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as UiColor,
    event::hit_test::hit_test,
    input::InputSubscriber,
    math::Vector,
    node::{IntoNode, Node, NodeRef, flexbox},
    style::Styleable,
};

const WIDTH: i32 = 1000;
const HEIGHT: i32 = 640;
const LEFT: f32 = 100.;
const RIGHT: f32 = 650.;
const TOP: f32 = 180.;
const TARGET_W: f32 = 250.;
const TARGET_H: f32 = 140.;

struct Options {
    fps: f64,
    check: bool,
    capture: Option<String>,
}

impl Options {
    fn parse() -> Self {
        let mut result = Self {
            fps: 1.,
            check: false,
            capture: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fps" => {
                    result.fps = args
                        .next()
                        .expect("--fps requires a number")
                        .parse()
                        .expect("invalid FPS");
                    assert!(
                        result.fps.is_finite() && result.fps > 0. && result.fps <= 60.,
                        "FPS must be in (0, 60]"
                    );
                }
                "--check" => result.check = true,
                "--capture" => {
                    result.capture = Some(args.next().expect("--capture requires a PNG path"))
                }
                _ => panic!("unknown argument: {arg}; use --fps N, --check, --capture PATH"),
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    A,
    B,
    Empty,
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::Empty => "empty",
        }
    }
}

struct Click {
    position: Vector,
    touch_time: Option<f64>,
    received_at: f64,
    expected: Option<(Target, Target)>,
}

#[derive(Default)]
struct Input {
    clicks: Vec<Click>,
}

impl Input {
    fn push(&mut self, x: f32, y: f32, touch_time: Option<f64>) {
        let dpi = screen_dpi_scale().max(f32::EPSILON);
        self.clicks.push(Click {
            position: Vector::new(x / dpi, y / dpi),
            touch_time,
            received_at: get_time(),
            expected: None,
        });
    }
}

impl EventHandler for Input {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.push(x, y, None);
        }
    }

    fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32, time: f64) {
        if phase == TouchPhase::Started {
            self.push(x, y, Some(time));
        }
    }
}

struct Tree {
    root: NodeRef,
    a: NodeRef,
    b: NodeRef,
}

impl Tree {
    fn hit(&self, position: Vector) -> Target {
        let path = hit_test(self.root, position);
        if path.contains(&self.a) {
            Target::A
        } else if path.contains(&self.b) {
            Target::B
        } else {
            Target::Empty
        }
    }
}

fn ui(swapped: Signal<bool>) -> (Node, Tree) {
    let target = |a: bool, color| {
        let left = swapped.map(move |swapped| if a == swapped { RIGHT } else { LEFT });
        flexbox()
            .style(mue_macroquad::style! {
                position: absolute; left: {left}; top: {TOP};
                width: {TARGET_W}; height: {TARGET_H}; background: {color};
            })
            .into_node()
    };
    let a = target(true, UiColor::from_rgba(50, 160, 230, 255));
    let b = target(false, UiColor::from_rgba(235, 155, 55, 255));
    let a_ref = *a;
    let b_ref = *b;
    let root = flexbox()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
        })
        .children((a, b))
        .into_node();
    let tree = Tree {
        root: *root,
        a: a_ref,
        b: b_ref,
    };
    (root, tree)
}

#[derive(Default)]
struct Results {
    samples: usize,
    agreements: usize,
    differences: usize,
    checked: usize,
    failures: usize,
    last: Option<(Vector, Target, Target, Option<f64>, f64)>,
}

fn draw_text_line(font: &Font, text: &str, x: f32, y: f32, size: u16, color: Color) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font),
            font_size: size,
            color,
            ..Default::default()
        },
    );
}

fn draw_labels(font: &Font, swapped: bool) {
    for (name, x) in [
        ("A", if swapped { RIGHT } else { LEFT }),
        ("B", if swapped { LEFT } else { RIGHT }),
    ] {
        draw_text_line(font, name, x + 100., TOP + 90., 54, WHITE);
    }
}

fn draw_report(font: &Font, options: &Options, step: usize, results: &Results) {
    draw_text_line(font, "Click the target you can SEE", 38., 48., 28, WHITE);
    draw_text_line(
        font,
        &format!(
            "UI: {:.2} FPS | displayed frame {step} | A and B swap each UI frame",
            options.fps
        ),
        38.,
        86.,
        18,
        LIGHTGRAY,
    );
    draw_text_line(
        font,
        "Host input stays live. Clicks wait for the next UI frame.",
        38.,
        118.,
        18,
        LIGHTGRAY,
    );
    draw_text_line(
        font,
        "BEFORE: hit-test the displayed Node tree, then update",
        38.,
        365.,
        19,
        SKYBLUE,
    );
    draw_text_line(
        font,
        "AFTER: update + layout, then hit-test the same click",
        38.,
        398.,
        19,
        ORANGE,
    );
    draw_text_line(
        font,
        &format!(
            "Samples {} | agree {} | DIFFER {}",
            results.samples, results.agreements, results.differences
        ),
        38.,
        440.,
        22,
        WHITE,
    );
    if let Some((position, before, after, time, buffered_ms)) = results.last {
        draw_text_line(
            font,
            &format!(
                "Last ({:.0}, {:.0}): BEFORE={}  AFTER={}",
                position.x,
                position.y,
                before.name(),
                after.name()
            ),
            38.,
            477.,
            22,
            WHITE,
        );
        let timestamp = time.map_or_else(
            || "No touch timestamp (mouse or synthetic sample)".to_owned(),
            |t| format!("touch source timestamp: {t:.6} (unchanged)"),
        );
        draw_text_line(
            font,
            &format!("{timestamp}; buffered {buffered_ms:.0} ms"),
            38.,
            510.,
            16,
            LIGHTGRAY,
        );
    }
    if options.check {
        draw_text_line(
            font,
            &format!(
                "Synthetic check: {} cases, {} failures (not hardware input)",
                results.checked, results.failures
            ),
            38.,
            551.,
            18,
            YELLOW,
        );
    } else {
        draw_text_line(
            font,
            "Clicking blank space is useful too. Escape exits.",
            38.,
            551.,
            18,
            LIGHTGRAY,
        );
    }
    draw_text_line(
        font,
        "This probes Node hit-testing order, not Scene routing or gesture capture.",
        38.,
        597.,
        16,
        GRAY,
    );
}

fn conf() -> Conf {
    Conf {
        window_title: "mue - input timing".to_owned(),
        window_width: WIDTH,
        window_height: HEIGHT,
        window_resizable: false,
        high_dpi: false,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let options = Options::parse();
    let font = load_ttf_font_from_bytes(include_bytes!(
        "../../basics/assets/fonts/SpaceMono-Regular.ttf"
    ))
    .expect("experiment font");
    let owner = OwnedScope::new();
    let swapped = owner.run(|| signal(false));
    let (root, tree) = ui(swapped);
    let mut app = App::new(root);
    let mut subscriber = InputSubscriber::new();
    let mut input = Input::default();
    let target = render_target(WIDTH as u32, HEIGHT as u32);
    target.texture.set_filter(FilterMode::Nearest);
    let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., WIDTH as f32, HEIGHT as f32));
    camera.render_target = Some(target.clone());
    let mut results = Results::default();
    let mut step = 0;
    let mut next_update = get_time();
    println!("frame,x,y,before,after,touch_source_time,buffered_ms,source");

    loop {
        let batch = subscriber.read();
        if batch.is_interrupted() {
            input.clicks.clear();
        }
        batch.repeat(&mut input);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        if get_time() >= next_update {
            let initial = step == 0;
            if !initial && options.check {
                for (position, before, after) in [
                    (
                        Vector::new(LEFT + TARGET_W / 2., TOP + TARGET_H / 2.),
                        if swapped.get() { Target::B } else { Target::A },
                        if swapped.get() { Target::A } else { Target::B },
                    ),
                    (
                        Vector::new(RIGHT + TARGET_W / 2., TOP + TARGET_H / 2.),
                        if swapped.get() { Target::A } else { Target::B },
                        if swapped.get() { Target::B } else { Target::A },
                    ),
                    (Vector::new(500., TOP + 60.), Target::Empty, Target::Empty),
                ] {
                    input.clicks.push(Click {
                        position,
                        touch_time: None,
                        received_at: get_time(),
                        expected: Some((before, after)),
                    });
                }
            }

            // No UI signals, transforms or children change before this lookup.
            // The tree still matches the cached image that was visible on screen.
            let clicks = std::mem::take(&mut input.clicks);
            let before: Vec<_> = clicks
                .iter()
                .map(|click| tree.hit(click.position))
                .collect();
            set_camera(&camera);
            clear_background(Color::from_rgba(16, 20, 30, 255));
            // Existing App has no standalone layout entry. Rendering into a hidden
            // target computes/publishes the real new geometry before AFTER lookup.
            // The screen still shows the previous cached image at this point.
            app.frame_with_update(|| {
                if !initial {
                    swapped.update(|value| *value = !*value);
                }
            });
            draw_labels(&font, swapped.get());

            for (click, before) in clicks.into_iter().zip(before) {
                let after = tree.hit(click.position);
                let buffered_ms = (get_time() - click.received_at) * 1000.;
                results.samples += 1;
                if before == after {
                    results.agreements += 1;
                } else {
                    results.differences += 1;
                }
                if let Some(expected) = click.expected {
                    results.checked += 1;
                    if (before, after) != expected {
                        results.failures += 1;
                    }
                }
                println!(
                    "{step},{:.1},{:.1},{},{},{},{buffered_ms:.3},{}",
                    click.position.x,
                    click.position.y,
                    before.name(),
                    after.name(),
                    click
                        .touch_time
                        .map_or_else(String::new, |t| format!("{t:.6}")),
                    if click.expected.is_some() {
                        "synthetic"
                    } else {
                        "native"
                    }
                );
                results.last = Some((click.position, before, after, click.touch_time, buffered_ms));
            }
            draw_report(&font, &options, step, &results);
            set_default_camera();
            step += 1;
            next_update = get_time() + 1. / options.fps;
        }

        clear_background(BLACK);
        draw_texture_ex(
            &target.texture,
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                flip_y: true,
                ..Default::default()
            },
        );
        let finished = options.check && step >= 7;
        if finished && let Some(path) = &options.capture {
            get_screen_data().export_png(path);
        }
        next_frame().await;
        if finished {
            println!(
                "CHECK: {} cases, {} failures, {} identity differences",
                results.checked, results.failures, results.differences
            );
            std::process::exit(if results.checked == 18 && results.failures == 0 {
                0
            } else {
                1
            });
        }
    }
}
