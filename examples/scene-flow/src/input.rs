//! Edge-triggered keyboard commands, collected separately by each Scene.
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue_macroquad::input::InputBatch;

#[derive(Default)]
pub(crate) struct DetailsKeys {
    pub(crate) back: bool,
    pub(crate) load: bool,
}

impl DetailsKeys {
    pub(crate) fn read(&mut self, input: &InputBatch) {
        // There is no retained pressed/pointer state across batches or interruptions.
        *self = Self::default();
        input.repeat(self);
    }
}

impl EventHandler for DetailsKeys {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, _: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::Escape => self.back = true,
            KeyCode::Space => self.load = true,
            _ => {}
        }
    }
}

#[derive(Default)]
pub(crate) struct PlayFlowKeys {
    pub(crate) back: bool,
    pub(crate) start: bool,
    pub(crate) pause: bool,
    pub(crate) retry: bool,
}

impl PlayFlowKeys {
    pub(crate) fn read(&mut self, input: &InputBatch) {
        *self = Self::default();
        input.repeat(self);
    }
}

impl EventHandler for PlayFlowKeys {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, _: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::Escape => self.back = true,
            KeyCode::Space => self.start = true,
            KeyCode::P => self.pause = true,
            KeyCode::R => self.retry = true,
            _ => {}
        }
    }
}
