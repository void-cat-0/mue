//! Application-owned settings; copied handles never extend their lifetime.
//! UI scopes own any palette maps or other values derived from these signals.

use mue_core::{
    scope::OwnedScope,
    signal::{Access, ReadSignal, Signal, signal},
};
use mue_macroquad::color::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: Color::new(0.035, 0.047, 0.074, 1.),
                surface: Color::new(0.066, 0.078, 0.113, 1.),
                selected: Color::new(0.105, 0.125, 0.18, 1.),
                foreground: Color::new(0.94, 0.95, 0.99, 1.),
                muted: Color::new(0.64, 0.69, 0.8, 1.),
                accent: Color::new(0.28, 0.36, 0.7, 1.),
                panel: Color::new(0.045, 0.058, 0.09, 0.94),
            },
            Self::Light => Palette {
                background: Color::new(0.94, 0.95, 0.98, 1.),
                surface: Color::new(0.985, 0.988, 1., 1.),
                selected: Color::new(0.84, 0.88, 0.96, 1.),
                foreground: Color::new(0.1, 0.13, 0.2, 1.),
                muted: Color::new(0.35, 0.4, 0.5, 1.),
                accent: Color::new(0.22, 0.29, 0.64, 1.),
                panel: Color::new(0.98, 0.985, 1., 0.94),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub selected: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub panel: Color,
}

/// The application keeps this owner until all settings-consuming UI is dropped.
pub struct Settings {
    editor: SettingsEditor,
    // Keep the owner last so future fields can be destroyed while signals are live.
    _owner: OwnedScope,
}

impl Settings {
    pub fn new() -> Self {
        let owner = OwnedScope::new();
        let editor = owner.run(|| SettingsEditor {
            theme: signal(Theme::Dark),
            reduce_motion: signal(false),
        });
        Self {
            editor,
            _owner: owner,
        }
    }

    pub fn read(&self) -> ReadSettings {
        self.editor.read()
    }

    pub fn editor(&self) -> SettingsEditor {
        self.editor
    }
}

/// Read-only, non-owning access shared by independently owned Scenes and views.
#[derive(Clone, Copy)]
pub struct ReadSettings {
    pub theme: ReadSignal<Theme>,
    pub reduce_motion: ReadSignal<bool>,
}

impl ReadSettings {
    /// Sample the current preference when deciding whether to start an animation.
    pub fn animations_enabled(self) -> bool {
        !self.reduce_motion.get_untracked()
    }
}

/// Writable, non-owning access; copying an editor never keeps settings alive.
#[derive(Clone, Copy)]
pub struct SettingsEditor {
    theme: Signal<Theme>,
    reduce_motion: Signal<bool>,
}

impl SettingsEditor {
    pub fn read(self) -> ReadSettings {
        ReadSettings {
            theme: *self.theme,
            reduce_motion: *self.reduce_motion,
        }
    }

    pub fn set_theme(self, theme: Theme) {
        self.theme.set(theme);
    }

    pub fn toggle_theme(self) {
        self.set_theme(self.theme.get_untracked().next());
    }

    pub fn set_reduce_motion(self, reduce_motion: bool) {
        self.reduce_motion.set(reduce_motion);
    }

    pub fn toggle_reduce_motion(self) {
        self.set_reduce_motion(!self.reduce_motion.get_untracked());
    }
}
