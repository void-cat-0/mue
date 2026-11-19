use super::{play_flow::PlayFlow, scene_fade};
use crate::{flow::FlowResult, input::DetailsKeys, ui};
use mue_core::prelude::*;
use mue_macroquad::{
    CallbackResult, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Default)]
pub(crate) enum Action {
    #[default]
    None,
    SelectNext,
    ToggleFilter,
    Load,
    LoadFailure,
    Quit,
}

/// Retained selection, filter, summary and Node ownership; no child-state bridge.
pub(crate) struct Details {
    // Drop the Node tree before disposing its reactive owner.
    view: ui::DetailsView,
    _scope: OwnedScope,
    action: Rc<Cell<Action>>,
    keys: DetailsKeys,
    has_rendered: bool,
    input_enabled: bool,
}

impl Details {
    pub(crate) fn new() -> Self {
        let scope = OwnedScope::new();
        let action = Rc::new(Cell::new(Action::None));
        let view = scope.run(|| ui::details(action.clone()));
        Self {
            view,
            _scope: scope,
            action,
            keys: DetailsKeys::default(),
            has_rendered: false,
            input_enabled: false,
        }
    }

    fn observe_context(&mut self, ctx: &SceneContext<'_>) {
        self.input_enabled = self.has_rendered && !ctx.is_transitioning();
    }
}

#[derive(Debug, thiserror::Error)]
#[error("details expected FlowResult")]
struct UnexpectedFlowResult;

impl Scene for Details {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.observe_context(ctx);
        match event {
            SceneEvent::Returned(value) => {
                let outcome = value
                    .downcast::<FlowResult>()
                    .map_err(|_| UnexpectedFlowResult)?;
                self.view.summary.set(ui::result_summary(&outcome));
                self.has_rendered = false;
                self.input_enabled = false;
            }
            SceneEvent::Input(input) => {
                self.keys.read(input);
                if self.keys.back {
                    self.action.set(Action::None);
                    ctx.pop();
                } else if self.input_enabled && self.keys.load {
                    self.action.set(Action::Load);
                }
            }
            SceneEvent::Pause => {
                self.action.set(Action::None);
                self.input_enabled = false;
            }
            SceneEvent::Enter | SceneEvent::Resume => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.observe_context(ctx);
        let action = self.action.replace(Action::None);
        if !self.input_enabled {
            return Ok(());
        }
        match action {
            Action::SelectNext => self.view.selected.update(|selected| *selected += 1),
            Action::ToggleFilter => self.view.filter.set(!self.view.filter.get()),
            Action::Load | Action::LoadFailure => {
                ctx.push(PlayFlow::new(matches!(action, Action::LoadFailure)))
                    .transition(scene_fade());
                self.input_enabled = false;
            }
            Action::Quit => {
                ctx.pop();
            }
            Action::None => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        let input = if self.input_enabled {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        roots.push(SceneRoot::new(*self.view.root, frame.viewport).input(input));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        ui::paint_base(render);
        render.draw(*self.view.root)?;
        if !render.is_transition_source() {
            self.has_rendered = true;
        }
        Ok(())
    }
}
