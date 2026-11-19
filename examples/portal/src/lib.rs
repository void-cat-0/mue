//! Composite a channel render target through a mue portal node.
//!
//! The chart channel owns the gameplay HUD (including the pause button) and
//! receives all pointers that start in gameplay. The mue overlay is modal: new
//! pointers belong to mue while it is open. A pointer that started in gameplay
//! keeps its owner across the mode switch, so a held touch is not cancelled by
//! pausing.

mod input;
mod session;
mod target;

use std::collections::HashMap;

use input::{ChartInput, ChartTouch, InputEvent, InputEvents, InputMode, TouchPhase};
use macroquad::prelude::*;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, RenderStateGuard, Shadow,
    color::Color as MueColor,
    font::{FamilyOwned, load_font},
    input::InputSubscriber,
    node::portal,
    shader::SharedTexture,
    style::{Size, Styleable},
};
use session::{CommandQueue, GameUiCommand, MockSession, command_queue, drain};
use target::TargetMap;

const CHART_WIDTH: u32 = 1280;
const CHART_HEIGHT: u32 = 720;

struct MockChannel {
    target: RenderTarget,
    camera: Camera2D,
    font: Font,
    touches: HashMap<u64, Vec2>,
    pause_requested: bool,
    width: f32,
    height: f32,
}

impl MockChannel {
    fn new(font: Font) -> Self {
        let width = CHART_WIDTH as f32;
        let height = CHART_HEIGHT as f32;
        let target = render_target(CHART_WIDTH, CHART_HEIGHT);
        target.texture.set_filter(FilterMode::Linear);
        let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., width, height));
        camera.render_target = Some(target.clone());
        Self {
            target,
            camera,
            font,
            touches: HashMap::new(),
            pause_requested: false,
            width,
            height,
        }
    }

    fn pause_rect(&self) -> Rect {
        Rect::new(self.width - 72., 16., 56., 56.)
    }

    fn take_pause_request(&mut self) -> bool {
        std::mem::take(&mut self.pause_requested)
    }

    fn resize(&mut self, size: (u32, u32)) {
        if self.target.texture.width() as u32 == size.0
            && self.target.texture.height() as u32 == size.1
        {
            return;
        }
        let target = render_target(size.0, size.1);
        target.texture.set_filter(FilterMode::Linear);
        let mut camera =
            Camera2D::from_display_rect(Rect::new(0., 0., size.0 as f32, size.1 as f32));
        camera.render_target = Some(target.clone());
        self.target = target;
        self.camera = camera;
        self.width = size.0 as f32;
        self.height = size.1 as f32;
        // Touch positions are in target coordinates; drop them on resize.
        self.touches.clear();
    }

    fn render(&mut self, time: f64) {
        let width = self.width;
        let height = self.height;
        set_camera(&self.camera);
        clear_background(Color::from_rgba(18, 20, 28, 255));
        let marker_w = width * 0.1875;
        let marker_h = height * 0.25;
        draw_rectangle(0., 0., marker_w, marker_h, RED);
        draw_rectangle(
            width - marker_w,
            height - marker_h,
            marker_w,
            marker_h,
            BLUE,
        );
        draw_line(0., 0., width, height, 6., WHITE);
        draw_circle(width * 0.5, height * 0.5, height * 0.111, YELLOW);
        draw_text_ex(
            format!("chart {time:.1}s"),
            width * 0.025,
            height * 0.09,
            TextParams {
                font: Some(&self.font),
                font_size: (height * 0.066) as u16,
                color: WHITE,
                ..Default::default()
            },
        );

        // Chart HUD: the pause button belongs to the chart channel.
        let pause = self.pause_rect();
        draw_rectangle(
            pause.x,
            pause.y,
            pause.w,
            pause.h,
            Color::from_rgba(30, 36, 52, 220),
        );
        draw_rectangle(pause.x + 18., pause.y + 14., 6., 28., WHITE);
        draw_rectangle(pause.x + 32., pause.y + 14., 6., 28., WHITE);

        for position in self.touches.values() {
            draw_circle_lines(position.x, position.y, 18., 3., WHITE);
            draw_circle(position.x, position.y, 4., WHITE);
        }
        // Channel contract: restore macroquad state before returning.
        set_default_camera();
    }

    fn touch(&mut self, event: ChartTouch) {
        match event.phase {
            TouchPhase::Down => {
                if self.pause_rect().contains(event.position) {
                    self.pause_requested = true;
                } else {
                    self.touches.insert(event.id, event.position);
                }
            }
            TouchPhase::Hold => {
                // Track a held pointer without a click or a chart HUD press.
                self.touches.insert(event.id, event.position);
            }
            TouchPhase::Move => {
                if let Some(position) = self.touches.get_mut(&event.id) {
                    *position = event.position;
                }
            }
            TouchPhase::Up | TouchPhase::Cancel => {
                self.touches.remove(&event.id);
            }
        }
    }

    fn texture(&self) -> SharedTexture {
        self.target.texture.clone().into()
    }
}

/// Translate raw physical window events through the currently displayed map.
fn route_input(
    input: &mut ChartInput,
    map: TargetMap,
    dpi: f32,
    event: InputEvent,
) -> Vec<ChartTouch> {
    let InputEvent::Pointer(event) = event else {
        return input.cancel_all();
    };
    let logical = event.position / dpi;
    let position = map.window_to_target_unclamped(logical);
    let routed = match event.phase {
        TouchPhase::Down => map
            .window_to_target(logical)
            .and_then(|position| input.pointer_down(event.id, position, event.time)),
        TouchPhase::Move => input.pointer_move(event.id, position, event.time),
        TouchPhase::Up => input.pointer_up(event.id, position, event.time),
        TouchPhase::Cancel => input.pointer_cancel(event.id),
        TouchPhase::Hold => unreachable!("the host only synthesizes Hold on resume"),
    };
    routed.into_iter().collect()
}

/// Both subscribers consume a batch before its new mode or overlay is published.
/// Publish before the next layout/render; no Down is re-hit on a new modal.
fn finish_input_frame(
    input: &mut ChartInput,
    session: &MockSession,
    snapshot: Signal<session::GameUiSnapshot>,
    time: f64,
) -> Vec<ChartTouch> {
    let held = match (session.is_paused(), input.mode()) {
        (true, InputMode::Gameplay) => {
            input.enter_overlay();
            Vec::new()
        }
        (false, InputMode::Overlay) => input.exit_overlay(Some(time)),
        _ => Vec::new(),
    };
    snapshot.set(session.snapshot());
    held
}

fn ui(
    texture: Signal<SharedTexture>,
    font: FamilyOwned,
    paused: ReadSignal<bool>,
    dead: ReadSignal<bool>,
    title: ReadSignal<&'static str>,
    commands: CommandQueue,
) -> impl Component {
    let label = |value: &'static str| {
        text(value).style(mue_macroquad::style! {
            font-family: {font.clone()};
            font-size: 20px;
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
                    padding: 32px;
                    gap: 12px;
                    flex-direction: column;
                    align-items: center;
                    justify-content: center;
                })
                .children((
                    heading("mue overlay").style(mue_macroquad::style! {
                        font-family: {font.clone()};
                        font-size: 42px;
                        color: {MueColor::from_rgba(255, 255, 255, 255)};
                    }),
                    label("chart texture is composited by the portal node."),
                    label("the session owns pause; the overlay sends commands."),
                    label("the pause button is drawn by the chart channel."),
                    label("new pointers belong to mue while paused."),
                    label("hold the chart and press P: the held touch survives."),
                    label("press during pause, then P: the held touch is synced as Hold."),
                )),
            overlay_host(
                paused,
                flexbox()
                    .style(mue_macroquad::style! {
                        width: 320px;
                        padding: 24px;
                        background: {MueColor::from_rgba(20, 24, 36, 250)};
                        border-radius: 16px;
                        flex-direction: column;
                        gap: 12px;
                        align-items: center;
                    })
                    .shadow(Shadow::new(18., 16.))
                    .shadow_cache(true)
                    .children((
                        heading(title).style(mue_macroquad::style! {
                            font-family: {font.clone()};
                            font-size: 36px;
                            color: {MueColor::from_rgba(255, 255, 255, 255)};
                        }),
                        flexbox()
                            .style(mue_macroquad::style! {
                                width: 272px;
                                height: 153px;
                                flex-shrink: 0;
                            })
                            .clip_radius(Size::Px(12.))
                            .children(portal(texture).style(mue_macroquad::style! {
                                width: 100%;
                                height: 100%;
                            })),
                        button("resume")
                            .primary(true)
                            .disabled(dead)
                            .style(mue_macroquad::style! {
                                font-family: {font.clone()};
                                font-size: 16px;
                            })
                            .on_activate({
                                let commands = commands.clone();
                                move || commands.borrow_mut().push(GameUiCommand::Resume)
                            }),
                        button("retry")
                            .style(mue_macroquad::style! {
                                font-family: {font.clone()};
                                font-size: 16px;
                            })
                            .on_activate({
                                let commands = commands.clone();
                                move || commands.borrow_mut().push(GameUiCommand::Retry)
                            }),
                        button("exit")
                            .style(mue_macroquad::style! {
                                font-family: {font.clone()};
                                font-size: 16px;
                            })
                            .on_activate({
                                let commands = commands.clone();
                                move || commands.borrow_mut().push(GameUiCommand::Exit)
                            }),
                    )),
            )
            .backdrop(MueColor::from_rgba(0, 0, 0, 170))
            .dismissible(false),
        ))
}

pub async fn run() {
    let font_bytes = include_bytes!("../../basics/assets/fonts/SpaceMono-Regular.ttf");
    let font = load_font(font_bytes.to_vec()).remove(0);
    // The channel draws its own text, so it needs macroquad's font stack too.
    // This is a separate loader from mue's cosmic-text font system.
    let chart_font = load_ttf_font_from_bytes(font_bytes).expect("chart font");

    let owner = OwnedScope::new();
    let mut session = MockSession::new(12.0);
    let (texture, snapshot, paused, dead, title) = owner.run(|| {
        let placeholder: SharedTexture = Texture2D::from_rgba8(1, 1, &[0, 0, 0, 0]).into();
        let snapshot = signal(session.snapshot());
        let paused = snapshot.map(|s| s.paused);
        let dead = snapshot.map(|s| s.dead);
        let title = snapshot.map(|s| if s.dead { "failed" } else { "paused" });
        (signal(placeholder), snapshot, paused, dead, title)
    });
    let commands = command_queue();
    let mut chart = MockChannel::new(chart_font);
    let mut input = ChartInput::new();
    let mut subscriber = InputSubscriber::new();
    let mut events = InputEvents::default();
    let mut displayed_map = None;
    let mut app = App::new(ui(texture, font, paused, dead, title, commands.clone()));

    loop {
        clear_background(BLACK);
        set_default_camera();

        if is_key_pressed(KeyCode::R) {
            request_new_screen_size(640., 360.);
        }

        // UI commands are applied at the frame boundary, before the chart
        // update, so the session sees one consistent state per frame.
        for command in drain(&commands) {
            if command == GameUiCommand::Retry {
                // A new attempt is not a resume of the old session's touches.
                for event in input.cancel_all() {
                    chart.touch(event);
                }
            }
            session.apply(command);
        }
        if session.exited() {
            break;
        }

        // This example follows window DPI for resolution; TargetMap itself is
        // independent of that policy. Establish a real map before first input.
        let scale = screen_dpi_scale();
        let dpi = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        let layout =
            mue_macroquad::math::Rect::new(0., 0., screen_width().max(1.), screen_height().max(1.));
        let size = (
            (layout.w * dpi).round().max(1.) as u32,
            (layout.h * dpi).round().max(1.) as u32,
        );
        let next_map = TargetMap::new(layout, size);
        if displayed_map.is_none() {
            chart.resize(size);
        }
        let input_map = displayed_map.unwrap_or(next_map);

        // Replay while mode and overlay visibility still match the displayed
        // frame. In particular, don't publish a HUD pause between subscribers.
        let batch = subscriber.read();
        if batch.is_interrupted() {
            events.interrupt();
        }
        batch.repeat(&mut events);
        for event in events.drain() {
            for event in route_input(&mut input, input_map, dpi, event) {
                chart.touch(event);
            }
        }

        // Keyboard shortcut: P toggles pause through the session, without a second
        // finger, so a held chart pointer can be observed across the switch.
        if is_key_pressed(KeyCode::P) {
            if session.is_paused() {
                session.resume();
            } else {
                session.pause();
            }
        }

        // The chart HUD owns the pause button; the channel reports the hit.
        if chart.take_pause_request() {
            session.pause();
        }

        // Back is a session policy, not a renderer or framework operation.
        if (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Back))
            && input.mode() == InputMode::Overlay
            && !session.is_dead()
        {
            session.resume();
        }

        // The session advances only while playing.
        let dt = get_frame_time();
        session.update(dt);

        // Resizing cancels chart tracking explicitly. Moving a layout alone
        // would only change its mapping; no hidden cancellation in TargetMap.
        if chart.target.texture.width() as u32 != size.0
            || chart.target.texture.height() as u32 != size.1
        {
            for event in input.cancel_all() {
                chart.touch(event);
            }
            chart.resize(size);
        }
        displayed_map = Some(next_map);
        let guard = RenderStateGuard::enter();
        chart.render(session.time());
        drop(guard);
        texture.set(chart.texture());

        app.frame_with_update(|| {
            for event in finish_input_frame(&mut input, &session, snapshot, get_time()) {
                chart.touch(event);
            }
        });
        next_frame().await;
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - portal".into(),
            ..Default::default()
        },
        run(),
    );
}
