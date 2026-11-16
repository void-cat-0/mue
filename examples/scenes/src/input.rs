//! Observe the complete original Scene batch, independently of Node UI routing.
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods, MouseButton, TouchPhase};
use mue_macroquad::input::InputBatch;

#[derive(Default)]
pub(crate) struct KeyboardCommands {
    pub toggle_pause: bool,
    pub exit: bool,
}

pub(crate) struct RawDown {
    pub x: f32,
    pub y: f32,
    pub touch_time: Option<f64>,
}

#[derive(Default)]
pub(crate) struct RawInputObserver {
    commands: KeyboardCommands,
    pub last_raw_down: Option<RawDown>,
}

impl RawInputObserver {
    pub fn collect(&mut self, input: &InputBatch) -> KeyboardCommands {
        if input.is_interrupted() {
            *self = Self::default();
        }
        // UI has already been dispatched, but it never filters this raw batch.
        // Preserve event order, physical window pixels, and original touch time.
        input.repeat(self);
        std::mem::take(&mut self.commands)
    }
}

impl EventHandler for RawInputObserver {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::P => self.commands.toggle_pause = !self.commands.toggle_pause,
            KeyCode::Escape => self.commands.exit = true,
            _ => {}
        }
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.last_raw_down = Some(RawDown {
                x,
                y,
                touch_time: None, // Mouse Down has no timestamp.
            });
        }
    }

    fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32, time: f64) {
        if phase == TouchPhase::Started {
            self.last_raw_down = Some(RawDown {
                x,
                y,
                touch_time: Some(time),
            });
        }
    }
    // Wheel and unmatched Move/Up remain in the replay; this observer ignores them.
    // Down is observation (including UI/modal), never gameplay hit-testing.
}
