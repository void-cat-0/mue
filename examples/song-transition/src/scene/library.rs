use super::{
    SettingsFactory,
    details::{DetailsReturn, DetailsScene},
};
use crate::{
    catalog::Catalog,
    effects::{self, ReturnTarget},
    input::{self, LibraryAction},
    localization::ReadTranslations,
    settings::ReadSettings,
    toast::{Notice, Notices},
    view::LibraryView,
};
use macroquad::prelude::draw_rectangle;
use mue_components::{Dialog, DialogResult, DialogSender};
use mue_core::signal::Access;
use mue_macroquad::{
    CallbackResult, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

pub struct LibraryScene {
    view: LibraryView,
    catalog: Rc<Catalog>,
    settings: ReadSettings,
    translations: ReadTranslations,
    open_settings: SettingsFactory,
    commands: Rc<Cell<Option<LibraryAction>>>,
    notices: Notices,
    dialogs: DialogSender,
    input_enabled: bool,
    returning: Option<(usize, ReturnTarget)>,
}
impl LibraryScene {
    pub fn new(
        catalog: Rc<Catalog>,
        settings: ReadSettings,
        translations: ReadTranslations,
        open_settings: SettingsFactory,
        notices: Notices,
        dialogs: DialogSender,
    ) -> Self {
        let commands = Rc::new(Cell::new(None));
        let view = LibraryView::new(
            catalog.songs(),
            settings,
            translations.clone(),
            commands.clone(),
        );
        Self {
            view,
            catalog,
            settings,
            translations,
            open_settings,
            commands,
            notices,
            dialogs,
            input_enabled: false,
            returning: None,
        }
    }
    fn open(&mut self, index: usize, show_dialog: bool, ctx: &mut SceneContext<'_>) {
        let Some(anchor) = self.view.displayed_anchor(index) else {
            self.notices.show(Notice::SongOffscreen);
            return;
        };
        let Some(song) = self.catalog.songs().get(index).cloned() else {
            return;
        };
        self.input_enabled = false;
        self.view.select(index);
        self.notices.show(Notice::OpeningSong(song.title));
        if show_dialog {
            self.show_song_dialog(song.title);
        }
        let request = ctx.push(DetailsScene::new(
            index,
            song.clone(),
            self.settings,
            self.translations.clone(),
            self.open_settings.clone(),
        ));
        if self.settings.animations_enabled() {
            request.transition(effects::expand(song.artwork, anchor));
        } else {
            request.without_transition();
        }
    }

    fn show_song_dialog(&self, song: &'static str) {
        let common = &self.translations.common;
        let mut args = mue_i18n::FluentArgs::new();
        args.set("song", song);
        let title = common.format("dialog-song-title", Some(&args)).text;
        let message = common.format("dialog-song-body", None).text;
        let palette = self.settings.theme.get_untracked().palette();
        let notices = self.notices.clone();
        let dialog = Dialog::new(title, message)
            .buttons([
                common.format("dialog-dismiss", None).text,
                common.format("dialog-confirm", None).text,
            ])
            .colors(palette.surface, palette.foreground);
        self.dialogs.show(dialog, move |result| {
            if result == DialogResult::Button(1) {
                notices.show(Notice::DialogAcknowledged);
            }
        });
    }
}
#[derive(Debug, thiserror::Error)]
#[error("unexpected details result")]
struct UnexpectedDetailsResult;

impl Scene for LibraryScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.input_enabled = !ctx.is_transitioning();
        match event {
            SceneEvent::Input(_) if ctx.is_modal_open() => {}
            SceneEvent::Input(batch) => match input::library(batch) {
                Some(LibraryAction::Quit) => {
                    ctx.exit().without_transition();
                }
                Some(action) => self.commands.set(Some(action)),
                None => {}
            },
            SceneEvent::Returned(result) => {
                let result = result
                    .downcast::<DetailsReturn>()
                    .map_err(|_| UnexpectedDetailsResult)?;
                self.view.select(result.index);
                self.returning = result.target.map(|target| (result.index, target));
            }
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
        if !ctx.is_transitioning() {
            self.returning = None;
        }
        let action = self.commands.take();
        if ctx.is_transitioning() {
            return Ok(());
        }
        match action {
            Some(LibraryAction::Open(index)) => self.open(index, false, ctx),
            Some(LibraryAction::OpenWithDialog(index)) => self.open(index, true, ctx),
            Some(LibraryAction::Scroll(delta)) => self.view.scroll(delta),
            Some(LibraryAction::Settings) => {
                self.input_enabled = false;
                ctx.push((self.open_settings)()).without_transition();
            }
            Some(LibraryAction::Back) => {
                ctx.pop().without_transition();
            }
            _ => {}
        }
        Ok(())
    }
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        let input = if self.input_enabled {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        self.view.layout(frame.viewport, input, roots);
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        let v = render.frame.viewport;
        draw_rectangle(
            v.x,
            v.y,
            v.w,
            v.h,
            self.settings
                .theme
                .get_untracked()
                .palette()
                .background
                .into(),
        );
        self.view.draw(render, self.catalog.songs())?;
        if !render.is_transition_source()
            && let Some((index, target)) = &self.returning
        {
            target.update(self.view.displayed_anchor(*index));
        }
        Ok(())
    }
}
