//! A manual example of a host miniquad render pass alongside mue.
//!
//! Use the Left and Right arrow keys to select a rendering sequence. The
//! window shows the resulting UI and previews of the host and chart targets
//! for direct visual inspection.
//!
//! Run with `DISPLAY=:0 cargo run --manifest-path examples/Cargo.toml --example pass-probe --offline`. Press Escape or
//! close the window to quit. The retained sequence falls back to immediate
//! rendering on backends without retained support.

use macroquad::miniquad::PassAction;
use macroquad::prelude::*;
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
}

struct ChartChannel {
    target: RenderTarget,
    camera: Camera2D,
}

impl ChartChannel {
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

fn retained_text() -> String {
    let line = "retained text line: render pass example 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz\n";
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
                    heading("render pass example").style(mue_macroquad::style! {
                        font-family: {font.clone()};
                        font-size: 28px;
                        color: {MueColor::from_rgba(255, 255, 255, 255)};
                    }),
                    label("host raw pass vs mue App::frame"),
                    show_if(
                        retained,
                        text(retained_text()).style(mue_macroquad::style! {
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

fn render_case(
    case: Case,
    app: &mut App,
    channel: &mut ChartChannel,
    host_target: &RenderTarget,
    chart_target: &RenderTarget,
) {
    set_default_camera();
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
}

fn draw_target_previews(case: Case, host: &RenderTarget, chart: &RenderTarget) {
    // Flush the sequence's queued UI before adding the manual inspection overlay.
    set_default_camera();
    draw_rectangle(0., 0., screen_width(), 70., Color::new(0., 0., 0., 0.85));
    draw_text(format!("Sequence: {}", case.name()), 12., 22., 18., WHITE);
    draw_text(
        "Left / Right: change sequence    Escape: quit",
        12.,
        42.,
        16.,
        WHITE,
    );
    draw_text(
        format!("Retained backend: {}", macroquad::retained::is_supported()),
        12.,
        62.,
        16.,
        WHITE,
    );

    let width = (screen_width() * 0.24).clamp(1., 180.);
    let height = width * HEIGHT as f32 / WIDTH as f32;
    let y = screen_height() - height - 12.;
    for (target, x, label) in [
        (host, 12., "Host target"),
        (chart, 24. + width, "Chart target"),
    ] {
        draw_rectangle(
            x - 4.,
            y - 24.,
            width + 8.,
            height + 28.,
            Color::new(0., 0., 0., 0.85),
        );
        draw_text(label, x, y - 6., 16., WHITE);
        draw_texture_ex(
            &target.texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(width, height)),
                flip_y: true,
                ..Default::default()
            },
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

    let owner = OwnedScope::new();
    let texture = owner.run(|| signal(SharedTexture::from(chart_target.texture.clone())));
    let retained = owner.run(|| signal(false));

    let mut channel = ChartChannel::new(chart_target.clone());
    let mut app = App::new(ui(texture, font, retained));
    let mut selected = 0;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Right) {
            selected = (selected + 1) % Case::ALL.len();
        }
        if is_key_pressed(KeyCode::Left) {
            selected = (selected + Case::ALL.len() - 1) % Case::ALL.len();
        }

        let case = Case::ALL[selected];
        retained.set(case.uses_retained());
        render_case(case, &mut app, &mut channel, &host_target, &chart_target);
        draw_target_previews(case, &host_target, &chart_target);
        next_frame().await;
    }
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - render pass example".into(),
            window_width: WIDTH as i32,
            window_height: HEIGHT as i32,
            ..Default::default()
        },
        run(),
    );
}
