//! Composite a channel render target through a mue portal node.
//!
//! The chart channel owns the gameplay HUD (including the pause button) and
//! receives all pointers that start in gameplay. The mue overlay is modal: new
//! pointers belong to mue while it is open. A pointer that started in gameplay
//! keeps its owner across the mode switch, so a held touch is not cancelled by
//! pausing.

mod controller;

use std::collections::HashMap;

use controller::{ChannelTouch, GameChannel, GameController, InputMode, TouchPhase};
use macroquad::prelude::*;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color as MueColor,
    font::{FamilyOwned, load_font},
    node::portal,
    shader::SharedTexture,
    style::Styleable,
};

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
}

impl GameChannel for MockChannel {
    fn resize(&mut self, size: (u32, u32), _dpi: f32) {
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

    fn render(&mut self, time: f64, _dt: f32) {
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

    fn touch(&mut self, event: ChannelTouch) -> bool {
        match event.phase {
            TouchPhase::Down => {
                if self.pause_rect().contains(event.position) {
                    self.pause_requested = true;
                    return true;
                }
                self.touches.insert(event.id, event.position);
                true
            }
            TouchPhase::Hold => {
                // A pointer held from the pause: track it, but do not treat
                // it as a click or a chart HUD press.
                self.touches.insert(event.id, event.position);
                true
            }
            TouchPhase::Move => {
                if let Some(position) = self.touches.get_mut(&event.id) {
                    *position = event.position;
                    true
                } else {
                    false
                }
            }
            TouchPhase::Up | TouchPhase::Cancel => self.touches.remove(&event.id).is_some(),
        }
    }

    fn texture(&self) -> SharedTexture {
        self.target.texture.clone().into()
    }
}

fn ui(texture: Signal<SharedTexture>, font: FamilyOwned, paused: Signal<bool>) -> impl Component {
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
                    .children((
                        heading("paused").style(mue_macroquad::style! {
                            font-family: {font.clone()};
                            font-size: 36px;
                            color: {MueColor::from_rgba(255, 255, 255, 255)};
                        }),
                        label("new pointers are swallowed while the overlay is open."),
                        button("resume")
                            .primary(true)
                            .style(mue_macroquad::style! {
                                font-family: {font.clone()};
                                font-size: 16px;
                            })
                            .on_activate(move || paused.set(false)),
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
    let (texture, paused) = owner.run(|| {
        let placeholder: SharedTexture = Texture2D::from_rgba8(1, 1, &[0, 0, 0, 0]).into();
        (signal(placeholder), signal(false))
    });
    let mut controller = GameController::new(MockChannel::new(chart_font), texture);
    let mut app = App::new(ui(texture, font, paused));

    loop {
        clear_background(BLACK);
        set_default_camera();

        if is_key_pressed(KeyCode::R) {
            request_new_screen_size(640., 360.);
        }

        // Input is replayed from miniquad's event queue at frame start.
        controller.process_input();

        // Test hook: P toggles the modal without a second finger, so a held
        // chart pointer can be observed across the mode switch.
        if is_key_pressed(KeyCode::P) {
            if controller.mode() == InputMode::Gameplay {
                controller.enter_overlay();
                paused.set(true);
            } else {
                controller.exit_overlay(Some(get_time()));
                paused.set(false);
            }
        }

        // The chart HUD owns the pause button; the channel reports the hit.
        if controller.channel_mut().take_pause_request() {
            controller.enter_overlay();
            paused.set(true);
        }

        // Back closes the overlay before the host sees it.
        if (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Back))
            && controller.back(Some(get_time()))
        {
            paused.set(false);
        }

        // The overlay's resume button clears paused; the host returns new
        // pointers to the chart on the next frame.
        if !paused.get() && controller.mode() == InputMode::Overlay {
            controller.exit_overlay(Some(get_time()));
        }

        let window_size = (
            screen_width().max(1.0) as u32,
            screen_height().max(1.0) as u32,
        );
        controller.render(
            window_size,
            screen_dpi_scale(),
            get_time(),
            get_frame_time(),
        );

        app.frame();
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
