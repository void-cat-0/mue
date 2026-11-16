use std::{cell::Cell, rc::Rc};

use anyhow::{Result, bail};
use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot, math::Rect,
    node::Node,
};

use super::ResultsScene;
use crate::{
    input::RawInputObserver,
    model::{GameModel, GameResult},
    ui::{self, GameAction},
};

pub(crate) struct GameScene {
    // Nodes drop before the detached owner of the UI resources they read.
    hud: Node,
    modal: Node,
    _scope: OwnedScope,
    action: Rc<Cell<GameAction>>,
    raw_input: RawInputObserver,
    model: GameModel,
    // Runtime eligibility (coverage or host suspension), not gameplay pause.
    lifecycle_paused: bool,
}

impl GameScene {
    pub fn new() -> Self {
        let scope = OwnedScope::new();
        let action = Rc::new(Cell::new(GameAction::None));
        let (hud, modal) = scope.run(|| ui::game(&action));
        Self {
            hud,
            modal,
            _scope: scope,
            action,
            raw_input: RawInputObserver::default(),
            model: GameModel::default(),
            lifecycle_paused: false,
        }
    }
}

impl Scene for GameScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            // App sends Enter once. Construction already owns the model and UI;
            // exposing this Scene again must not rebuild or restart either one.
            SceneEvent::Enter => {
                self.raw_input = RawInputObserver::default();
            }
            SceneEvent::Pause => {
                self.lifecycle_paused = true;
                self.raw_input = RawInputObserver::default();
                self.action.set(GameAction::None);
            }
            SceneEvent::Resume => {
                // Regaining runtime eligibility never clears the user's pause.
                self.lifecycle_paused = false;
            }
            SceneEvent::Input(input) => {
                // UI was dispatched first. Apply its pause choice before raw keys,
                // so a same-batch Resume click followed by P still toggles pause.
                match self.action.get() {
                    GameAction::TogglePause => {
                        self.model.toggle_pause();
                        self.action.set(GameAction::None);
                    }
                    GameAction::Resume => {
                        self.model.resume();
                        self.action.set(GameAction::None);
                    }
                    _ => {}
                }
                let commands = self.raw_input.collect(input);
                // Interruption resets only raw input state, not fresh UI commands
                // which App has already dispatched from this same batch.
                if commands.toggle_pause {
                    self.model.toggle_pause();
                }
                if commands.exit {
                    ctx.exit();
                }
            }
            SceneEvent::Returned(result) => {
                let result = match result.downcast::<GameResult>() {
                    Ok(result) => *result,
                    Err(_) => bail!("GameScene expected a GameResult from ResultsScene"),
                };
                // App delivers Returned before Resume. The parent owns this policy;
                // ResultsScene cannot mutate the game through a shared object.
                self.model.apply_result(result);
                if result == GameResult::Retry {
                    self.raw_input = RawInputObserver::default();
                }
            }
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        match self.action.replace(GameAction::None) {
            GameAction::TogglePause => self.model.toggle_pause(),
            GameAction::Resume => self.model.resume(),
            GameAction::Results => {
                // Keep this exact GameScene, its Node trees, and its UI scope.
                ctx.push(ResultsScene::new(self.model.elapsed));
                return Ok(());
            }
            GameAction::Pop => {
                ctx.pop(); // Popping the root leaves App Exited; main stops.
                return Ok(());
            }
            GameAction::Exit => {
                ctx.exit(); // Intentional shutdown of the entire stack.
                return Ok(());
            }
            GameAction::None => {}
        }
        if !self.lifecycle_paused {
            self.model.advance(ctx.frame.dt);
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        // Pure submission: App can call layout twice. No timers, updates, or draws.
        roots.push(SceneRoot::new(
            *self.hud,
            Rect::new(
                frame.viewport.x + 24.,
                frame.viewport.y + 24.,
                (frame.viewport.w - 48.).max(0.),
                84.,
            ),
        ));
        if self.model.user_paused {
            // Blank modal areas block new input to lower Nodes, not Scene rawinput.
            roots.push(SceneRoot::new(*self.modal, frame.viewport).input(RootInput::Modal));
        }
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        let view = render.frame.viewport;
        // Draw inside App's camera/pass, including a captured transition layer.
        draw_rectangle(
            view.x,
            view.y,
            view.w,
            view.h,
            Color::from_rgba(12, 20, 34, 255),
        );
        let center = view.center();
        let x = center.x + (self.model.elapsed as f32 * 1.4).sin() * view.w * 0.28;
        draw_circle(x, center.y, 38., SKYBLUE);
        draw_text("GAMEPLAY / CUSTOM BACKGROUND", 30., 165., 24., LIGHTGRAY);
        draw_text(
            format!("Gameplay time: {:.2}s", self.model.elapsed),
            30.,
            200.,
            24.,
            WHITE,
        );
        draw_text(
            "P: pause / resume     Esc: exit",
            30.,
            view.h - 78.,
            22.,
            LIGHTGRAY,
        );
        if let Some(down) = &self.raw_input.last_raw_down {
            let stamp = down.touch_time.map_or_else(
                || "mouse (no timestamp)".to_owned(),
                |time| format!("touch time={time:.6}"),
            );
            draw_text(
                format!(
                    "Last raw Down (incl. UI/modal): physical ({:.0}, {:.0}), {stamp}",
                    down.x, down.y
                ),
                30.,
                view.h - 42.,
                18.,
                GRAY,
            );
        }

        render.draw(*self.hud)?;

        // Custom foreground comes after the HUD and before the pause modal.
        // The orange band overlaps the bottom of the HUD to show this ordering.
        draw_rectangle(
            view.x + 12.,
            view.y + 101.,
            (view.w - 24.).max(0.),
            8.,
            ORANGE,
        );
        draw_text(
            "CUSTOM FOREGROUND (after HUD, before modal)",
            30.,
            view.h - 120.,
            20.,
            ORANGE,
        );
        if self.model.user_paused {
            render.draw(*self.modal)?;
        }
        Ok(())
    }
}
