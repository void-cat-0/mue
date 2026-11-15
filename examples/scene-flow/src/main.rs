//! An offline consumer: retained details -> loading -> ready -> run -> results.
//! GPU presentation is host-local; it never retains or updates an old Scene.
mod assets;
mod flow;
mod presentation;

use anyhow::{Result, ensure};
use flow::{
    AttemptResult, EndReason, FlowResult, LoadConfig, LoadMonitor, LoadOutcome, Prepared, Request,
    Session, SessionState,
};
use macroquad::{
    miniquad::{EventHandler, KeyCode, KeyMods, MouseButton, TouchPhase},
    prelude::*,
};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot, font,
    math::Rect as UiRect, node::NodeRef,
};
use presentation::Presenter;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum Action {
    #[default]
    None,
    Load,
    LoadFailure,
    Cancel,
    RetryLoad,
    Start,
    TogglePause,
    ExitAttempt,
    Return,
    Retry,
    Quit,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Details,
    Loading,
    Failure,
    Ready,
    Running,
    Results,
}
impl Phase {
    fn number(self) -> u64 {
        match self {
            Self::Details => 0,
            Self::Loading => 1,
            Self::Failure => 2,
            Self::Ready => 3,
            Self::Running => 4,
            Self::Results => 5,
        }
    }
    fn color(self) -> Color {
        match self {
            Self::Details => Color::from_rgba(35, 95, 170, 255),
            Self::Loading => Color::from_rgba(170, 115, 35, 255),
            Self::Failure => Color::from_rgba(190, 60, 75, 255),
            Self::Ready => Color::from_rgba(40, 175, 130, 255),
            Self::Running => Color::from_rgba(60, 140, 220, 255),
            Self::Results => Color::from_rgba(145, 75, 190, 255),
        }
    }
}
#[derive(Default)]
struct Stats {
    details_enter: usize,
    details_resume: usize,
    details_return: usize,
    flow_enter: usize,
    flow_drop: usize,
    uploads: usize,
    starts: u64,
    stops: u64,
    running: u64,
    attempt: u64,
    generation: u64,
    elapsed: f32,
    user_paused: bool,
    cpu_accepted: usize,
    raw_downs: usize,
    returns: Vec<FlowResult>,
}
struct Shared {
    presenter: RefCell<Presenter>,
    action: Cell<Action>,
    phase: Cell<Phase>,
    key: Cell<u64>,
    next_scene: Cell<u64>,
    stats: RefCell<Stats>,
    monitor: RefCell<Option<Arc<LoadMonitor>>>,
    monitors: RefCell<Vec<Arc<LoadMonitor>>>,
    prepared: RefCell<Option<Weak<Prepared>>>,
    gpu: RefCell<Option<Weak<Texture2D>>>,
    session: RefCell<Option<Weak<Session>>>,
    details_root: Cell<Option<NodeRef>>,
    input_key: Cell<u64>,
    input_enabled: Cell<bool>,
    check: bool,
}
impl Shared {
    fn new(check: bool) -> Rc<Self> {
        Rc::new(Self {
            presenter: RefCell::new(Presenter::new(0.25)),
            action: Cell::new(Action::None),
            phase: Cell::new(Phase::Details),
            key: Cell::new(0),
            next_scene: Cell::new(2),
            stats: RefCell::new(Stats::default()),
            monitor: RefCell::new(None),
            monitors: RefCell::new(Vec::new()),
            prepared: RefCell::new(None),
            gpu: RefCell::new(None),
            session: RefCell::new(None),
            details_root: Cell::new(None),
            input_key: Cell::new(0),
            input_enabled: Cell::new(false),
            check,
        })
    }
    fn ready(&self, key: u64) -> bool {
        self.presenter.borrow().is_ready(key)
    }
    fn register(&self, request: &Request) {
        let monitor = request.monitor().expect("a loaded request has a monitor");
        assert_eq!(request.generation_id(), Some(monitor.generation_id()));
        self.monitors.borrow_mut().push(monitor.clone());
        *self.monitor.borrow_mut() = Some(monitor);
    }
}
struct View {
    // Keep reactive owners alive through the last Node draw and cleanup.
    root: Node,
    _owner: OwnedScope,
    title: Signal<Rc<str>>,
    body: Signal<Rc<str>>,
}
fn controls(shared: Rc<Shared>, label: &'static str, action: Action) -> impl IntoNode {
    button(label).on_activate(move || shared.action.set(action))
}
fn view(shared: Rc<Shared>, phase: Signal<Phase>) -> View {
    let owner = OwnedScope::new();
    let (root, title, body) = owner.run(|| {
        let title = signal(Rc::<str>::from("Preparing"));
        let body = signal(Rc::<str>::from(""));
        let root = flexbox()
            .style(mue_macroquad::style! {
                width: 65%; height: 100%; flex-direction: column;
                padding: 36px; gap: 14px; justify-content: center;
                font-family: "Noto Sans"; font-size: 18px;
            })
            .children((
                text(title).font_size(28.),
                text(body),
                controls(shared.clone(), "Cancel loading / Back", Action::Cancel)
                    .show_if(phase.map(|p| p == Phase::Loading)),
                controls(
                    shared.clone(),
                    "Retry loading successfully",
                    Action::RetryLoad,
                )
                .show_if(phase.map(|p| p == Phase::Failure)),
                controls(shared.clone(), "Start prepared session", Action::Start)
                    .show_if(phase.map(|p| p == Phase::Ready)),
                controls(shared.clone(), "Pause / resume", Action::TogglePause)
                    .show_if(phase.map(|p| p == Phase::Running)),
                controls(shared.clone(), "Stop attempt", Action::ExitAttempt)
                    .show_if(phase.map(|p| p == Phase::Running)),
                controls(
                    shared.clone(),
                    "Retry with the same resources",
                    Action::Retry,
                )
                .show_if(phase.map(|p| p == Phase::Results)),
                controls(shared.clone(), "Return to original details", Action::Return)
                    .show_if(phase.map(|p| p != Phase::Loading)),
                text(
                    "Space: start    P: pause    R: retry    Esc: back
Raw pointer events are observed, not gameplay hit tests.",
                )
                .font_size(14.),
            ))
            .into_node();
        (root, title, body)
    });
    View {
        root,
        _owner: owner,
        title,
        body,
    }
}
#[derive(Default)]
struct Keys {
    back: bool,
    start: bool,
    pause: bool,
    retry: bool,
    downs: usize,
}
impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::Escape => self.back = true,
            KeyCode::Space => self.start = true,
            KeyCode::P => self.pause = true,
            KeyCode::R => self.retry = true,
            _ => {}
        }
    }
    fn mouse_button_down_event(&mut self, button: MouseButton, _: f32, _: f32) {
        if button == MouseButton::Left {
            self.downs += 1;
        }
    }
    fn touch_event(&mut self, phase: TouchPhase, _: u64, _: f32, _: f32, _: f64) {
        if phase == TouchPhase::Started {
            self.downs += 1;
        }
    }
}
fn submit(
    root: NodeRef,
    frame: &SceneFrame,
    key: u64,
    shared: &Shared,
    roots: &mut Vec<SceneRoot>,
) {
    let policy = if shared.ready(key) {
        RootInput::Interactive
    } else {
        RootInput::None
    };
    shared.input_key.set(key);
    shared.input_enabled.set(policy != RootInput::None);
    roots.push(SceneRoot::new(root, frame.viewport).input(policy));
}
fn begin(shared: &Shared, key: u64, phase: Phase, frame: SceneFrame) -> Result<()> {
    shared.key.set(key);
    shared.phase.set(phase);
    shared.presenter.borrow_mut().begin(key, frame.viewport)?;
    clear_background(Color::from_rgba(12, 22, 38, 255));
    let v = frame.viewport;
    // An opaque marker lets the finite check validate fades and texture orientation.
    draw_rectangle(v.w - 60., v.h - 60., 40., 40., phase.color());
    Ok(())
}
struct Details {
    root: Node,
    _scope: OwnedScope,
    selected: Signal<u32>,
    filter: Signal<bool>,
    summary: Signal<Rc<str>>,
    shared: Rc<Shared>,
    before: Option<(u32, bool, NodeRef)>,
}
impl Details {
    fn new(shared: Rc<Shared>) -> Self {
        let scope = OwnedScope::new();
        let (root, selected, filter, summary) = scope.run(|| {
            let selected = signal(7_u32);
            let filter = signal(true);
            let summary = signal(Rc::<str>::from("No child result yet."));
            let root = flexbox()
                .style(mue_macroquad::style! {
                    width: 65%; height: 100%; flex-direction: column; justify-content: center;
                    padding: 36px; gap: 14px; font-family: "Noto Sans"; font-size: 18px;
                })
                .children((
                    text("Resource details").font_size(30.),
                    text(
                        "Offline 3-second trajectory + decoded RGBA texture.
This is a consumer, not a Phira chart/audio engine.",
                    ),
                    text(selected.map(|n| Rc::<str>::from(format!("Selected entry: {n}")))),
                    text(filter.map(|v| Rc::<str>::from(format!("Filter enabled: {v}")))),
                    button("Change selected entry")
                        .on_activate(move || selected.update(|n| *n += 1)),
                    button("Toggle filter").on_activate(move || filter.set(!filter.get())),
                    controls(shared.clone(), "Load resource", Action::Load),
                    controls(
                        shared.clone(),
                        "Load with a business failure",
                        Action::LoadFailure,
                    ),
                    text(summary),
                    controls(shared.clone(), "Exit root Scene", Action::Quit),
                ))
                .into_node();
            (root, selected, filter, summary)
        });
        shared.details_root.set(Some(*root));
        Self {
            root,
            _scope: scope,
            selected,
            filter,
            summary,
            shared,
            before: None,
        }
    }
}
impl Scene for Details {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => self.shared.stats.borrow_mut().details_enter += 1,
            SceneEvent::Resume => self.shared.stats.borrow_mut().details_resume += 1,
            SceneEvent::Returned(value) => {
                let outcome = value
                    .downcast::<FlowResult>()
                    .map_err(|_| anyhow::anyhow!("details expected FlowResult"))?;
                if let Some((selected, filter, root)) = self.before.take() {
                    ensure!(
                        selected == self.selected.get()
                            && filter == self.filter.get()
                            && root == *self.root,
                        "original details selection/filter/Node was not retained"
                    );
                }
                self.summary.set(format!("Returned: {outcome:?}").into());
                let mut stats = self.shared.stats.borrow_mut();
                stats.details_return += 1;
                stats.returns.push(*outcome);
            }
            SceneEvent::Input(input) => {
                let mut keys = Keys::default();
                input.repeat(&mut keys);
                self.shared.stats.borrow_mut().raw_downs += keys.downs;
                if keys.back {
                    ctx.pop();
                } else if self.shared.ready(100) && keys.start {
                    self.shared.action.set(Action::Load);
                }
            }
            SceneEvent::Pause => {}
        }
        Ok(())
    }
    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        match self.shared.action.replace(Action::None) {
            action @ (Action::Load | Action::LoadFailure) => {
                // Change state before the first finite round trip so retention
                // checks are not accidentally passing with constructor defaults.
                if self.shared.check
                    && self.before.is_none()
                    && self.shared.stats.borrow().flow_enter == 0
                {
                    self.selected.set(23);
                    self.filter.set(false);
                }
                self.before = Some((self.selected.get(), self.filter.get(), *self.root));
                ctx.push(PlayFlow::new(
                    self.shared.clone(),
                    action == Action::LoadFailure,
                ));
            }
            Action::Quit => {
                ctx.pop();
            }
            _ => {}
        }
        Ok(())
    }
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        submit(*self.root, frame, 100, &self.shared, roots);
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        begin(&self.shared, 100, Phase::Details, render.frame)?;
        let result = render.draw(*self.root);
        self.shared.presenter.borrow_mut().finish_draw();
        result
    }
}

struct PlayFlow {
    view: View,
    _scope: OwnedScope,
    phase: Signal<Phase>,
    stage: Phase,
    shared: Rc<Shared>,
    id: u64,
    request: Request,
    session: Option<Rc<Session>>,
    texture: Option<Rc<Texture2D>>,
    failure: Option<flow::Failure>,
    result: Option<AttemptResult>,
    user_paused: bool,
    lifecycle_paused: bool,
    loading_elapsed: f32,
}
impl PlayFlow {
    fn new(shared: Rc<Shared>, fail: bool) -> Self {
        let scope = OwnedScope::new();
        let phase = scope.run(|| signal(Phase::Loading));
        let view = view(shared.clone(), phase);
        let id = shared.next_scene.get();
        shared.next_scene.set(id + 1);
        let mut request = Request::new();
        request.load(
            assets::demo_resource(),
            LoadConfig {
                fail_after_decode: fail,
            },
        );
        shared.register(&request);
        Self {
            view,
            _scope: scope,
            phase,
            stage: Phase::Loading,
            shared,
            id,
            request,
            session: None,
            texture: None,
            failure: None,
            result: None,
            user_paused: false,
            lifecycle_paused: false,
            loading_elapsed: 0.,
        }
    }
    fn key(&self) -> u64 {
        self.id * 100 + self.stage.number()
    }
    fn set_phase(&mut self, phase: Phase) {
        self.stage = phase;
        self.phase.set(phase);
    }
    fn snapshot(&self) {
        if let Some(session) = &self.session {
            let snapshot = session.snapshot();
            let mut stats = self.shared.stats.borrow_mut();
            stats.starts = snapshot.start_count;
            stats.stops = snapshot.stop_count;
            stats.running = snapshot.running_count;
            stats.attempt = snapshot.attempt;
            stats.generation = snapshot.generation_id;
            stats.elapsed = snapshot.elapsed;
            stats.user_paused = self.user_paused;
        }
    }
    fn return_result(&mut self, ctx: &mut SceneContext<'_>) {
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
        self.snapshot();
        ctx.pop_with_result(outcome);
    }
    fn publish(&mut self) {
        self.view.title.set(
            match self.stage {
                Phase::Loading => "Loading CPU resource",
                Phase::Failure => "Loading failed",
                Phase::Ready => "Resource ready — not running",
                Phase::Running => "Running session",
                Phase::Results => "Results",
                Phase::Details => unreachable!(),
            }
            .into(),
        );
        let body = match self.stage {
            Phase::Loading => format!(
                "Request {} · CPU progress {:.0}% · finished={}
Cancel revokes publication; a worker may finish late.
GPU upload and start wait for a normal Scene update.",
                self.request.generation_id().unwrap_or(0),
                self.request.monitor().map_or(0., |m| m.progress()) * 100.,
                self.request.is_finished()
            ),
            Phase::Failure => format!(
                "{}
Failure is a business result, not a faulted App.",
                self.failure.as_ref().unwrap()
            ),
            Phase::Ready => {
                let session = self.session.as_ref().unwrap();
                let p = session.prepared();
                format!(
                    "Generation {} · {}×{} texture · {} points
CPU hash {:016x}
Prepared/uploaded does not start gameplay: press Start.",
                    p.generation_id,
                    p.width,
                    p.height,
                    p.points.len(),
                    p.source_hash
                )
            }
            Phase::Running => {
                let s = self.session.as_ref().unwrap().snapshot();
                format!(
                    "Attempt {} · {:.2}/{:.2}s · {:?}
Reached {} points · starts {} / stops {}
Pause is local state, independent of host suspension.",
                    s.attempt,
                    s.elapsed,
                    s.duration,
                    s.state,
                    s.points_reached,
                    s.start_count,
                    s.stop_count
                )
            }
            Phase::Results => format!(
                "{:?}
Session stopped before results. Retry reuses the same CPU/GPU resources.",
                self.result.as_ref().unwrap()
            ),
            Phase::Details => unreachable!(),
        };
        self.view.body.set(body.into());
    }
}
impl Drop for PlayFlow {
    fn drop(&mut self) {
        if let Some(session) = &self.session {
            session.exit();
        }
        self.snapshot();
        self.shared.stats.borrow_mut().flow_drop += 1;
    }
}
impl Scene for PlayFlow {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => {
                self.shared.stats.borrow_mut().flow_enter += 1;
                self.publish();
            }
            SceneEvent::Pause => {
                self.lifecycle_paused = true;
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
                let mut keys = Keys::default();
                input.repeat(&mut keys);
                self.shared.stats.borrow_mut().raw_downs += keys.downs;
                if keys.back {
                    self.return_result(ctx);
                } else if self.shared.ready(self.key()) {
                    if keys.start {
                        self.shared.action.set(Action::Start);
                    }
                    if keys.pause {
                        self.shared.action.set(Action::TogglePause);
                    }
                    if keys.retry {
                        self.shared.action.set(if self.stage == Phase::Failure {
                            Action::RetryLoad
                        } else {
                            Action::Retry
                        });
                    }
                }
            }
            SceneEvent::Returned(_) => {}
        }
        Ok(())
    }
    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        ensure!(!self.lifecycle_paused, "a paused consumer must not update");
        let action = self.shared.action.replace(Action::None);
        match action {
            Action::Cancel | Action::Return => {
                self.return_result(ctx);
                return Ok(());
            }
            Action::RetryLoad if self.stage == Phase::Failure => {
                self.request
                    .load(assets::demo_resource(), LoadConfig::default());
                self.shared.register(&self.request);
                self.failure = None;
                self.loading_elapsed = 0.;
                self.set_phase(Phase::Loading);
                self.publish();
                return Ok(());
            }
            Action::Start if self.stage == Phase::Ready && self.shared.ready(self.key()) => {
                ensure!(
                    self.session.as_ref().unwrap().start(),
                    "prepared session must start once"
                );
                self.user_paused = false;
                self.set_phase(Phase::Running);
            }
            Action::Retry if self.stage == Phase::Results && self.shared.ready(self.key()) => {
                ensure!(
                    self.session.as_ref().unwrap().start(),
                    "stopped session can retry"
                );
                self.result = None;
                self.user_paused = false;
                self.set_phase(Phase::Running);
            }
            Action::TogglePause if self.stage == Phase::Running => {
                self.user_paused = !self.user_paused;
                let session = self.session.as_ref().unwrap();
                if self.user_paused {
                    session.pause();
                } else {
                    session.resume();
                }
            }
            Action::ExitAttempt if self.stage == Phase::Running => {
                self.result = self.session.as_ref().unwrap().exit();
                self.set_phase(Phase::Results);
            }
            Action::Quit => {
                self.return_result(ctx);
                return Ok(());
            }
            _ => {}
        }
        if self.stage == Phase::Loading {
            self.loading_elapsed += ctx.frame.dt;
            if (self.shared.check || self.loading_elapsed >= 0.5)
                && self.shared.ready(self.key())
                && let Some(outcome) = self.request.update()
            {
                match outcome {
                    LoadOutcome::Ready(prepared) => {
                        // All GPU work is explicitly here, never in a worker/effect/paused frame.
                        let texture = Rc::new(Texture2D::from_rgba8(
                            prepared.width,
                            prepared.height,
                            &prepared.rgba,
                        ));
                        texture.set_filter(FilterMode::Nearest);
                        let session = Rc::new(Session::new(prepared.clone()));
                        *self.shared.prepared.borrow_mut() = Some(Rc::downgrade(&prepared));
                        *self.shared.gpu.borrow_mut() = Some(Rc::downgrade(&texture));
                        *self.shared.session.borrow_mut() = Some(Rc::downgrade(&session));
                        {
                            let mut stats = self.shared.stats.borrow_mut();
                            stats.uploads += 1;
                            stats.cpu_accepted += 1;
                        }
                        self.texture = Some(texture);
                        self.session = Some(session);
                        self.set_phase(Phase::Ready);
                    }
                    LoadOutcome::Failure(failure) => {
                        self.failure = Some(failure);
                        self.set_phase(Phase::Failure);
                    }
                    LoadOutcome::Cancelled { generation_id } => {
                        ctx.pop_with_result(FlowResult::Cancelled { generation_id });
                        return Ok(());
                    }
                }
            }
        }
        if self.stage == Phase::Running
            && let Some(result) =
                self.session
                    .as_ref()
                    .unwrap()
                    .update(if self.shared.ready(self.key()) {
                        ctx.frame.dt
                    } else {
                        0.
                    })
        {
            self.result = Some(result);
            self.set_phase(Phase::Results);
        }
        self.snapshot();
        self.publish();
        Ok(())
    }
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        submit(*self.view.root, frame, self.key(), &self.shared, roots);
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        begin(&self.shared, self.key(), self.stage, render.frame)?;
        let v = render.frame.viewport;
        if let Some(texture) = &self.texture {
            draw_texture_ex(
                texture,
                v.w * 0.72,
                v.h * 0.2,
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
                    v.w * (0.7 + point.x * 0.25),
                    v.h * (0.55 + point.y * 0.23),
                    radius,
                    color,
                );
            }
        }
        let result = render.draw(*self.view.root);
        self.shared.presenter.borrow_mut().finish_draw();
        result
    }
}

fn has_ui_hit(app: &App, v: UiRect) -> bool {
    // Query actual prepared Node geometry without injecting a native event.
    (0..16).any(|y| {
        (0..16).any(|x| {
            app.hit_test_ui(mue_macroquad::math::vec2(
                v.w * (x as f32 + 0.5) / 16.,
                v.h * (y as f32 + 0.5) / 16.,
            ))
        })
    })
}
fn sample_marker(v: UiRect) -> Color {
    let image = get_screen_data();
    let x = ((v.w - 40.) / v.w * f32::from(image.width)) as u32;
    let y = ((v.h - 40.) / v.h * f32::from(image.height)) as u32;
    image.get_pixel(x, u32::from(image.height) - 1 - y)
}
fn close(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.04
        && (a.g - b.g).abs() < 0.04
        && (a.b - b.b).abs() < 0.04
        && (a.a - b.a).abs() < 0.04
}
struct Check {
    step: u8,
    suspended: u32,
    paused_at: f32,
    paused_checked: usize,
    previous: Option<Color>,
    frozen: Color,
    key: u64,
    fade_samples: usize,
    frames: u32,
    retry_prepared: Option<Weak<Prepared>>,
    retry_gpu: Option<Weak<Texture2D>>,
    retry_session: Option<Weak<Session>>,
    viewport: Option<UiRect>,
    resize_requested: bool,
    resized: bool,
    frozen_progress: Option<f32>,
    input_queries: usize,
}
impl Check {
    fn new() -> Self {
        Self {
            step: 0,
            suspended: 0,
            paused_at: 0.,
            paused_checked: 0,
            previous: None,
            frozen: BLACK,
            key: 0,
            fade_samples: 0,
            frames: 0,
            retry_prepared: None,
            retry_gpu: None,
            retry_session: None,
            viewport: None,
            resize_requested: false,
            resized: false,
            frozen_progress: None,
            input_queries: 0,
        }
    }
    fn should_pause(&mut self, shared: &Shared) -> bool {
        let pausing = (self.step == 6 && shared.phase.get() == Phase::Loading)
            || (self.step == 9 && shared.stats.borrow().user_paused && self.suspended < 2);
        if pausing {
            if let Some(progress) = self.frozen_progress {
                assert_eq!(
                    shared.presenter.borrow().progress(),
                    progress,
                    "host suspension cannot advance a picture fade"
                );
            } else {
                self.frozen_progress = Some(shared.presenter.borrow().progress());
            }
        } else {
            self.frozen_progress = None;
        }
        if self.step == 6 && shared.phase.get() == Phase::Loading {
            self.suspended += 1;
            self.paused_checked += 1;
            let stats = shared.stats.borrow();
            assert_eq!(stats.uploads, 0);
            assert_eq!(stats.starts, 0);
            assert_eq!(stats.cpu_accepted, 0);
            assert!(!shared.monitor.borrow().as_ref().unwrap().is_accepted());
            if self.suspended >= 3
                && shared
                    .monitor
                    .borrow()
                    .as_ref()
                    .is_some_and(|m| m.is_finished())
            {
                self.step = 7;
                self.suspended = 0;
            }
            return true;
        }
        if self.step == 9 && shared.stats.borrow().user_paused && self.suspended < 2 {
            self.suspended += 1;
            self.paused_checked += 1;
            return true;
        }
        false
    }
    fn frame(&mut self, shared: &Shared, app: &App, v: UiRect) -> Result<()> {
        self.frames += 1;
        ensure!(self.frames < 1500, "bounded flow check did not finish");
        let phase = shared.phase.get();
        let key = shared.key.get();
        let p = shared.presenter.borrow().progress();
        let pixel = sample_marker(v);
        if self.viewport.is_some_and(|old| old != v) {
            self.resized = true;
            self.previous = None;
            ensure!(
                p == 1. && shared.ready(key),
                "resize rebuild must discard only obsolete picture caches"
            );
        }
        self.viewport = Some(v);
        if self.key != key || self.previous.is_none() {
            self.key = key;
            self.frozen = self.previous.unwrap_or(phase.color());
        }
        let target = phase.color();
        let expected = Color::new(
            target.r * p + self.frozen.r * (1. - p),
            target.g * p + self.frozen.g * (1. - p),
            target.b * p + self.frozen.b * (1. - p),
            1.,
        );
        ensure!(
            close(pixel, expected),
            "crossfade/orientation pixel mismatch: actual {pixel:?}, expected {expected:?}, progress {p}"
        );
        if phase == Phase::Ready && p < 1. {
            ensure!(
                !has_ui_hit(app, v),
                "an unpresented Ready view must not acquire Node input"
            );
            self.input_queries += 1;
        }
        if p < 1. {
            ensure!(
                shared.presenter.borrow().is_transitioning() && !shared.ready(key),
                "a partially presented scene must not be ready for Node input"
            );
        }
        if p > 0. && p < 1. {
            self.fade_samples += 1;
        }
        self.previous = Some(pixel);
        // Cancel after a real nonzero fade step, while publication is still
        // waiting for the Loading picture to settle. Freeze that displayed blend.
        if self.step == 1 && phase == Phase::Loading && p > 0. && p < 1. {
            shared.action.set(Action::Cancel);
            self.step = 2;
            return Ok(());
        }
        if !shared.ready(key) {
            return Ok(());
        }
        match self.step {
            0 if phase == Phase::Details => {
                shared.action.set(Action::Load);
                self.step = 1;
            }
            2 if phase == Phase::Details => {
                ensure!(
                    matches!(
                        shared.stats.borrow().returns.as_slice(),
                        [FlowResult::Cancelled { .. }]
                    ),
                    "cancel must return once"
                );
                shared.action.set(Action::LoadFailure);
                self.step = 3;
            }
            3 if phase == Phase::Failure => {
                shared.action.set(Action::Return);
                self.step = 4;
            }
            4 if phase == Phase::Details => {
                ensure!(
                    matches!(
                        shared.stats.borrow().returns.as_slice(),
                        [_, FlowResult::Failure(_)]
                    ),
                    "failure must return as a business value"
                );
                shared.action.set(Action::LoadFailure);
                self.step = 5;
            }
            5 if phase == Phase::Failure => {
                shared.action.set(Action::RetryLoad);
                self.step = 6;
            }
            7 if phase == Phase::Ready => {
                if !self.resize_requested {
                    self.resize_requested = true;
                    request_new_screen_size(v.w + 80., v.h);
                    return Ok(());
                }
                if !self.resized {
                    return Ok(());
                }
                let hit = has_ui_hit(app, v);
                ensure!(
                    hit == shared.input_enabled.get(),
                    "Node input eligibility must follow the last submitted/drawn root policy"
                );
                self.input_queries += 1;
                if !hit {
                    return Ok(());
                }
                let stats = shared.stats.borrow();
                ensure!(
                    stats.uploads == 1 && stats.starts == 0,
                    "ready must upload without starting"
                );
                ensure!(
                    shared
                        .session
                        .borrow()
                        .as_ref()
                        .and_then(Weak::upgrade)
                        .is_some_and(|s| s.snapshot().state == SessionState::Ready),
                    "ready resource has no active session yet"
                );
                shared.action.set(Action::Start);
                self.step = 8;
            }
            8 if phase == Phase::Running => {
                shared.action.set(Action::TogglePause);
                self.step = 9;
                self.paused_at = shared.stats.borrow().elapsed;
            }
            9 if phase == Phase::Running && self.suspended == 2 => {
                let stats = shared.stats.borrow();
                ensure!(
                    stats.user_paused && stats.elapsed == self.paused_at,
                    "host resume must not clear local pause or integrate the gap"
                );
                shared.action.set(Action::TogglePause);
                self.step = 10;
            }
            10 if phase == Phase::Results => {
                let stats = shared.stats.borrow();
                ensure!(
                    stats.starts == 1 && stats.stops == 1 && stats.running == 0,
                    "natural completion must stop once"
                );
                let session = shared
                    .session
                    .borrow()
                    .as_ref()
                    .and_then(Weak::upgrade)
                    .unwrap();
                let complete = session.snapshot().result.unwrap();
                ensure!(
                    complete.reason == EndReason::Completed
                        && complete.points_reached == session.prepared().points.len(),
                    "completion must consume every decoded trajectory point"
                );
                self.retry_prepared = shared.prepared.borrow().clone();
                self.retry_gpu = shared.gpu.borrow().clone();
                self.retry_session = shared.session.borrow().clone();
                shared.action.set(Action::Retry);
                self.step = 11;
            }
            11 if phase == Phase::Running && shared.stats.borrow().elapsed > 0.1 => {
                let stats = shared.stats.borrow();
                ensure!(
                    stats.attempt == 2 && stats.uploads == 1,
                    "retry must reuse CPU/GPU resources"
                );
                ensure!(
                    self.retry_prepared
                        .as_ref()
                        .zip(shared.prepared.borrow().as_ref())
                        .is_some_and(|(a, b)| Weak::ptr_eq(a, b))
                        && self
                            .retry_gpu
                            .as_ref()
                            .zip(shared.gpu.borrow().as_ref())
                            .is_some_and(|(a, b)| Weak::ptr_eq(a, b))
                        && self
                            .retry_session
                            .as_ref()
                            .zip(shared.session.borrow().as_ref())
                            .is_some_and(|(a, b)| Weak::ptr_eq(a, b)),
                    "retry must retain the same CPU, GPU and session allocations"
                );
                shared.action.set(Action::ExitAttempt);
                self.step = 12;
            }
            12 if phase == Phase::Results => {
                shared.action.set(Action::Return);
                self.step = 13;
            }
            13 if phase == Phase::Details => {
                let stats = shared.stats.borrow();
                ensure!(
                    stats.details_enter == 1
                        && stats.details_return == 3
                        && stats.details_resume == 3,
                    "details must retain its instance across all returns"
                );
                ensure!(
                    stats.starts == 2
                        && stats.stops == 2
                        && stats.running == 0
                        && stats.uploads == 1,
                    "attempt/resource totals mismatch"
                );
                ensure!(
                    matches!(
                        stats.returns.last(),
                        Some(FlowResult::AttemptResult(AttemptResult {
                            reason: EndReason::Exited,
                            attempt: 2,
                            ..
                        }))
                    ),
                    "final result must describe exited retry"
                );
                ensure!(
                    shared
                        .prepared
                        .borrow()
                        .as_ref()
                        .is_some_and(|w| w.upgrade().is_none())
                        && shared
                            .gpu
                            .borrow()
                            .as_ref()
                            .is_some_and(|w| w.upgrade().is_none())
                        && shared
                            .session
                            .borrow()
                            .as_ref()
                            .is_some_and(|w| w.upgrade().is_none()),
                    "returned flow resources must be released despite retained transition pictures"
                );
                shared.action.set(Action::Quit);
                self.step = 14;
            }
            _ => {}
        }
        Ok(())
    }
    fn finish(&self, shared: &Shared, app: &App) -> Result<()> {
        ensure!(
            self.step == 14 && app.scene_count() == 0 && app.is_exited(),
            "root pop must finish the App"
        );
        ensure!(
            self.fade_samples >= 10
                && self.paused_checked >= 5
                && self.resized
                && self.input_queries >= 2,
            "fade and host suspension coverage missing"
        );
        let monitors = shared.monitors.borrow();
        ensure!(
            monitors.len() == 4,
            "cancel/failure/retry should create four requests"
        );
        ensure!(
            monitors[0].is_cancelled() && !monitors[0].is_accepted(),
            "cancelled work must never be accepted"
        );
        ensure!(
            monitors[0].is_finished(),
            "cancelled CPU work must be observed finished without revival"
        );
        ensure!(
            shared.stats.borrow().flow_enter == 3 && shared.stats.borrow().flow_drop == 3,
            "child flow instances enter and drop exactly once"
        );
        ensure!(
            monitors.iter().filter(|m| m.is_accepted()).count() == 1,
            "only the successful generation is handed off"
        );
        println!(
            "check: details -> cancel/failure -> retry loading -> ready -> run/complete -> retry/exit -> retained details passed"
        );
        println!(
            "check: CPU worker/normal-frame GPU handoff, suspended completion, explicit starts=2/stops=2, uploads=1 and resource release passed"
        );
        println!(
            "check: crossfade GPU pixels ({} samples), resize, host pause ({} frames), original details and empty-root exit passed; no Phira/audio/device-input claim",
            self.fade_samples, self.paused_checked
        );
        Ok(())
    }
}
async fn run(check: bool) -> Result<()> {
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans");
    font::initialize("en-US", db)?;
    let shared = Shared::new(check);
    let mut app = App::with_scene(Details::new(shared.clone()));
    let mut script = check.then(Check::new);
    let start = get_time();
    let mut resumed = false;
    while !app.is_exited() {
        ensure!(
            !check || get_time() - start < 15.,
            "finite flow script exceeded its deadline"
        );
        let paused = script.as_mut().is_some_and(|s| s.should_pause(&shared));
        if paused {
            app.try_frame_paused()?;
            resumed = true;
        } else {
            app.try_frame()?;
            if !app.is_exited() {
                let v = UiRect::new(0., 0., screen_width(), screen_height());
                let dt = if resumed {
                    0.
                } else if check {
                    0.08
                } else {
                    get_frame_time()
                };
                resumed = false;
                shared.presenter.borrow_mut().present(dt, v)?;
                if let Some(script) = &mut script {
                    script.frame(&shared, &app, v)?;
                }
            }
        }
        if !app.is_exited() {
            next_frame().await;
        }
    }
    if let Some(script) = script {
        script.finish(&shared, &app)?;
    }
    Ok(())
}
fn main() {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Scene flow".into(),
            window_width: 960,
            window_height: 640,
            ..Default::default()
        },
        async move {
            run(check).await.expect("Scene flow failed");
        },
    );
}
