use crate::{
    input::{self, SettingsAction},
    settings::SettingsEditor,
    view::SettingsView,
};
use anyhow::Result;
use macroquad::prelude::draw_rectangle;
use mue_core::signal::Access;
use mue_macroquad::{
    RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

/// Only this Scene receives write access. Changes apply immediately, not via Returned.
pub struct SettingsScene {
    view: SettingsView,
    settings: SettingsEditor,
    commands: Rc<Cell<Option<SettingsAction>>>,
    input_enabled: bool,
}

impl SettingsScene {
    pub fn new(settings: SettingsEditor) -> Self {
        let commands = Rc::new(Cell::new(None));
        let view = SettingsView::new(settings.read(), commands.clone());
        Self {
            view,
            settings,
            commands,
            input_enabled: false,
        }
    }
}

impl Scene for SettingsScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.input_enabled = !ctx.is_transitioning();
        match event {
            SceneEvent::Input(batch) => {
                if let Some(action) = input::settings(batch) {
                    self.commands.set(Some(action));
                }
            }
            SceneEvent::Pause => {
                self.input_enabled = false;
                self.commands.take();
            }
            _ => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.input_enabled = !ctx.is_transitioning();
        let action = self.commands.take();
        if action == Some(SettingsAction::Quit) {
            ctx.exit().without_transition();
            return Ok(());
        }
        if ctx.is_transitioning() {
            return Ok(());
        }
        match action {
            Some(SettingsAction::CycleTheme) => self.settings.toggle_theme(),
            Some(SettingsAction::ToggleMotion) => self.settings.toggle_reduce_motion(),
            Some(SettingsAction::Back) => {
                self.input_enabled = false;
                ctx.pop().without_transition();
            }
            _ => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        roots.push(
            SceneRoot::new(self.view.root(), frame.viewport).input(if self.input_enabled {
                RootInput::Interactive
            } else {
                RootInput::None
            }),
        );
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        let viewport = render.frame.viewport;
        let palette = self.settings.read().theme.get_untracked().palette();
        draw_rectangle(
            viewport.x,
            viewport.y,
            viewport.w,
            viewport.h,
            palette.background.into(),
        );
        render.draw(self.view.root())
    }
}
