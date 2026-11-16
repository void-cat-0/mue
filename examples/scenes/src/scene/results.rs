use std::{cell::Cell, rc::Rc};

use anyhow::Result;
use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot, node::Node,
};

use crate::{
    input::RawInputObserver,
    model::GameResult,
    ui::{self, ResultsAction},
};

// A pure UI Scene, not a replacement GameScene or a handle to its business state.
pub(super) struct ResultsScene {
    root: Node,
    _scope: OwnedScope,
    action: Rc<Cell<ResultsAction>>,
}

impl ResultsScene {
    pub fn new(elapsed: f64) -> Self {
        let scope = OwnedScope::new();
        let action = Rc::new(Cell::new(ResultsAction::None));
        let root = scope.run(|| ui::results(elapsed, &action));
        Self {
            root,
            _scope: scope,
            action,
        }
    }
}

impl Scene for ResultsScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => {} // App sends this once for this owner.
            SceneEvent::Pause => {
                self.action.set(ResultsAction::None);
            }
            SceneEvent::Resume => {}
            SceneEvent::Input(input) => {
                // This observer has no held state; each complete raw batch is local.
                let commands = RawInputObserver::default().collect(input);
                if commands.exit {
                    ctx.exit();
                }
            }
            SceneEvent::Returned(_) => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        match self.action.replace(ResultsAction::None) {
            ResultsAction::Return => {
                ctx.pop_with_result(GameResult::Return);
            }
            ResultsAction::Retry => {
                ctx.pop_with_result(GameResult::Retry);
            }
            ResultsAction::Exit => {
                ctx.exit();
            }
            ResultsAction::None => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        roots.push(SceneRoot::new(*self.root, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        let view = render.frame.viewport;
        draw_rectangle(view.x, view.y, view.w, view.h, BLACK);
        render.draw(*self.root)?;
        Ok(())
    }
}
