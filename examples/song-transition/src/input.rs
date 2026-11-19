//! Keyboard intents are collected locally, then interpreted by the active Scene.
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue_macroquad::input::InputBatch;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibraryAction {
    Open(usize),
    Scroll(f32),
    Settings,
    Back,
    Quit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsAction {
    Settings,
    Back,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    CycleTheme,
    ToggleMotion,
    CycleLocale,
    Back,
    Quit,
}

#[derive(Default)]
struct Keys {
    key: Option<KeyCode>,
}
impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: KeyMods, repeat: bool) {
        if !repeat {
            self.key = Some(key);
        }
    }
}
fn read(input: &InputBatch) -> Option<KeyCode> {
    let mut keys = Keys::default();
    input.repeat(&mut keys);
    keys.key
}
fn library_key(key: KeyCode) -> Option<LibraryAction> {
    match key {
        KeyCode::Escape => Some(LibraryAction::Back),
        KeyCode::Q => Some(LibraryAction::Quit),
        KeyCode::S => Some(LibraryAction::Settings),
        KeyCode::Down => Some(LibraryAction::Scroll(100.)),
        KeyCode::Up => Some(LibraryAction::Scroll(-100.)),
        KeyCode::Key1 => Some(LibraryAction::Open(0)),
        KeyCode::Key2 => Some(LibraryAction::Open(1)),
        KeyCode::Key3 => Some(LibraryAction::Open(2)),
        _ => None,
    }
}
pub fn library(input: &InputBatch) -> Option<LibraryAction> {
    read(input).and_then(library_key)
}
pub fn details(input: &InputBatch) -> Option<DetailsAction> {
    match read(input)? {
        KeyCode::Escape => Some(DetailsAction::Back),
        KeyCode::Q => Some(DetailsAction::Quit),
        KeyCode::S => Some(DetailsAction::Settings),
        _ => None,
    }
}

fn settings_key(key: KeyCode) -> Option<SettingsAction> {
    match key {
        KeyCode::T => Some(SettingsAction::CycleTheme),
        KeyCode::M => Some(SettingsAction::ToggleMotion),
        KeyCode::L => Some(SettingsAction::CycleLocale),
        KeyCode::Escape => Some(SettingsAction::Back),
        KeyCode::Q => Some(SettingsAction::Quit),
        _ => None,
    }
}

pub fn settings(input: &InputBatch) -> Option<SettingsAction> {
    read(input).and_then(settings_key)
}
