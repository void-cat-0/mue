//! Translate one input batch into a Scene-local command.
use crate::effects::Effect;
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue_macroquad::input::InputBatch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Action {
    #[default]
    None,
    Open(Effect),
    Back,
    Quit,
}

pub(crate) fn action(batch: &InputBatch) -> Action {
    // Edge-triggered commands have no held-key state to cancel on interruption.
    let mut keys = Keys::default();
    batch.repeat(&mut keys);
    keys.action
}

#[derive(Default)]
struct Keys {
    action: Action,
}

impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        let action = match key {
            KeyCode::Key1 => Action::Open(Effect::Fade),
            KeyCode::Key2 => Action::Open(Effect::Slide),
            KeyCode::Key3 => Action::Open(Effect::Cover),
            KeyCode::Escape => Action::Back,
            KeyCode::Q => Action::Quit,
            _ => return,
        };
        // The last recognized key in this batch wins, just like Scene requests.
        self.action = action;
    }
}
