//! Frame scheduler and retained Scene-stack owner.
//!
//! Construct [`App`] only after macroquad has initialized. A normal host loop
//! calls [`App::try_frame`] (or [`App::frame`]) once, then awaits
//! [`macroquad::window::next_frame`]. `App` handles input, Scene callbacks, Node
//! updates, layout, drawing, and pending Scene requests; it does not own or end
//! the host loop. Use [`App::with_scene`] for a custom [`Scene`], or [`App::new`]
//! for a single Node tree.
//!
//! Stack methods register deferred [`SceneRequest`]s. A normal request commits
//! after the current input/global/host work and before layout/drawing. The top
//! Scene alone is scheduled; covered instances retain their state and resources.
//! For lifecycle and raw-input details, see the [`scene`] module. Optional
//! draw-only effects are described by [`SceneTransition`].
//!
//! External renderers should finish their target draws, close raw passes, and
//! restore the default camera before the App frame. [`crate::RenderStateGuard`]
//! restores material, draw mode, scissor, and viewport, not camera/pass/depth or
//! arbitrary backend state. [`crate::node::portal`](fn@crate::node::portal) displays a texture published
//! by the host; it does not invoke the renderer.

// Application runtime: the App, scenes, timers, the viewport, and errors.
pub mod error;
pub mod runtime;
pub mod scene;
pub mod scene_transition;
pub mod viewport;

use crate::{
    AppError, CallbackError, CallbackResult, Result, SceneError,
    app::runtime::{Runtime, TimeoutKey, get_time},
    app::scene::{
        NodeScene, Scene, SceneCommand, SceneContext, SceneEvent, SceneFrame, SceneRender,
        SceneRequest, SceneRequestData, SceneResult, SceneRoot, TransitionChoice,
    },
    app::scene_transition::{
        PhaseClock, PictureCache, SceneTransition, TransitionFrame, TransitionPhase,
        TransitionTiming,
    },
    input::InputBatch,
    input::pointer::PointerManager,
    math::{Rect, Vector},
    motion::transition,
    node::{GeometrySnapshot, IntoNode, NodeRef},
};
use mue_core::{batch, task};
use ordered_float::NotNan;
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    mem,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneState {
    New,
    Running,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppState {
    Active,
    Faulted,
    Exited,
}

struct SceneEntry {
    // Return values may refer to receiver-owned handles: drop them before owner.
    pending_results: VecDeque<SceneResult>,
    scene: Box<dyn Scene>,
    state: SceneState,
}
impl SceneEntry {
    fn new(scene: Box<dyn Scene>) -> Self {
        Self {
            scene,
            state: SceneState::New,
            pending_results: VecDeque::new(),
        }
    }
}

struct ActiveTransition {
    // Drop visual captures before pending destinations/results, then source owners.
    effect: SceneTransition,
    pending: Option<SceneCommand>,
    clock: PhaseClock,
    sample: TransitionFrame,
}
impl ActiveTransition {
    fn new(effect: SceneTransition, pending: Option<SceneCommand>, viewport: Rect) -> Self {
        let clock = PhaseClock::new(effect.timing());
        let sample = clock.frame(0., viewport);
        Self {
            effect,
            pending,
            clock,
            sample,
        }
    }

    fn ready_to_switch(&self) -> bool {
        self.pending.is_some()
            && (self.clock.phase() == TransitionPhase::AfterSwitch || self.clock.ready_to_switch())
    }
}

/// Retains a Scene stack and schedules its top for each normal frame.
///
/// Covered instances keep their resources, but receive no ordinary Scene or Node
/// updates. Global work runs once per application frame, not once per retained
/// Scene. Use [`App::new`] for the Node-only entry point or [`App::with_scene`]
/// to supply a custom [`Scene`].
pub struct App {
    scenes: Vec<SceneEntry>,
    state: AppState,
    // Coverage is determined by stack position; host suspension is independent.
    host_paused: bool,
    last_frame_time: Option<f64>,
    command: Option<SceneRequestData>,
    active_transition: Option<ActiveTransition>,
    default_transition: Option<Box<dyn FnMut() -> SceneTransition>>,
    pictures: PictureCache,
    displayed: Vec<SceneRoot>,
    layout: SceneLayout,
    pointer_manager: PointerManager,
    opaque_window: bool,
}

impl Drop for App {
    fn drop(&mut self) {
        // A queued child can borrow retained parent handles without owning them.
        drop(self.command.take());
        drop(self.default_transition.take());
        self.cancel_transition();
        self.drop_scenes();
    }
}

fn batched_result(
    phase: &'static str,
    f: impl FnOnce() -> CallbackResult<()>,
) -> std::result::Result<(), CallbackError> {
    let mut result = Ok(());
    batch(|| result = f().map_err(|error| CallbackError::new(phase, error)));
    result
}

fn window_viewport() -> Rect {
    Rect::new(
        0.,
        0.,
        macroquad::window::screen_width(),
        macroquad::window::screen_height(),
    )
}

fn drawable(viewport: Rect) -> bool {
    viewport.size().is_finite() && viewport.w > 0. && viewport.h > 0.
}

impl App {
    fn initialize_host() -> Rect {
        crate::set_task_wake_callback(Some(Arc::new(miniquad::window::schedule_update)));
        crate::input::text_input::set_host_available();
        let viewport = window_viewport();
        Self::publish_viewport(viewport);
        viewport
    }

    fn publish_viewport(viewport: Rect) {
        crate::app::viewport::set(crate::app::viewport::Viewport {
            width: viewport.w,
            height: viewport.h,
            dpi_scale: macroquad::window::screen_dpi_scale(),
        });
    }

    /// Build and retain one Node tree as a Scene.
    ///
    /// The root is updated, laid out, and drawn by the same scheduler used by
    /// [`App::with_scene`]. It is wrapped in a [`layer_host`](crate::overlay::layer::layer_host),
    /// so [`teleport`](crate::overlay::layer::teleport) works anywhere in the tree. The
    /// macroquad host must already be initialized.
    pub fn new(root_node: impl IntoNode + 'static) -> Self {
        let viewport = Self::initialize_host();
        let root = crate::overlay::layer::layer_host(root_node).into_node();
        root.resolve_font_sizes();
        let root_ref = *root;
        let mut app = Self::from_scene(NodeScene { root });
        // Preserve explicit custom hit bounds before the first Node layout.
        app.displayed.push(SceneRoot::new(root_ref, viewport));
        app
    }

    /// Own a [`Scene`]. Its [`SceneEvent::Enter`] notification runs once on its
    /// first normal frame. Scene UI starts receiving new input after its first
    /// successful drawing; raw Scene input is delivered independently.
    pub fn with_scene(scene: impl Scene + 'static) -> Self {
        Self::initialize_host();
        Self::from_scene(scene)
    }

    fn from_scene(scene: impl Scene + 'static) -> Self {
        Self {
            scenes: vec![SceneEntry::new(Box::new(scene))],
            state: AppState::Active,
            host_paused: false,
            last_frame_time: None,
            command: None,
            active_transition: None,
            default_transition: None,
            pictures: PictureCache::new(),
            displayed: Vec::new(),
            layout: SceneLayout::default(),
            pointer_manager: PointerManager::new(),
            opaque_window: true,
        }
    }

    /// Whether each frame ends by forcing the window's alpha channel to 1
    /// (default true). Turn it off for a host that renders into a deliberately
    /// transparent surface. See [`crate::paint::render::restore_opaque_alpha`].
    pub fn set_opaque_window(&mut self, opaque: bool) {
        self.opaque_window = opaque;
    }

    /// Test the roots drawn in the last successful frame, in actual drawing order.
    /// Positions are logical window pixels. Use before frame/update mutates UI.
    pub fn hit_test_ui(&self, pos: Vector) -> bool {
        !crate::input::pointer::hit_scene_roots(&self.displayed, pos).is_empty()
    }

    /// Set a factory for fresh default effects. Called only at request selection.
    /// Captures must own the resources they use; they cannot borrow Scene-local handles.
    /// Existing/explicitly configured transitions are unaffected.
    pub fn set_default_scene_transition(
        &mut self,
        factory: impl FnMut() -> SceneTransition + 'static,
    ) {
        self.default_transition = Some(Box::new(factory));
    }

    /// Restore the initial no-animation default, without cancelling a running effect.
    pub fn clear_default_scene_transition(&mut self) {
        self.default_transition = None;
    }

    /// Last successful visual sample (or initial state before the first drawing).
    /// A completed entering endpoint remains observable until the next normal frame.
    pub fn transition_state(&self) -> Option<TransitionFrame> {
        self.active_transition
            .as_ref()
            .map(|transition| transition.sample)
    }

    /// Whether a draw-only [`SceneTransition`] is active or waiting for a frame.
    pub fn is_transitioning(&self) -> bool {
        self.active_transition.is_some()
    }

    /// Queue a push at the safe boundary. Covered Scene state stays owned.
    pub fn push_scene(&mut self, scene: impl Scene + 'static) -> SceneRequest<'_> {
        SceneRequest::register(&mut self.command, SceneCommand::Push(Box::new(scene)))
    }
    /// Queue a pop. An empty final stack ends this App, not the host process.
    pub fn pop_scene(&mut self) -> SceneRequest<'_> {
        SceneRequest::register(&mut self.command, SceneCommand::Pop(None))
    }
    /// Queue a pop with a concrete business value (not an already boxed result).
    pub fn pop_scene_with_result(&mut self, result: impl Any) -> SceneRequest<'_> {
        SceneRequest::register(&mut self.command, SceneCommand::Pop(Some(Box::new(result))))
    }
    /// Replace only the top Scene. Old input completes before it enters;
    /// on a faulted App, recovery is instantaneous and cannot sample failed content.
    /// Covered Scenes are retained; use reset_scene to abandon the whole stack.
    pub fn replace_scene(&mut self, scene: impl Scene + 'static) -> SceneRequest<'_> {
        SceneRequest::register(&mut self.command, SceneCommand::Replace(Box::new(scene)))
    }
    /// Replace the whole stack, including any queued return values.
    /// This does not enter an intermediate exited state.
    pub fn reset_scene(&mut self, scene: impl Scene + 'static) -> SceneRequest<'_> {
        SceneRequest::register(&mut self.command, SceneCommand::Reset(Box::new(scene)))
    }
    /// Number of retained instances, including the active top (before commands).
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }
    pub fn is_exited(&self) -> bool {
        self.state == AppState::Exited
    }
    pub fn is_faulted(&self) -> bool {
        self.state == AppState::Faulted
    }

    /// Run one normal frame, panicking if a callback returns an error.
    ///
    /// Use [`App::try_frame`] when the host handles application errors. A normal
    /// frame is followed by the host's [`macroquad::window::next_frame`] call.
    pub fn frame(&mut self) {
        self.try_frame().expect("Scene frame failed");
    }

    /// Run one normal frame and return callback, layout, or transition errors.
    ///
    /// An error faults the App and cancels input eligibility; it is not retried.
    /// Recover explicitly with [`App::replace_scene`] or [`App::reset_scene`].
    /// Call this once before the host awaits [`macroquad::window::next_frame`].
    pub fn try_frame(&mut self) -> Result<()> {
        self.try_frame_with_update(|| {})
    }

    /// Run host changes after displayed-frame input and ready tasks, before Scene
    /// update and layout. Never mutate hit-test state before the input phase.
    pub fn frame_with_update(&mut self, update: impl FnOnce()) {
        self.try_frame_with_update(update)
            .expect("Scene frame failed");
    }

    /// Fallible form of [`App::frame_with_update`].
    ///
    /// The host callback runs after displayed-frame input and ready tasks, but
    /// before Scene update/layout. It runs once per normal App frame.
    pub fn try_frame_with_update(&mut self, update: impl FnOnce()) -> Result<()> {
        let input = self.pointer_manager.prepare_input();
        let time = macroquad::time::get_time();
        let frame = SceneFrame {
            // Macroquad reports the preceding frame's interval. Sample at the App
            // boundary so suspension cannot cause a delayed jump one frame later.
            dt: self
                .last_frame_time
                .replace(time)
                .map_or_else(macroquad::time::get_frame_time, |previous| {
                    (time - previous).max(0.) as f32
                }),
            time,
            viewport: window_viewport(),
        };
        let result = self.run_frame(input, frame, update);
        let result = self.finish_frame(result);
        if drawable(frame.viewport) {
            crate::paint::texture::flush_pending_texture_deletions();
        }
        result
    }

    fn finish_frame(&mut self, result: Result<()>) -> Result<()> {
        if result.is_err() {
            self.fault();
        }
        result
    }

    fn run_frame(
        &mut self,
        input: InputBatch,
        mut frame: SceneFrame,
        update: impl FnOnce(),
    ) -> Result<()> {
        // Freeze frame-driven animations on recovery, not the timeout/task clock.
        if mem::take(&mut self.host_paused) {
            frame.dt = 0.;
        }
        let can_present = drawable(frame.viewport);
        self.retire_transition();
        let switched = if self.state != AppState::Active {
            // Recovery never replays the drained batch, nor resumes a failed Scene.
            self.check_stopped_command(self.command.as_ref().map(|request| &request.command))?;
            let Some(request) = self.command.take() else {
                return Ok(());
            };
            self.select_request(request, frame, None)?
        } else {
            // Freeze Node targets before results, lifecycle or input callbacks
            // write signals. Node capture and Scene raw input are independent.
            let plan = self.pointer_manager.plan_roots(&self.displayed, &input);
            self.activate(frame)?;
            self.pointer_manager
                .dispatch_roots(plan, &self.displayed, &input);
            let entry = self.scenes.last_mut().unwrap();
            let mut ctx = SceneContext {
                frame,
                transition: self
                    .active_transition
                    .as_ref()
                    .map(|transition| transition.sample),
                command: &mut self.command,
            };
            batched_result("input", || {
                entry.scene.event(SceneEvent::Input(&input), &mut ctx)
            })?;
            false
        };
        if self.state == AppState::Exited {
            return Ok(());
        }
        batch(|| {
            Self::publish_viewport(frame.viewport);
            transition::advance(frame.dt);
        });
        self.pointer_manager.reconcile_roots(&self.displayed);
        self.drive_tasks();
        batch(update);
        let switch_ready = can_present
            && self
                .active_transition
                .as_ref()
                .is_some_and(ActiveTransition::ready_to_switch);
        if self.command.is_none() && !switched && !switch_ready {
            let entry = self.scenes.last_mut().unwrap();
            let mut ctx = SceneContext {
                frame,
                transition: self
                    .active_transition
                    .as_ref()
                    .map(|transition| transition.sample),
                command: &mut self.command,
            };
            batched_result("update", || entry.scene.update(&mut ctx))?;
        }
        let switched = if switched {
            true
        } else if let Some(request) = self.command.take() {
            self.select_request(request, frame, None)?
        } else {
            switch_ready && self.advance_switch_boundary(frame)?
        };
        if self.state == AppState::Exited || !can_present {
            return Ok(());
        }
        if switched {
            // The new top may draw now, but receives neither the old input batch
            // nor a Scene update. Its one Node update must not advance time either.
            frame.dt = 0.;
        }
        self.render_scene(frame)
    }

    fn prepare_scene(&mut self, frame: SceneFrame, update_nodes: bool) -> Result<()> {
        let scene = self.scenes.last_mut().unwrap().scene.as_mut();
        self.layout.collect(scene, &frame)?;
        self.layout.prepare()?;
        if update_nodes {
            batch(|| {
                Runtime::with(|rt| {
                    for node in self
                        .layout
                        .snapshots
                        .iter()
                        .flat_map(GeometrySnapshot::nodes)
                    {
                        rt.invoke_hook(node.id, |hooks| &mut hooks.update, &frame.dt);
                    }
                })
            });
            self.layout.collect(scene, &frame)?;
            self.layout.prepare()?;
        }
        Ok(())
    }

    fn draw_scene(
        scene: &mut dyn Scene,
        frame: SceneFrame,
        roots: &[SceneRoot],
        transition: Option<TransitionFrame>,
        transition_source: bool,
    ) -> Result<Vec<SceneRoot>> {
        let mut render = SceneRender {
            frame,
            prepared: roots,
            drawn: Vec::new(),
            transition,
            transition_source,
        };
        scene
            .render(&mut render)
            .map_err(|error| CallbackError::new("render", error))?;
        Ok(render.drawn)
    }

    fn render_scene(&mut self, frame: SceneFrame) -> Result<()> {
        self.prepare_scene(frame, true)?;
        let roots = &self.layout.roots;
        let scene = self.scenes.last_mut().unwrap().scene.as_mut();
        let draw_normal = |scene, sample| {
            Self::draw_scene(scene, frame, roots, sample, false)
                .inspect_err(|_| PictureCache::cleanup_scene_error())
        };
        self.displayed = if let Some(active) = &mut self.active_transition {
            let sample = active.clock.frame(frame.dt, frame.viewport);
            let drawn = if active.effect.is_composite() {
                self.pictures
                    .draw_composite(sample, &mut active.effect, || {
                        Self::draw_scene(scene, frame, roots, Some(sample), false)
                    })?
            } else {
                let drawn = draw_normal(scene, Some(sample))?;
                PictureCache::draw_overlay(sample, &mut active.effect)?;
                drawn
            };
            active.clock.did_present(sample);
            active.sample = sample;
            drawn
        } else {
            draw_normal(scene, None)?
        };
        if self.opaque_window {
            crate::paint::render::restore_opaque_alpha(frame.viewport);
        }
        self.pointer_manager
            .mark_undisplayed_captures(&self.displayed, |parent, child| {
                self.layout
                    .forest
                    .nodes
                    .get(&child)
                    .is_some_and(|location| location.parent == Some(parent))
            });
        Ok(())
    }

    fn capture_source(&mut self, mut frame: SceneFrame) -> Result<()> {
        frame.dt = 0.;
        self.prepare_scene(frame, false)?;
        let roots = &self.layout.roots;
        let scene = self.scenes.last_mut().unwrap().scene.as_mut();
        // Current retained picture, not last-screen pixels. No input eligibility or updates.
        self.pictures.capture_source(frame.viewport, || {
            Self::draw_scene(scene, frame, roots, None, true).map(|_| ())
        })
    }

    fn activate(&mut self, frame: SceneFrame) -> Result<()> {
        if self.host_paused {
            return Ok(());
        }
        let entry = self.scenes.last_mut().unwrap();
        let mut ctx = SceneContext {
            frame,
            command: &mut self.command,
            transition: self
                .active_transition
                .as_ref()
                .map(|transition| transition.sample),
        };
        // Pop return values belong to the retained receiver, even if that receiver
        // was covered again before the host resumed. Consume each exactly once.
        while let Some(result) = entry.pending_results.pop_front() {
            batched_result("result", || {
                entry.scene.event(SceneEvent::Returned(result), &mut ctx)
            })?;
        }
        match entry.state {
            SceneState::New => {
                entry.state = SceneState::Running;
                batched_result("enter", || entry.scene.event(SceneEvent::Enter, &mut ctx))?;
            }
            SceneState::Paused => {
                entry.state = SceneState::Running;
                batched_result("resume", || entry.scene.event(SceneEvent::Resume, &mut ctx))?;
            }
            SceneState::Running => {}
        }
        Ok(())
    }

    fn pause_top(&mut self, frame: SceneFrame) -> Result<Option<SceneRequestData>> {
        if self.state != AppState::Active {
            return Ok(None);
        }
        // Keep this callback's requests separate from host/task commands. A
        // permanently removed source must not queue a command for its parent.
        let mut command = None;
        if let Some(entry) = self.scenes.last_mut()
            && entry.state == SceneState::Running
        {
            // If the callback fails, neither its side effects nor this partially
            // paused state are rolled back. The checked frame faults the App.
            entry.state = SceneState::Paused;
            let mut ctx = SceneContext {
                frame,
                command: &mut command,
                transition: self
                    .active_transition
                    .as_ref()
                    .map(|transition| transition.sample),
            };
            batched_result("pause", || entry.scene.event(SceneEvent::Pause, &mut ctx))?;
        }
        Ok(command)
    }

    fn check_stopped_command(&self, command: Option<&SceneCommand>) -> Result<()> {
        if self.state == AppState::Faulted
            && !matches!(
                command,
                Some(SceneCommand::Replace(_) | SceneCommand::Reset(_))
            )
        {
            return Err(AppError::Faulted);
        }
        Ok(())
    }

    fn cancel_transition(&mut self) {
        drop(self.active_transition.take());
        self.pictures.invalidate();
    }

    fn retire_transition(&mut self) {
        if self
            .active_transition
            .as_ref()
            .is_some_and(|transition| transition.clock.is_finished())
        {
            self.cancel_transition();
        }
    }

    fn command_exits(&self, command: &SceneCommand) -> bool {
        matches!(command, SceneCommand::Exit)
            || matches!(command, SceneCommand::Pop(_)) && self.scenes.len() <= 1
    }

    fn commit_instant(
        &mut self,
        request: SceneRequestData,
        frame: SceneFrame,
        suspended_pause_request: Option<SceneRequestData>,
    ) -> Result<bool> {
        let SceneRequestData {
            transition,
            command,
        } = request;
        drop(transition);
        self.cancel_transition();
        self.commit_command(command, frame, suspended_pause_request)?;
        Ok(true)
    }

    fn select_request(
        &mut self,
        mut request: SceneRequestData,
        frame: SceneFrame,
        suspended_pause_request: Option<SceneRequestData>,
    ) -> Result<bool> {
        // Keep the effect and command together across fallible capture so visual
        // closures drop before any destination/result handles they may reference.
        let entered_source = self.state == AppState::Active
            && self
                .scenes
                .last()
                .is_some_and(|entry| entry.state != SceneState::New);
        if !entered_source {
            return self.commit_instant(request, frame, suspended_pause_request);
        }
        if matches!(request.transition, TransitionChoice::Default) {
            request.transition = self
                .default_transition
                .as_mut()
                .map_or(TransitionChoice::Instant, |factory| {
                    TransitionChoice::Custom(factory())
                });
        }
        let TransitionChoice::Custom(effect) = &request.transition else {
            return self.commit_instant(request, frame, suspended_pause_request);
        };
        let timing = effect.timing();
        if matches!(timing, TransitionTiming::AfterSwitch(_))
            && self.command_exits(&request.command)
        {
            // No entering Scene exists. An AroundSwitch exit can still animate this.
            return self.commit_instant(request, frame, suspended_pause_request);
        }
        let waits_for_presentation = self.host_paused
            || !drawable(frame.viewport)
            || matches!(timing, TransitionTiming::AroundSwitch { .. });
        let reuse_picture = effect.is_composite()
            && self
                .active_transition
                .as_ref()
                .is_some_and(|active| active.effect.is_composite())
            && self.pictures.freeze_presented(frame.viewport);
        drop(self.active_transition.take());
        if !reuse_picture {
            self.pictures.invalidate();
        }
        if !waits_for_presentation && effect.is_composite() && !reuse_picture {
            self.capture_source(frame)?;
        }
        let SceneRequestData {
            transition: TransitionChoice::Custom(effect),
            command,
        } = request
        else {
            unreachable!("request effect was resolved above")
        };
        if waits_for_presentation {
            self.active_transition =
                Some(ActiveTransition::new(effect, Some(command), frame.viewport));
            // The source is still owned. A newer request from Pause is allowed
            // to interrupt this waiting effect at the next frame's boundary.
            if let Some(request) = suspended_pause_request {
                self.command = Some(request);
            }
            return Ok(false);
        }
        self.active_transition = Some(ActiveTransition::new(effect, None, frame.viewport));
        self.commit_command(command, frame, suspended_pause_request)?;
        Ok(true)
    }

    fn advance_switch_boundary(&mut self, frame: SceneFrame) -> Result<bool> {
        let active = self.active_transition.as_ref().expect("ready transition");
        if active.clock.phase() == TransitionPhase::AfterSwitch
            && active.effect.is_composite()
            && self.pictures.needs_source(frame.viewport)?
        {
            self.capture_source(frame)?;
        }
        let active = self.active_transition.as_mut().unwrap();
        let command = active
            .pending
            .take()
            .expect("ready transition owns a command");
        if self.command_exits(&command) {
            self.cancel_transition();
        } else {
            let active = self.active_transition.as_mut().unwrap();
            if active.clock.phase() == TransitionPhase::BeforeSwitch {
                active.clock.after_switch();
            }
            active.sample = active.clock.frame(0., frame.viewport);
        }
        self.commit_command(command, frame, None)?;
        Ok(true)
    }

    fn commit_command(
        &mut self,
        command: SceneCommand,
        mut frame: SceneFrame,
        suspended_pause_request: Option<SceneRequestData>,
    ) -> Result<()> {
        // The chosen command is no longer in the request slot. Boundary callbacks
        // can only queue requests for a subsequent frame, never mutate the stack.
        frame.dt = 0.;
        let pause_request = if self.host_paused {
            suspended_pause_request
        } else {
            self.pointer_manager.cancel_input();
            match self.pause_top(frame) {
                Ok(request) => request,
                Err(error) => {
                    // The destination is local, so drop its visual first.
                    self.cancel_transition();
                    drop(command);
                    return Err(error);
                }
            }
        };
        self.displayed.clear();
        if matches!(command, SceneCommand::Push(_)) {
            if let Some(request) = pause_request {
                self.command = Some(request);
            }
        } else {
            drop(pause_request);
        }
        match command {
            SceneCommand::Push(scene) => self.scenes.push(SceneEntry::new(scene)),
            SceneCommand::Pop(result) if self.scenes.len() > 1 => {
                drop(self.scenes.pop());
                if let Some(result) = result {
                    self.scenes
                        .last_mut()
                        .unwrap()
                        .pending_results
                        .push_back(result);
                }
            }
            SceneCommand::Replace(scene) => {
                drop(self.scenes.pop());
                self.scenes.push(SceneEntry::new(scene));
            }
            SceneCommand::Reset(scene) => {
                self.drop_scenes();
                self.scenes.push(SceneEntry::new(scene));
            }
            terminal @ (SceneCommand::Pop(_) | SceneCommand::Exit) => {
                // A result without a receiver and other requests must drop
                // before the Scene owners their destructors might reference.
                drop(terminal);
                drop(self.command.take());
                self.drop_scenes();
                self.state = AppState::Exited;
                return Ok(());
            }
        }
        self.state = AppState::Active;
        // While suspended, results and activation wait for the next normal frame.
        self.activate(frame)
    }

    fn drop_scenes(&mut self) {
        // Release children before the retained state/resources they may borrow.
        while let Some(entry) = self.scenes.pop() {
            drop(entry);
        }
    }

    fn fault(&mut self) {
        self.state = AppState::Faulted;
        self.command = None;
        self.cancel_transition();
        self.displayed.clear();
        self.pointer_manager.cancel_input();
    }

    /// Suspend Scene presentation while draining input and ticking host tasks.
    ///
    /// This runs no layout, drawing, Node updates, or frame-driven transition
    /// time. The running top receives [`SceneEvent::Pause`] once. Instantaneous
    /// queued commands may commit, but result/enter/resume notifications wait for
    /// the first normal frame, whose Scene/Node delta is zero. Use
    /// [`App::try_frame_paused`] to handle errors.
    pub fn frame_paused(&mut self) {
        self.try_frame_paused().expect("Scene pause failed");
    }
    /// Fallible form of [`App::frame_paused`].
    pub fn try_frame_paused(&mut self) -> Result<()> {
        self.pointer_manager.pause_input();
        let frame = SceneFrame {
            dt: 0.,
            time: macroquad::time::get_time(),
            viewport: window_viewport(),
        };
        let result = self.run_paused_frame(frame);
        self.finish_frame(result)
    }

    fn run_paused_frame(&mut self, frame: SceneFrame) -> Result<()> {
        self.host_paused = true;
        // Requests made by Pause belong to a later frame, unless the selected
        // command actually removes their owner now.
        self.check_stopped_command(self.command.as_ref().map(|request| &request.command))?;
        let selected = self.command.take();
        if self.state != AppState::Active {
            // Recovery replaces the faulted owner before any ordinary work runs.
            if let Some(request) = selected {
                self.select_request(request, frame, None)?;
                if self.state == AppState::Active {
                    self.drive_tasks();
                }
            }
            return Ok(());
        }
        let pause_request = self.pause_top(frame)?;
        self.drive_tasks();
        if let Some(request) = selected {
            self.select_request(request, frame, pause_request)?;
        } else {
            self.command = pause_request;
        }
        Ok(())
    }

    fn drive_tasks(&mut self) {
        batch(|| {
            let time = get_time();
            Runtime::with(|rt| {
                let key = TimeoutKey {
                    time: NotNan::new(time.next_up()).unwrap(),
                    aborted: Rc::default(),
                };
                #[allow(clippy::mutable_key_type)]
                let due_timeouts = {
                    let mut timeouts = rt.timeouts.borrow_mut();
                    let mut due = timeouts.split_off(&key);
                    mem::swap(&mut *timeouts, &mut due);
                    due
                };
                for (key, callback) in due_timeouts {
                    if !key.aborted.load(Ordering::Relaxed) {
                        callback();
                    }
                }
            });
        });
        task::tick(32);
    }
}

#[derive(Default)]
struct SceneLayout {
    roots: Vec<SceneRoot>,
    snapshots: Vec<GeometrySnapshot>,
    forest: SceneForest,
}

impl SceneLayout {
    fn collect(&mut self, scene: &mut dyn Scene, frame: &SceneFrame) -> Result<()> {
        self.roots.clear();
        scene
            .layout(frame, &mut self.roots)
            .map_err(|error| CallbackError::new("layout", error))?;
        for root in &self.roots {
            if !(root.bounds.min().is_finite()
                && root.bounds.size().is_finite()
                && root.bounds.w >= 0.
                && root.bounds.h >= 0.)
            {
                return Err(SceneError::InvalidRootBounds.into());
            }
        }
        Ok(())
    }

    fn prepare(&mut self) -> std::result::Result<(), SceneError> {
        self.snapshots
            .resize_with(self.roots.len(), GeometrySnapshot::default);
        let snapshots = &mut self.snapshots;
        for _ in 0..64 {
            // Check tree structure before recursively preparing any root.
            self.forest.visit(&self.roots)?;
            for (root, snapshot) in self.roots.iter().zip(snapshots.iter_mut()) {
                if !root.node.prepare_layout_snapshot(root.bounds, snapshot) {
                    return Err(SceneError::DisposedRoot);
                }
            }
            // A later root can invalidate an earlier root's font or geometry.
            // Each root already checked its own publication before returning.
            if self.roots.len() > 1 {
                for root in &self.roots {
                    root.node.resolve_font_sizes();
                }
                if !self
                    .roots
                    .iter()
                    .zip(snapshots.iter())
                    .all(|(root, snapshot)| snapshot.is_current(root.node))
                {
                    continue;
                }
            }
            if self.forest.use_snapshots(&self.roots, snapshots)? {
                return Ok(());
            }
        }
        Err(SceneError::UnstableGeometry)
    }
}

#[derive(Default)]
struct SceneForest {
    nodes: HashMap<NodeRef, NodeLocation>,
    layouts: HashMap<taffy::NodeId, usize>,
    pending: Vec<(NodeRef, Option<NodeRef>)>,
}

struct NodeLocation {
    root: usize,
    parent: Option<NodeRef>,
}

impl SceneForest {
    fn add(
        &mut self,
        node: NodeRef,
        parent: Option<NodeRef>,
        root: usize,
    ) -> std::result::Result<(), SceneError> {
        if !(self
            .nodes
            .insert(node, NodeLocation { root, parent })
            .is_none())
        {
            return Err(SceneError::DuplicateNode);
        }
        Ok(())
    }

    fn visit(&mut self, roots: &[SceneRoot]) -> std::result::Result<(), SceneError> {
        self.nodes.clear();
        self.pending.clear();
        for (index, root) in roots.iter().enumerate() {
            self.pending.push((root.node, None));
            while let Some((node, parent)) = self.pending.pop() {
                let children =
                    Runtime::with(|rt| rt.nodes.borrow().get(node.id).map(|inner| *inner.children));
                let Some(children) = children else { continue };
                // Test identity before descending, including layoutless cycles.
                self.add(node, parent, index)?;
                if let Some(children) = children.try_get_clone_untracked() {
                    self.pending
                        .extend(children.iter().rev().map(|&child| (child, Some(node))));
                }
            }
        }
        self.validate_layouts(roots)?;
        Ok(())
    }

    fn use_snapshots(
        &mut self,
        roots: &[SceneRoot],
        snapshots: &[GeometrySnapshot],
    ) -> std::result::Result<bool, SceneError> {
        self.nodes.clear();
        for (index, snapshot) in snapshots.iter().enumerate() {
            for (node, parent) in snapshot.edges() {
                self.add(node, parent, index)?;
            }
        }
        self.validate_layouts(roots)
    }

    /// No reactive reads here: verify final liveness and Taffy ownership after
    /// callbacks, and report whether the snapshotted forest is still complete.
    fn validate_layouts(&mut self, roots: &[SceneRoot]) -> std::result::Result<bool, SceneError> {
        self.layouts.clear();
        Runtime::with(|rt| {
            let arena = rt.nodes.borrow();
            let taffy = rt.taffy.borrow();
            for root in roots {
                let node = arena.get(root.node.id).ok_or(SceneError::DisposedRoot)?;
                if !node.layout_id.is_none_or(|id| taffy.parent(id).is_none()) {
                    return Err(SceneError::AttachedRoot);
                }
            }
            let count = self.nodes.len();
            self.nodes.retain(|node, location| {
                let Some(inner) = arena.get(node.id) else {
                    return false;
                };
                if let Some(id) = inner.layout_id {
                    self.layouts.insert(id, location.root);
                }
                true
            });
            // A layoutless wrapper must not hide a subtree attached outside its root.
            for (layout, owner) in &self.layouts {
                if !(taffy
                    .parent(*layout)
                    .is_none_or(|parent| self.layouts.get(&parent) == Some(owner)))
                {
                    return Err(SceneError::AttachedRoot);
                }
            }
            Ok(self.nodes.len() == count)
        })
    }
}
