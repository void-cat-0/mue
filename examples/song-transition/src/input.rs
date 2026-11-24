//! Keyboard intents are collected locally, then interpreted by the active Scene.
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue_macroquad::input::InputBatch;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibraryAction {
    Open(usize),
    OpenWithDialog(usize),
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
    key: Option<(KeyCode, KeyMods)>,
}
impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, modifiers: KeyMods, repeat: bool) {
        if !repeat {
            self.key = Some((key, modifiers));
        }
    }
}
fn read(input: &InputBatch) -> Option<(KeyCode, KeyMods)> {
    let mut keys = Keys::default();
    input.repeat(&mut keys);
    keys.key
}
fn library_key(key: KeyCode, modifiers: KeyMods) -> Option<LibraryAction> {
    let open_song = if modifiers.shift {
        LibraryAction::OpenWithDialog
    } else {
        LibraryAction::Open
    };
    match key {
        KeyCode::Escape => Some(LibraryAction::Back),
        KeyCode::Q => Some(LibraryAction::Quit),
        KeyCode::S => Some(LibraryAction::Settings),
        KeyCode::Down => Some(LibraryAction::Scroll(100.)),
        KeyCode::Up => Some(LibraryAction::Scroll(-100.)),
        KeyCode::Key1 => Some(open_song(0)),
        KeyCode::Key2 => Some(open_song(1)),
        KeyCode::Key3 => Some(open_song(2)),
        _ => None,
    }
}
pub fn library(input: &InputBatch) -> Option<LibraryAction> {
    read(input).and_then(|(key, modifiers)| library_key(key, modifiers))
}
pub fn details(input: &InputBatch) -> Option<DetailsAction> {
    let (key, _) = read(input)?;
    match key {
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
    read(input).and_then(|(key, _)| settings_key(key))
}
