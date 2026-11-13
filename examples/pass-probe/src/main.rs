//! Probe how a host raw miniquad render pass coexists with mue's frame.
//!
//! Each case runs one measured frame and reads back:
//! - the host render target center (did the host pass survive?),
//! - the default framebuffer center before macroquad flushes queued UI,
//! - the default framebuffer center after `get_screen_data()` flushes UI,
//! - the chart target center (did the channel render?).
//!
//! Run with `DISPLAY=:0 cargo run -p pass-probe --offline`. The process exits
//! with 0 when every case matches its expectation and 1 otherwise.
//!
//! The retained case requires macroquad's OpenGL retained backend. On other
//! backends the probe reports `retained supported: false`; the retained case
//! falls back to the immediate path and fails its expectation.

use std::io::Write;

use macroquad::miniquad::PassAction;
use macroquad::prelude::*;
use macroquad::texture::get_screen_data;
use macroquad::window::get_internal_gl;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as MueColor,
    font::{FamilyOwned, load_font},
    node::portal,
    shader::SharedTexture,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

const HOST_GREEN: Color = Color::new(0.0, 1.0, 0.0, 1.0);
const HOST_RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
const CHART_CYAN: Color = Color::new(0.0, 1.0, 1.0, 1.0);
const CHART_MAGENTA: Color = Color::new(1.0, 0.0, 1.0, 1.0);
const CHART_YELLOW: Color = Color::new(1.0, 1.0, 0.0, 1.0);
const SCREEN_BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);

#[derive(Clone, Copy, Debug)]
enum Case {
    RawOnly,
    AppInPassNoRetained,
    AppInPassRetained,
    FlushInPass,
    ChannelInPass,
    ChannelBeforePass,
    MacroquadStateTarget,
    SetDefaultCameraInPass,
    SetDefaultCameraInPassNoPending,
}

impl Case {
    const ALL: [Case; 9] = [
        Case::RawOnly,
        Case::AppInPassNoRetained,
        Case::AppInPassRetained,
        Case::FlushInPass,
        Case::ChannelInPass,
        Case::ChannelBeforePass,
        Case::MacroquadStateTarget,
        Case::SetDefaultCameraInPass,
        Case::SetDefaultCameraInPassNoPending,
    ];

    fn name(self) -> &'static str {
        match self {
            Case::RawOnly => "raw_only",
            Case::AppInPassNoRetained => "app_in_pass_no_retained",
            Case::AppInPassRetained => "app_in_pass_retained",
            Case::FlushInPass => "flush_in_pass",
            Case::ChannelInPass => "channel_in_pass",
            Case::ChannelBeforePass => "channel_before_pass",
            Case::MacroquadStateTarget => "macroquad_state_target",
            Case::SetDefaultCameraInPass => "set_default_camera_in_pass",
            Case::SetDefaultCameraInPassNoPending => "set_default_camera_in_pass_no_pending",
        }
    }

    fn uses_retained(self) -> bool {
        matches!(self, Case::AppInPassRetained)
    }

    fn renders_chart_in_case(self) -> bool {
        matches!(self, Case::ChannelInPass | Case::ChannelBeforePass)
    }

    fn expected(self) -> Expected {
        match self {
            Case::RawOnly | Case::AppInPassNoRetained | Case::ChannelBeforePass => Expected {
                host: HOST_RED,
                screen_raw: SCREEN_BLACK,
                chart: CHART_YELLOW,
            },
            Case::AppInPassRetained
            | Case::FlushInPass
            | Case::ChannelInPass
            | Case::SetDefaultCameraInPass => Expected {
                host: HOST_GREEN,
                screen_raw: HOST_RED,
                chart: CHART_YELLOW,
            },
            Case::MacroquadStateTarget => Expected {
                host: CHART_YELLOW,
                screen_raw: SCREEN_BLACK,
                chart: CHART_YELLOW,
            },
            Case::SetDefaultCameraInPassNoPending => Expected {
                host: HOST_RED,
                screen_raw: SCREEN_BLACK,
                chart: CHART_YELLOW,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct Expected {
    host: Color,
    screen_raw: Color,
    chart: Color,
}

struct CaseResult {
    case: Case,
    host: Color,
    screen_raw: Color,
    screen: Color,
    chart: Color,
    pass: bool,
}

struct MockChannel {
    target: RenderTarget,
    camera: Camera2D,
}

impl MockChannel {
    fn new(target: RenderTarget) -> Self {
        let mut camera = Camera2D::from_display_rect(Rect::new(
            0.,
            0.,
            target.texture.width(),
            target.texture.height(),
        ));
        camera.render_target = Some(target.clone());
        Self { target, camera }
    }

    fn render(&mut self) {
        let width = self.target.texture.width();
        let height = self.target.texture.height();
        set_camera(&self.camera);
        clear_background(CHART_CYAN);
        draw_rectangle(width * 0.5, 0., width * 0.5, height, CHART_MAGENTA);
        draw_rectangle(
            width * 0.5 - 20.,
            height * 0.5 - 20.,
            40.,
            40.,
            CHART_YELLOW,
        );
        set_default_camera();
    }
}

fn long_text() -> String {
    let line = "retained text line: pass ownership probe 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz\n";
    line.repeat(20)
}

fn ui(texture: Signal<SharedTexture>, font: FamilyOwned, retained: Signal<bool>) -> impl Component {
    let label = |value: &'static str| {
        text(value).style(mue_macroquad::style! {
            font-family: {font.clone()};
            font-size: 14px;
            color: {MueColor::from_rgba(210, 220, 240, 255)};
        })
    };

    flexbox()
        .style(mue_macroquad::style! {
            width: 100%;
            height: 100%;
            background: {MueColor::from_rgba(8, 10, 16, 255)};
        })
        .children((
            portal(texture).style(mue_macroquad::style! {
                position: absolute;
                inset: 0;
            }),
            flexbox()
                .style(mue_macroquad::style! {
                    position: absolute;
                    inset: 0;
                    padding: 12px;
                    gap: 4px;
                    flex-direction: column;
                    align-items: flex-start;
                    justify-content: flex-start;
                })
                .children((
                    heading("pass ownership probe").style(mue_macroquad::style! {
                        font-family: {font.clone()};
                        font-size: 28px;
                        color: {MueColor::from_rgba(255, 255, 255, 255)};
                    }),
                    label("host raw pass vs mue App::frame"),
                    show_if(
                        retained,
                        text(long_text()).style(mue_macroquad::style! {
                            font-family: {font.clone()};
                            font-size: 9px;
                            color: {MueColor::from_rgba(230, 230, 230, 255)};
                            position: absolute;
                            inset: 0;
                        }),
                    ),
                )),
        ))
}

fn begin_host_pass(target: &RenderTarget, color: Color) {
    let gl = unsafe { get_internal_gl() };
    gl.quad_context.begin_pass(
        Some(target.render_pass.raw_miniquad_id()),
        PassAction::Nothing,
    );
    gl.quad_context
        .clear(Some((color.r, color.g, color.b, color.a)), None, None);
}

fn end_host_pass() {
    let gl = unsafe { get_internal_gl() };
    gl.quad_context.end_render_pass();
}

fn clear_current(color: Color) {
    let gl = unsafe { get_internal_gl() };
    gl.quad_context
        .clear(Some((color.r, color.g, color.b, color.a)), None, None);
}

fn clear_target(target: &RenderTarget, color: Color) {
    let gl = unsafe { get_internal_gl() };
    gl.quad_context.begin_pass(
        Some(target.render_pass.raw_miniquad_id()),
        PassAction::Clear {
            color: Some((color.r, color.g, color.b, color.a)),
            depth: None,
            stencil: None,
        },
    );
    gl.quad_context.end_render_pass();
}

fn read_target(target: &RenderTarget) -> Color {
    let image = target.texture.get_texture_data();
    image.get_pixel(image.width as u32 / 2, image.height as u32 / 2)
}

fn read_screen_raw(screen_probe: &RenderTarget) -> Color {
    screen_probe.texture.grab_screen();
    let image = screen_probe.texture.get_texture_data();
    image.get_pixel(image.width as u32 / 2, image.height as u32 / 2)
}

fn read_screen_flushed() -> Color {
    let image = get_screen_data();
    image.get_pixel(image.width as u32 / 2, image.height as u32 / 2)
}

fn same(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
}

fn color_label(color: Color) -> String {
    let name = if same(color, HOST_RED) {
        "red"
    } else if same(color, HOST_GREEN) {
        "green"
    } else if same(color, SCREEN_BLACK) {
        "black"
    } else if same(color, CHART_YELLOW) {
        "chart"
    } else if same(color, CHART_CYAN) {
        "cyan"
    } else if same(color, CHART_MAGENTA) {
        "magenta"
    } else {
        "other"
    };
    format!(
        "{name}({:.0},{:.0},{:.0})",
        color.r * 255.,
        color.g * 255.,
        color.b * 255.
    )
}

async fn run_case(
    case: Case,
    app: &mut App,
    channel: &mut MockChannel,
    host_target: &RenderTarget,
    chart_target: &RenderTarget,
    screen_probe: &RenderTarget,
    retained: &Signal<bool>,
) -> CaseResult {
    retained.set(case.uses_retained());
    if case.uses_retained() {
        // Warm up layout and the retained mesh cache before the measured frame.
        app.frame();
        next_frame().await;
    }

    // Begin the measured frame from a known state.
    unsafe { get_internal_gl() }.flush();
    clear_background(SCREEN_BLACK);
    clear_target(host_target, SCREEN_BLACK);
    clear_target(chart_target, SCREEN_BLACK);

    if !case.renders_chart_in_case() {
        channel.render();
    }

    match case {
        Case::RawOnly => {
            begin_host_pass(host_target, HOST_GREEN);
            clear_current(HOST_RED);
            end_host_pass();
            app.frame();
        }
        Case::AppInPassNoRetained | Case::AppInPassRetained => {
            begin_host_pass(host_target, HOST_GREEN);
            app.frame();
            clear_current(HOST_RED);
            end_host_pass();
        }
        Case::FlushInPass => {
            begin_host_pass(host_target, HOST_GREEN);
            app.frame();
            unsafe { get_internal_gl() }.flush();
            clear_current(HOST_RED);
            end_host_pass();
        }
        Case::ChannelInPass => {
            begin_host_pass(host_target, HOST_GREEN);
            channel.render();
            clear_current(HOST_RED);
            end_host_pass();
        }
        Case::ChannelBeforePass => {
            channel.render();
            begin_host_pass(host_target, HOST_GREEN);
            clear_current(HOST_RED);
            end_host_pass();
            app.frame();
        }
        Case::MacroquadStateTarget => {
            {
                let gl = unsafe { get_internal_gl() };
                gl.quad_gl
                    .render_pass(Some(host_target.render_pass.raw_miniquad_id()));
            }
            app.frame();
            unsafe { get_internal_gl() }.flush();
            {
                let gl = unsafe { get_internal_gl() };
                gl.quad_gl.render_pass(None);
            }
        }
        Case::SetDefaultCameraInPass => {
            begin_host_pass(host_target, HOST_GREEN);
            app.frame();
            set_default_camera();
            clear_current(HOST_RED);
            end_host_pass();
        }
        Case::SetDefaultCameraInPassNoPending => {
            begin_host_pass(host_target, HOST_GREEN);
            set_default_camera();
            clear_current(HOST_RED);
            end_host_pass();
        }
    }

    let screen_raw = read_screen_raw(screen_probe);
    let screen = read_screen_flushed();
    next_frame().await;
    let host = read_target(host_target);
    let chart = read_target(chart_target);

    let expected = case.expected();
    let pass = same(host, expected.host)
        && same(screen_raw, expected.screen_raw)
        && same(chart, expected.chart);

    CaseResult {
        case,
        host,
        screen_raw,
        screen,
        chart,
        pass,
    }
}

fn print_case(result: &CaseResult) {
    let expected = result.case.expected();
    println!("case: {}", result.case.name());
    println!("  host target center: {}", color_label(result.host));
    println!("  screen center (raw): {}", color_label(result.screen_raw));
    println!(
        "  screen center (after get_screen_data flush): {}",
        color_label(result.screen)
    );
    println!("  chart target center: {}", color_label(result.chart));
    println!(
        "  expected: host={}, screen_raw={}, chart={}",
        color_label(expected.host),
        color_label(expected.screen_raw),
        color_label(expected.chart)
    );
    println!("  verdict: {}", if result.pass { "PASS" } else { "FAIL" });
    println!();
}

fn print_summary(results: &[CaseResult]) {
    println!("summary");
    println!(
        "{:<38} {:<16} {:<16} {:<16} {:<16} {}",
        "case", "host", "screen_raw", "screen", "chart", "verdict"
    );
    for result in results {
        println!(
            "{:<38} {:<16} {:<16} {:<16} {:<16} {}",
            result.case.name(),
            color_label(result.host),
            color_label(result.screen_raw),
            color_label(result.screen),
            color_label(result.chart),
            if result.pass { "PASS" } else { "FAIL" }
        );
    }
}

async fn run() {
    let font =
        load_font(include_bytes!("../../basics/assets/fonts/SpaceMono-Regular.ttf").to_vec())
            .remove(0);

    let chart_target = render_target(WIDTH, HEIGHT);
    chart_target.texture.set_filter(FilterMode::Nearest);
    let host_target = render_target(WIDTH, HEIGHT);
    host_target.texture.set_filter(FilterMode::Nearest);
    let (screen_width, screen_height) = macroquad::miniquad::window::screen_size();
    let screen_probe = render_target(screen_width as u32, screen_height as u32);

    let owner = OwnedScope::new();
    let texture = owner.run(|| signal(SharedTexture::from(chart_target.texture.clone())));
    let retained = owner.run(|| signal(false));

    let mut channel = MockChannel::new(chart_target.clone());
    let mut app = App::new(ui(texture, font, retained));

    println!("pass ownership probe");
    println!(
        "retained supported: {}",
        macroquad::retained::is_supported()
    );
    println!("window: {WIDTH}x{HEIGHT}");
    println!();

    let mut results = Vec::new();
    for case in Case::ALL {
        let result = run_case(
            case,
            &mut app,
            &mut channel,
            &host_target,
            &chart_target,
            &screen_probe,
            &retained,
        )
        .await;
        print_case(&result);
        results.push(result);
    }

    print_summary(&results);
    let all_pass = results.iter().all(|result| result.pass);
    let _ = std::io::stdout().flush();
    std::process::exit(if all_pass { 0 } else { 1 });
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - pass ownership probe".into(),
            window_width: WIDTH as i32,
            window_height: HEIGHT as i32,
            ..Default::default()
        },
        run(),
    );
}
