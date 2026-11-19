use crate::{
    input::{self, SettingsAction},
    localization::ReadTranslations,
    settings::SettingsEditor,
    view::SettingsView,
};
use macroquad::prelude::draw_rectangle;
use mue_core::signal::Access;
use mue_i18n::I18n;
use mue_macroquad::{
    CallbackResult, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

/// Only this Scene receives write access. Changes apply immediately, not via Returned.
pub struct SettingsScene {
    view: SettingsView,
    settings: SettingsEditor,
    i18n: Rc<I18n>,
    commands: Rc<Cell<Option<SettingsAction>>>,
    input_enabled: bool,
}

impl SettingsScene {
    pub fn new(settings: SettingsEditor, translations: ReadTranslations, i18n: Rc<I18n>) -> Self {
        let commands = Rc::new(Cell::new(None));
        let view = SettingsView::new(settings.read(), translations, commands.clone());
        Self {
            view,
            settings,
            i18n,
            commands,
            input_enabled: false,
        }
    }
}

impl Scene for SettingsScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.input_enabled = !ctx.is_transitioning();
        match event {
            SceneEvent::Input(batch) => match input::settings(batch) {
                Some(SettingsAction::Quit) => {
                    ctx.exit().without_transition();
                }
                Some(action) => self.commands.set(Some(action)),
                None => {}
            },
            SceneEvent::Pause => {
                self.input_enabled = false;
                self.commands.take();
            }
            _ => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.input_enabled = !ctx.is_transitioning();
        let action = self.commands.take();
        if ctx.is_transitioning() {
            return Ok(());
        }
        match action {
            Some(SettingsAction::CycleTheme) => self.settings.toggle_theme(),
            Some(SettingsAction::ToggleMotion) => self.settings.toggle_reduce_motion(),
            Some(SettingsAction::CycleLocale) => {
                // The Scene holds the write capability; this callback uses no ambient UI context.
                let locales = self.i18n.available_locales();
                let current = self.i18n.locale().get_clone_untracked();
                let index = locales
                    .iter()
                    .position(|locale| *locale == current)
                    .unwrap_or(0);
                let next = locales[(index + 1) % locales.len()].clone();
                self.i18n.set_locale(next)?;
            }
            Some(SettingsAction::Back) => {
                self.input_enabled = false;
                ctx.pop().without_transition();
            }
            _ => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(
            SceneRoot::new(self.view.root(), frame.viewport).input(if self.input_enabled {
                RootInput::Interactive
            } else {
                RootInput::None
            }),
        );
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        let viewport = render.frame.viewport;
        let palette = self.settings.read().theme.get_untracked().palette();
        draw_rectangle(
            viewport.x,
            viewport.y,
            viewport.w,
            viewport.h,
            palette.background.into(),
        );
        render.draw(self.view.root())?;
        Ok(())
    }
}
