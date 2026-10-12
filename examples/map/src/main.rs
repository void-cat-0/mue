use std::{cell::RefCell, rc::Rc};

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    event::pointer::{PointerAction, PointerId},
    layout::use_layout,
    node::Node,
    style::{Style, StyleableExt},
};

const CELL: f32 = 160.;
const HALF_GRID: i32 = 8;

struct Contact {
    id: PointerId,
    position: Vec2,
}

struct MapState {
    contacts: Vec<Contact>,
    offset: Vec2,
    scale: f32,
    rotation: f32,
    velocity: Vec2,
    last_move: f32,
    transforming: bool,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            contacts: Vec::new(),
            offset: Vec2::ZERO,
            scale: 1.,
            rotation: 0.,
            velocity: Vec2::ZERO,
            last_move: 0.,
            transforming: false,
        }
    }
}

fn rotate(p: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(cos * p.x - sin * p.y, sin * p.x + cos * p.y)
}

impl MapState {
    // Preserve the world point under the old midpoint while transforming it
    // to the new midpoint, so zooming/rotating never jumps to the map origin.
    fn transform(&mut self, old: Vec2, new: Vec2, factor: f32, angle: f32) {
        let scale = (self.scale * factor).clamp(0.25, 4.);
        self.offset = new + rotate(self.offset - old, angle) * (scale / self.scale);
        self.scale = scale;
        self.rotation = (self.rotation + angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
    }

    fn move_contact(&mut self, index: usize, point: Vec2, center: Vec2, time: f32) {
        if self.contacts.len() == 2 {
            let old_mid = (self.contacts[0].position + self.contacts[1].position) * 0.5;
            let old_span = self.contacts[1].position - self.contacts[0].position;
            self.contacts[index].position = point;
            let new_mid = (self.contacts[0].position + self.contacts[1].position) * 0.5;
            let new_span = self.contacts[1].position - self.contacts[0].position;
            let (factor, angle) = if old_span.length() > 4. && new_span.length() > 4. {
                (
                    new_span.length() / old_span.length(),
                    new_span.y.atan2(new_span.x) - old_span.y.atan2(old_span.x),
                )
            } else {
                (1., 0.)
            };
            self.transform(old_mid - center, new_mid - center, factor, angle);
            self.velocity = Vec2::ZERO;
        } else {
            let delta = point - self.contacts[index].position;
            self.contacts[index].position = point;
            self.offset += delta;
            let elapsed = time - self.last_move;
            if delta.length_squared() > 0. {
                if elapsed > 0. && elapsed < 0.15 {
                    self.velocity = (delta / elapsed).clamp_length_max(2400.);
                } else {
                    self.velocity = Vec2::ZERO;
                }
                self.last_move = time;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Control {
    ZoomOut,
    ZoomIn,
    RotateLeft,
    RotateRight,
    Reset,
}

fn controls() -> [(Rect, Control, &'static str); 5] {
    let left = (screen_width() - 320.) * 0.5;
    let top = screen_height() - 64.;
    [
        (Rect::new(left, top, 48., 44.), Control::ZoomOut, "-"),
        (Rect::new(left + 54., top, 48., 44.), Control::ZoomIn, "+"),
        (
            Rect::new(left + 108., top, 48., 44.),
            Control::RotateLeft,
            "-15°",
        ),
        (
            Rect::new(left + 162., top, 48., 44.),
            Control::RotateRight,
            "+15°",
        ),
        (
            Rect::new(left + 216., top, 104., 44.),
            Control::Reset,
            "Reset view",
        ),
    ]
}

fn apply_control(state: &mut MapState, control: Control) {
    state.velocity = Vec2::ZERO;
    match control {
        Control::ZoomOut => state.transform(Vec2::ZERO, Vec2::ZERO, 1. / 1.25, 0.),
        Control::ZoomIn => state.transform(Vec2::ZERO, Vec2::ZERO, 1.25, 0.),
        Control::RotateLeft => state.transform(Vec2::ZERO, Vec2::ZERO, 1., -15_f32.to_radians()),
        Control::RotateRight => state.transform(Vec2::ZERO, Vec2::ZERO, 1., 15_f32.to_radians()),
        Control::Reset => *state = MapState::default(),
    }
}

fn map_node(texture: Texture2D, state: Rc<RefCell<MapState>>) -> Node {
    Node::build_with_style(Style::new().w_full().h_full(), move |style| {
        let rect = use_layout(style).rect;
        let input = state.clone();
        style.on_pointer_event.append(move |event| {
            let mut state = input.borrow_mut();
            let point = vec2(event.position().x, event.position().y);
            if event.action() == PointerAction::Down
                && let Some((_, control, _)) =
                    controls().into_iter().find(|(r, _, _)| r.contains(point))
            {
                apply_control(&mut state, control);
                return;
            }
            let r = rect.get();
            let center = vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            let index = state
                .contacts
                .iter()
                .position(|c| c.id == event.pointer_id());
            match event.action() {
                PointerAction::Down if index.is_none() && state.contacts.len() < 2 => {
                    state.velocity = Vec2::ZERO;
                    state.last_move = event.time();
                    state.contacts.push(Contact {
                        id: event.pointer_id(),
                        position: point,
                    });
                    state.transforming = state.contacts.len() == 2;
                }
                PointerAction::Move => {
                    if let Some(index) = index {
                        state.move_contact(index, point, center, event.time());
                    }
                }
                PointerAction::Up | PointerAction::Cancel => {
                    if let Some(index) = index {
                        if event.action() == PointerAction::Up {
                            state.move_contact(index, point, center, event.time());
                        }
                        if event.action() == PointerAction::Cancel
                            || state.transforming
                            || event.time() - state.last_move > 0.1
                        {
                            state.velocity = Vec2::ZERO;
                        }
                        state.contacts.remove(index);
                        state.transforming = false;
                        state.last_move = event.time();
                    }
                }
                _ => {}
            }
        });
        let wheel = state.clone();
        style.on_wheel_event.append(move |event| {
            if controls()
                .iter()
                .any(|(r, _, _)| r.contains(vec2(event.position.x, event.position.y)))
            {
                return;
            }
            let r = rect.get();
            let anchor = vec2(
                event.position.x - r.x - r.w * 0.5,
                event.position.y - r.y - r.h * 0.5,
            );
            let mut state = wheel.borrow_mut();
            state.velocity = Vec2::ZERO;
            if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                state.transform(anchor, anchor, 1., event.delta.y * 0.08);
            } else {
                state.transform(anchor, anchor, (event.delta.y * 0.12).exp(), 0.);
            }
        });
        style.on_render.append(move |_| {
            let mut state = state.borrow_mut();
            if state.contacts.is_empty() {
                let dt = get_frame_time().min(0.05);
                let decay = (-5. * dt).exp();
                let velocity = state.velocity;
                state.offset += velocity * ((1. - decay) / 5.);
                state.velocity *= decay;
                if state.velocity.length() < 5. {
                    state.velocity = Vec2::ZERO;
                }
            }
            let r = rect.get();
            let center = vec2(r.x + r.w * 0.5, r.y + r.h * 0.5) + state.offset;
            let screen = |world: Vec2| center + rotate(world, state.rotation) * state.scale;
            let extent = HALF_GRID as f32 * CELL;
            for i in -HALF_GRID..=HALF_GRID {
                let n = i as f32 * CELL;
                for (a, b) in [
                    (vec2(n, -extent), vec2(n, extent)),
                    (vec2(-extent, n), vec2(extent, n)),
                ] {
                    let a = screen(a);
                    let b = screen(b);
                    draw_line(a.x, a.y, b.x, b.y, 1., Color::from_rgba(54, 68, 86, 255));
                }
            }
            let aspect = texture.width() / texture.height();
            let image_size = if aspect > 1. {
                vec2(116., 116. / aspect)
            } else {
                vec2(116. * aspect, 116.)
            } * state.scale;
            for y in -HALF_GRID..HALF_GRID {
                for x in -HALF_GRID..HALF_GRID {
                    let p = screen(vec2((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL));
                    if p.x < -CELL * state.scale
                        || p.x > r.w + CELL * state.scale
                        || p.y < -CELL * state.scale
                        || p.y > r.h + CELL * state.scale
                    {
                        continue;
                    }
                    draw_texture_ex(
                        &texture,
                        p.x - image_size.x * 0.5,
                        p.y - image_size.y * 0.5,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(image_size),
                            rotation: state.rotation,
                            pivot: Some(p),
                            ..Default::default()
                        },
                    );
                }
            }
            for contact in &state.contacts {
                draw_circle_lines(
                    contact.position.x,
                    contact.position.y,
                    22.,
                    2.,
                    Color::from_rgba(0, 210, 255, 255),
                );
            }
        });
    })
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - deer map".into(),
            window_width: 1000,
            window_height: 720,
            ..Default::default()
        },
        async {
            let texture = Texture2D::from_file_with_format(
                include_bytes!("../../../deer.png"),
                Some(ImageFormat::Png),
            );
            texture.set_filter(FilterMode::Linear);
            let state = Rc::new(RefCell::new(MapState::default()));
            let mut app = App::new(map_node(texture, state.clone()));
            loop {
                if is_key_pressed(KeyCode::R) {
                    *state.borrow_mut() = MapState::default();
                }
                clear_background(Color::from_rgba(13, 17, 23, 255));
                app.frame();
                draw_rectangle(16., 16., 156., 42., Color::from_rgba(22, 29, 39, 255));
                draw_text(
                    "Deer map",
                    30.,
                    44.,
                    24.,
                    Color::from_rgba(230, 237, 243, 255),
                );
                {
                    let state = state.borrow();
                    let label = format!(
                        "{:.0}%   {:.0}°",
                        state.scale * 100.,
                        state.rotation.to_degrees()
                    );
                    let width = measure_text(&label, None, 20, 1.).width;
                    draw_rectangle(
                        screen_width() - width - 44.,
                        16.,
                        width + 28.,
                        42.,
                        Color::from_rgba(22, 29, 39, 255),
                    );
                    draw_text(
                        label,
                        screen_width() - width - 30.,
                        43.,
                        20.,
                        Color::from_rgba(230, 237, 243, 255),
                    );
                }
                let mouse = Vec2::from(mouse_position());
                for (r, _, label) in controls() {
                    let hovered = r.contains(mouse);
                    draw_rectangle(
                        r.x,
                        r.y,
                        r.w,
                        r.h,
                        if hovered {
                            Color::from_rgba(0, 210, 255, 255)
                        } else {
                            Color::from_rgba(22, 29, 39, 255)
                        },
                    );
                    let color = if hovered {
                        BLACK
                    } else {
                        Color::from_rgba(230, 237, 243, 255)
                    };
                    let size = if label.len() > 1 { 18 } else { 28 };
                    let text = measure_text(label, None, size, 1.);
                    draw_text(
                        label,
                        r.x + (r.w - text.width) * 0.5,
                        r.y + (r.h + text.height) * 0.5,
                        size as f32,
                        color,
                    );
                }
                next_frame().await;
            }
        },
    );
}
