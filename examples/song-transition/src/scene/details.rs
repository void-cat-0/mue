use super::SettingsFactory;
use crate::{
    artwork::{self, Surface},
    catalog::Song,
    effects::{self, ReturnTarget},
    input::{self, DetailsAction},
    settings::ReadSettings,
    view::DetailsView,
};
use anyhow::Result;
use mue_macroquad::{
    RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

pub(super) struct DetailsReturn {
    pub index: usize,
    pub target: Option<ReturnTarget>,
}

pub(super) struct DetailsScene {
    index: usize,
    song: Song,
    view: DetailsView,
    settings: ReadSettings,
    open_settings: SettingsFactory,
    commands: Rc<Cell<Option<DetailsAction>>>,
    input_enabled: bool,
}
impl DetailsScene {
    pub fn new(
        index: usize,
        song: Song,
        settings: ReadSettings,
        open_settings: SettingsFactory,
    ) -> Self {
        let commands = Rc::new(Cell::new(None));
        let view = DetailsView::new(&song, settings, commands.clone());
        Self {
            index,
            song,
            view,
            settings,
            open_settings,
            commands,
            input_enabled: false,
        }
    }
}
impl Scene for DetailsScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.input_enabled = !ctx.is_transitioning();
        match event {
            SceneEvent::Input(batch) => {
                if let Some(action) = input::details(batch) {
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
        match self.commands.take() {
            Some(DetailsAction::Quit) => {
                ctx.exit().without_transition();
            }
            Some(DetailsAction::Back) if !ctx.is_transitioning() => {
                self.input_enabled = false;
                let target = self
                    .settings
                    .animations_enabled()
                    .then(ReturnTarget::default);
                let request = ctx.pop_with_result(DetailsReturn {
                    index: self.index,
                    target: target.clone(),
                });
                if let Some(target) = target {
                    request.transition(effects::shrink(self.song.artwork.clone(), target));
                } else {
                    request.without_transition();
                }
            }
            Some(DetailsAction::Settings) if !ctx.is_transitioning() => {
                self.input_enabled = false;
                ctx.push((self.open_settings)()).without_transition();
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
        artwork::draw(
            &self.song.artwork,
            Surface::fullscreen(render.frame.viewport, self.song.artwork.aspect()),
            1.,
        );
        render.draw(self.view.root())
    }
}
