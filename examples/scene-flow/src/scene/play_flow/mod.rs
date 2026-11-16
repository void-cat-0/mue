use super::scene_fade;
use crate::{
    assets,
    flow::{AttemptResult, Failure, FlowResult, LoadConfig, LoadOutcome, Request, Session},
    input::PlayFlowKeys,
    ui,
};
use anyhow::Result;
use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Default)]
pub(crate) enum Action {
    #[default]
    None,
    Cancel,
    RetryLoad,
    Start,
    TogglePause,
    ExitAttempt,
    Return,
    Retry,
}

/// These are local states of one Scene, not additional Scene-stack entries.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Loading,
    Failure,
    Ready,
    Running,
    Results,
}

pub(crate) struct PlayFlow {
    view: ui::PlayFlowView,
    _scope: OwnedScope,
    action: Rc<Cell<Action>>,
    keys: PlayFlowKeys,
    phase: Phase,
    last_rendered_phase: Option<Phase>,
    input_enabled: bool,
    request: Request,
    session: Option<Rc<Session>>,
    texture: Option<Texture2D>,
    failure: Option<Failure>,
    result: Option<AttemptResult>,
    user_paused: bool,
    lifecycle_paused: bool,
}

impl PlayFlow {
    pub(super) fn new(fail_after_decode: bool) -> Self {
        let scope = OwnedScope::new();
        let action = Rc::new(Cell::new(Action::None));
        let view = scope.run(|| ui::play_flow(action.clone()));
        let mut request = Request::new();
        request.load(assets::demo_resource(), LoadConfig { fail_after_decode });
        let mut scene = Self {
            view,
            _scope: scope,
            action,
            keys: PlayFlowKeys::default(),
            phase: Phase::Loading,
            last_rendered_phase: None,
            input_enabled: false,
            request,
            session: None,
            texture: None,
            failure: None,
            result: None,
            user_paused: false,
            lifecycle_paused: false,
        };
        scene.refresh_view();
        scene
    }

    fn is_presented(&self, ctx: &SceneContext<'_>) -> bool {
        self.last_rendered_phase == Some(self.phase) && !ctx.is_transitioning()
    }

    fn observe_context(&mut self, ctx: &SceneContext<'_>) {
        self.input_enabled = !self.lifecycle_paused && self.is_presented(ctx);
    }

    fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.view.phase.set(phase);
        // The new internal picture must render before subsequent layout enables it.
        self.last_rendered_phase = None;
        self.input_enabled = false;
    }

    fn return_result(&mut self, ctx: &mut SceneContext<'_>) {
        self.action.set(Action::None);
        self.input_enabled = false;
        let outcome = if let Some(result) = self.result.clone() {
            FlowResult::AttemptResult(result)
        } else if let Some(session) = &self.session {
            if let Some(result) = session.exit() {
                FlowResult::AttemptResult(result)
            } else {
                FlowResult::Cancelled {
                    generation_id: session.snapshot().generation_id,
                }
            }
        } else if let Some(failure) = self.failure.clone() {
            FlowResult::Failure(failure)
        } else {
            let generation_id = self
                .request
                .cancel()
                .or_else(|| self.request.generation_id())
                .unwrap_or(0);
            FlowResult::Cancelled { generation_id }
        };
        ctx.pop_with_result(outcome).transition(scene_fade());
    }

    fn accept_loading(&mut self, ctx: &mut SceneContext<'_>) {
        if !self.is_presented(ctx) {
            return;
        }
        match self.request.update() {
            Some(LoadOutcome::Ready(prepared)) => {
                // Only a normal update accepts CPU data and uploads GPU resources.
                let texture =
                    Texture2D::from_rgba8(prepared.width, prepared.height, &prepared.rgba);
                texture.set_filter(FilterMode::Nearest);
                self.texture = Some(texture);
                self.session = Some(Rc::new(Session::new(prepared)));
                self.set_phase(Phase::Ready);
            }
            Some(LoadOutcome::Failure(failure)) => {
                self.failure = Some(failure);
                self.set_phase(Phase::Failure);
            }
            Some(LoadOutcome::Cancelled { generation_id }) => {
                ctx.pop_with_result(FlowResult::Cancelled { generation_id })
                    .transition(scene_fade());
            }
            None => {}
        }
    }

    fn refresh_view(&mut self) {
        self.view.title.set(
            match self.phase {
                Phase::Loading => "Loading CPU resource",
                Phase::Failure => "Loading failed",
                Phase::Ready => "Resource ready — not running",
                Phase::Running => "Running session",
                Phase::Results => "Results",
            }
            .into(),
        );
        let body = match self.phase {
            Phase::Loading => format!(
                "Request {} · CPU progress {:.0}%
Cancel revokes publication; a worker may finish late.
GPU upload waits for a presented Loading picture and a normal update.",
                self.request.generation_id().unwrap_or(0),
                self.request
                    .monitor()
                    .map_or(0., |monitor| monitor.progress())
                    * 100.,
            ),
            Phase::Failure => format!(
                "{}
Failure is a business result. Retry starts a new loading request.",
                self.failure.as_ref().unwrap(),
            ),
            Phase::Ready => {
                let prepared = self.session.as_ref().unwrap().prepared();
                format!(
                    "Generation {} · {}×{} texture · {} points
Source hash {:016x}
Prepared and uploaded does not mean running: press Start.",
                    prepared.generation_id,
                    prepared.width,
                    prepared.height,
                    prepared.points.len(),
                    prepared.source_hash,
                )
            }
            Phase::Running => {
                let snapshot = self.session.as_ref().unwrap().snapshot();
                format!(
                    "Attempt {} · {:.2}/{:.2}s · {:?}
Reached {} trajectory points.
User pause is independent of host suspension.",
                    snapshot.attempt,
                    snapshot.elapsed,
                    snapshot.duration,
                    snapshot.state,
                    snapshot.points_reached,
                )
            }
            Phase::Results => format!(
                "{}
The session is stopped. Retry reuses its CPU/GPU resources.",
                ui::attempt_summary(self.result.as_ref().unwrap()),
            ),
        };
        self.view.body.set(body.into());
    }
}

impl Drop for PlayFlow {
    fn drop(&mut self) {
        if let Some(session) = &self.session {
            session.exit();
        }
    }
}

impl Scene for PlayFlow {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Pause => {
                self.lifecycle_paused = true;
                self.action.set(Action::None);
                if let Some(session) = &self.session {
                    session.pause();
                }
            }
            SceneEvent::Resume => {
                self.lifecycle_paused = false;
                if !self.user_paused
                    && let Some(session) = &self.session
                {
                    session.resume();
                }
            }
            SceneEvent::Input(input) => {
                self.keys.read(input);
                // Back may revoke loading even while the Scene crossfade is active.
                if self.keys.back {
                    self.return_result(ctx);
                    return Ok(());
                }
                if self.is_presented(ctx) {
                    let command = if self.keys.start && self.phase == Phase::Ready {
                        Action::Start
                    } else if self.keys.pause && self.phase == Phase::Running {
                        Action::TogglePause
                    } else if self.keys.retry && self.phase == Phase::Failure {
                        Action::RetryLoad
                    } else if self.keys.retry && self.phase == Phase::Results {
                        Action::Retry
                    } else {
                        Action::None
                    };
                    if !matches!(command, Action::None) {
                        self.action.set(command);
                    }
                }
            }
            SceneEvent::Enter | SceneEvent::Returned(_) => {}
        }
        self.observe_context(ctx);
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.observe_context(ctx);
        let action = self.action.replace(Action::None);
        if self.lifecycle_paused {
            return Ok(());
        }
        if self.input_enabled {
            match action {
                Action::Cancel | Action::Return => {
                    self.return_result(ctx);
                    return Ok(());
                }
                Action::RetryLoad if self.phase == Phase::Failure => {
                    self.request
                        .load(assets::demo_resource(), LoadConfig::default());
                    self.failure = None;
                    self.set_phase(Phase::Loading);
                }
                Action::Start if self.phase == Phase::Ready => {
                    if self.session.as_ref().unwrap().start() {
                        self.user_paused = false;
                        self.set_phase(Phase::Running);
                    }
                }
                Action::Retry if self.phase == Phase::Results => {
                    if self.session.as_ref().unwrap().start() {
                        self.result = None;
                        self.user_paused = false;
                        self.set_phase(Phase::Running);
                    }
                }
                Action::TogglePause if self.phase == Phase::Running => {
                    self.user_paused = !self.user_paused;
                    let session = self.session.as_ref().unwrap();
                    if self.user_paused {
                        session.pause();
                    } else {
                        session.resume();
                    }
                }
                Action::ExitAttempt if self.phase == Phase::Running => {
                    self.result = self.session.as_ref().unwrap().exit();
                    self.set_phase(Phase::Results);
                }
                _ => {}
            }
        }
        if self.phase == Phase::Loading {
            self.accept_loading(ctx);
        }
        if self.phase == Phase::Running
            && self.is_presented(ctx)
            && let Some(result) = self.session.as_ref().unwrap().update(ctx.frame.dt)
        {
            self.result = Some(result);
            self.set_phase(Phase::Results);
        }
        self.refresh_view();
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        let input = if self.input_enabled {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        roots.push(SceneRoot::new(*self.view.root, frame.viewport).input(input));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        ui::paint_base(render);
        let v = render.frame.viewport;
        if let Some(texture) = &self.texture {
            draw_texture_ex(
                texture,
                v.x + v.w * 0.72,
                v.y + v.h * 0.2,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(v.w * 0.2, v.w * 0.2)),
                    ..Default::default()
                },
            );
        }
        if let Some(session) = &self.session {
            let prepared = session.prepared();
            let elapsed = session.snapshot().elapsed;
            for point in &prepared.points {
                let color = Color::from_rgba(
                    point.color[0],
                    point.color[1],
                    point.color[2],
                    point.color[3],
                );
                let radius = if point.at <= elapsed { 9. } else { 4. };
                draw_circle(
                    v.x + v.w * (0.7 + point.x * 0.25),
                    v.y + v.h * (0.55 + point.y * 0.23),
                    radius,
                    color,
                );
            }
        }
        render.draw(*self.view.root)?;
        // Source capture is drawing only, not evidence of a newly presented phase.
        if !render.is_transition_source() {
            self.last_rendered_phase = Some(self.phase);
        }
        Ok(())
    }
}
