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

#[cfg(test)]
mod tests {
    use super::*;
    use mue_core::{
        effect::{computed, on_cleanup, watch_effect},
        scope::{Scope, inject, provide},
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn independent_scenes_share_settings_without_owning_them() {
        let construction = OwnedScope::new();
        let settings = construction.run(Settings::new);
        let read = settings.read();
        let editor = settings.editor();
        drop(construction);
        assert_eq!(read.theme.get(), Theme::Dark);
        assert!(read.animations_enabled());

        let library = OwnedScope::new();
        let details = OwnedScope::new();
        let library_palette = library.run(|| read.theme.map(Theme::palette));
        let details_state =
            details.run(|| computed(move |_| (read.theme.get(), !read.reduce_motion.get())));

        details.run(|| {
            editor.set_theme(Theme::Light);
            editor.set_reduce_motion(true);
        });
        assert_eq!(library_palette.get(), Theme::Light.palette());
        assert_eq!(details_state.get(), (Theme::Light, false));
        assert_eq!(editor.read().theme.get(), read.theme.get());
        assert_eq!(editor.read().reduce_motion.get(), read.reduce_motion.get());

        library.run(|| {
            editor.toggle_theme();
            editor.toggle_reduce_motion();
        });
        assert_eq!(details_state.get(), (Theme::Dark, true));
        drop(library);
        assert_eq!(library_palette.try_get(), None);
        details.run(|| editor.set_theme(Theme::Light));
        assert_eq!(details_state.get(), (Theme::Light, true));
        drop(details);
        assert_eq!(details_state.try_get(), None);
        assert_eq!(read.theme.get(), Theme::Light);
        assert!(read.animations_enabled());
    }

    #[test]
    fn ui_derivations_and_cleanup_are_released_while_settings_stay_live() {
        let settings = Settings::new();
        let read = settings.read();
        let editor = settings.editor();
        let ui = OwnedScope::new();
        let runs = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let producer_capture = Rc::new(());
        let weak_capture = Rc::downgrade(&producer_capture);
        let (palette, motion, local) = ui.run(|| {
            let palette = read.theme.map({
                let producer_capture = producer_capture.clone();
                move |theme| {
                    let _keep_alive = &producer_capture;
                    theme.palette()
                }
            });
            let motion = computed(move |_| !read.reduce_motion.get());
            let local = signal(17u32);
            watch_effect({
                let runs = runs.clone();
                let cleanups = cleanups.clone();
                move || {
                    palette.get();
                    motion.get();
                    runs.set(runs.get() + 1);
                    on_cleanup({
                        let cleanups = cleanups.clone();
                        move || {
                            // Cleanup sees live local and application-owned signals.
                            assert_eq!(local.get(), 17);
                            assert!(read.theme.try_get_untracked().is_some());
                            assert!(read.reduce_motion.try_get_untracked().is_some());
                            cleanups.set(cleanups.get() + 1);
                        }
                    });
                }
            });
            (palette, motion, local)
        });
        drop(producer_capture);
        assert!(weak_capture.upgrade().is_some());
        assert_eq!(runs.get(), 1);
        assert_eq!(cleanups.get(), 0);
        editor.set_theme(Theme::Light);
        assert_eq!(runs.get(), 2);
        assert_eq!(cleanups.get(), 1);

        drop(ui);
        assert_eq!(cleanups.get(), 2);
        assert!(weak_capture.upgrade().is_none());
        assert_eq!(palette.try_get(), None);
        assert_eq!(motion.try_get(), None);
        assert_eq!(local.try_get(), None);
        assert_eq!(read.theme.get(), Theme::Light);
        editor.toggle_theme();
        editor.set_reduce_motion(true);
        assert_eq!(read.theme.get(), Theme::Dark);
        assert!(!read.animations_enabled());
        assert_eq!(runs.get(), 2);
        assert_eq!(cleanups.get(), 2);
    }

    #[test]
    fn dropping_settings_invalidates_copied_and_provided_handles() {
        let settings = Settings::new();
        let read = settings.read();
        let editor = settings.editor();
        let owner_handle = settings._owner.scope();
        let ui = OwnedScope::new();
        ui.provide(read);
        ui.provide(editor);

        drop(settings);
        assert!(!owner_handle.is_alive());
        assert_eq!(read.theme.try_get(), None);
        assert_eq!(read.reduce_motion.try_get(), None);
        assert_eq!(editor.read().theme.try_get(), None);
        assert_eq!(editor.theme.try_set(Theme::Light), Err(Theme::Light));
        assert_eq!(editor.reduce_motion.try_set(true), Err(true));
        let provided_read = ui.inject::<ReadSettings>().unwrap();
        let provided_editor = ui.inject::<SettingsEditor>().unwrap();
        assert_eq!(provided_read.theme.try_get(), None);
        assert_eq!(provided_editor.read().reduce_motion.try_get(), None);
    }

    #[test]
    fn context_lookup_follows_construction_chain_not_call_stack_or_ownership() {
        let settings = Settings::new();
        let read = settings.read();
        let editor = settings.editor();
        let construction = OwnedScope::new();
        construction.run(|| {
            provide(read);
            provide(editor);
        });
        let inherited_ui = construction.run(OwnedScope::new_inherited);
        let detached_ui = construction.run(OwnedScope::new);
        let attached_ui = construction.run(Scope::new_attached);
        let local = inherited_ui.run(|| signal(23u32));
        let captured_read = inherited_ui.run(|| inject::<ReadSettings>().unwrap());
        let captured_editor = inherited_ui.run(|| inject::<SettingsEditor>().unwrap());
        assert!(attached_ui.inject::<ReadSettings>().is_some());
        assert!(settings._owner.inject::<ReadSettings>().is_none());

        let other_settings = Settings::new();
        other_settings.editor().set_theme(Theme::Light);
        let caller = OwnedScope::new();
        caller.provide(other_settings.read());
        assert_eq!(
            caller.run(|| inherited_ui.run(|| inject::<ReadSettings>().unwrap().theme.get())),
            Theme::Dark
        );
        assert!(
            caller
                .run(|| detached_ui.run(inject::<ReadSettings>))
                .is_none()
        );
        assert!(
            construction
                .run(|| detached_ui.run(inject::<SettingsEditor>))
                .is_none()
        );

        drop(construction);
        assert!(!attached_ui.is_alive());
        assert!(inherited_ui.is_alive());
        assert!(detached_ui.is_alive());
        assert!(inherited_ui.inject::<ReadSettings>().is_none());
        assert!(inherited_ui.inject::<SettingsEditor>().is_none());
        assert_eq!(local.get(), 23);
        inherited_ui.run(|| captured_editor.set_theme(Theme::Light));
        assert_eq!(captured_read.theme.get(), Theme::Light);
        drop(inherited_ui);
        assert_eq!(local.try_get(), None);
        assert_eq!(read.theme.get(), Theme::Light);
    }

    #[test]
    fn animation_decisions_do_not_subscribe_the_surrounding_effect() {
        let settings = Settings::new();
        let read = settings.read();
        let ui = OwnedScope::new();
        let runs = Rc::new(Cell::new(0));
        ui.run(|| {
            watch_effect({
                let runs = runs.clone();
                move || {
                    read.animations_enabled();
                    runs.set(runs.get() + 1);
                }
            });
        });
        settings.editor().set_reduce_motion(true);
        assert!(!read.animations_enabled());
        settings.editor().toggle_reduce_motion();
        assert!(read.animations_enabled());
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn palettes_preserve_dark_background_and_white_button_contrast() {
        fn luminance(color: Color) -> f32 {
            fn linear(channel: f32) -> f32 {
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            }
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        }
        fn contrast(a: Color, b: Color) -> f32 {
            let a = luminance(a);
            let b = luminance(b);
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }

        assert_eq!(Theme::default(), Theme::Dark);
        assert_eq!(Theme::Dark.label(), "Dark");
        assert_eq!(Theme::Light.label(), "Light");
        assert_eq!(Theme::Dark.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Dark);
        assert_eq!(
            Theme::Dark.palette().background,
            Color::new(0.035, 0.047, 0.074, 1.)
        );
        let light = Theme::Light.palette();
        assert!(luminance(light.background) > 0.8);
        assert!(luminance(light.surface) > 0.9);
        assert!(luminance(light.foreground) < 0.05);
        for theme in [Theme::Dark, Theme::Light] {
            let palette = theme.palette();
            assert!(contrast(Color::WHITE, palette.accent) >= 4.5);
            assert!(contrast(palette.foreground, palette.background) >= 7.);
            assert!(contrast(palette.foreground, palette.selected) >= 7.);
            assert!(contrast(palette.muted, palette.surface) >= 4.5);
            assert_eq!(palette.accent.a, 1.);
            assert!(palette.panel.a > 0. && palette.panel.a < 1.);
        }
    }
}
