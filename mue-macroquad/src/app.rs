#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod stack_tests;
#[cfg(test)]
mod transition_tests;

use crate::{
    event::pointer::PointerManager,
    input::InputBatch,
    math::{Rect, Vector},
    node::{IntoNode, NodeRef},
    runtime::{Runtime, TimeoutKey, get_time},
    scene::{
        NodeScene, Scene, SceneCommand, SceneContext, SceneEvent, SceneFrame, SceneRender,
        SceneRequest, SceneRequestData, SceneResult, SceneRoot, TransitionChoice,
    },
    scene_transition::{
        PhaseClock, PictureCache, SceneTransition, TransitionFrame, TransitionPhase,
        TransitionTiming,
    },
    transition,
};
use anyhow::{Context, Result, bail, ensure};
use mue_core::{batch, task};
use ordered_float::NotNan;
use std::{
    any::Any,
    collections::{HashSet, VecDeque},
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
    // AfterSwitch requested while suspended waits for one source drawing on resume.
    awaiting_resume: bool,
}
impl ActiveTransition {
    fn new(effect: SceneTransition, pending: Option<SceneCommand>, viewport: Rect) -> Self {
        let mut clock = PhaseClock::new(effect.timing());
        let sample = clock.frame(0., viewport);
        Self {
            effect,
            pending,
            clock,
            sample,
            awaiting_resume: false,
        }
    }
}

/// Retains a Scene stack, scheduling only its top for input, layout and drawing.
/// Covered instances keep their resources, but receive no Scene or Node updates.
/// Global work runs once per application frame, not once per retained Scene.
/// `new(root)` retains the convenient Node-only application entry point.
pub struct App {
    scenes: Vec<SceneEntry>,
    state: AppState,
    // Coverage is determined by stack position; host suspension is independent.
    host_paused: bool,
    command: Option<SceneRequestData>,
    active_transition: Option<ActiveTransition>,
    default_transition: Option<Box<dyn FnMut() -> SceneTransition>>,
    pictures: PictureCache,
    displayed: Vec<SceneRoot>,
    pointer_manager: PointerManager,
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

fn batched_result(f: impl FnOnce() -> Result<()>) -> Result<()> {
    let mut result = Ok(());
    batch(|| result = f());
    result
}

impl App {
    fn initialize_host() -> Rect {
        crate::set_task_wake_callback(Some(Arc::new(miniquad::window::schedule_update)));
        let viewport = crate::viewport::Viewport {
            width: macroquad::window::screen_width(),
            height: macroquad::window::screen_height(),
        };
        crate::viewport::set(viewport);
        Rect::new(0., 0., viewport.width, viewport.height)
    }

    /// Build and retain one Node tree as a Scene.
    pub fn new(root_node: impl IntoNode) -> Self {
        let viewport = Self::initialize_host();
        let root = root_node.into_node();
        root.resolve_font_sizes();
        let root_ref = *root;
        Self {
            scenes: vec![SceneEntry::new(Box::new(NodeScene { root }))],
            state: AppState::Active,
            host_paused: false,
            command: None,
            active_transition: None,
            default_transition: None,
            pictures: PictureCache::new(),
            // Preserve explicit custom hit bounds before the first Node layout.
            displayed: vec![SceneRoot::new(root_ref, viewport)],
            pointer_manager: PointerManager::new(),
        }
    }

    /// Own a Scene. Its SceneEvent::Enter notification runs once on its first normal frame.
    /// Scene UI starts receiving new input after its first successful drawing.
    pub fn with_scene(scene: impl Scene + 'static) -> Self {
        Self::initialize_host();
        Self {
            scenes: vec![SceneEntry::new(Box::new(scene))],
            state: AppState::Active,
            host_paused: false,
            command: None,
            active_transition: None,
            default_transition: None,
            pictures: PictureCache::new(),
            displayed: Vec::new(),
            pointer_manager: PointerManager::new(),
        }
    }

    /// Test the roots drawn in the last successful frame, in actual drawing order.
    /// Positions are logical window pixels. Use before frame/update mutates UI.
    pub fn hit_test_ui(&self, pos: Vector) -> bool {
        !crate::event::pointer::hit_scene_roots(&self.displayed, pos).is_empty()
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

    /// Run one frame, panicking if a Scene callback returns an error.
    /// Use try_frame when the host handles application errors.
    pub fn frame(&mut self) {
        self.try_frame().expect("Scene frame failed");
    }

    pub fn try_frame(&mut self) -> Result<()> {
        self.try_frame_with_update(|| {})
    }

    /// Run host changes after displayed-frame input and ready tasks, before Scene
    /// update and layout. Never mutate hit-test state before the input phase.
    pub fn frame_with_update(&mut self, update: impl FnOnce()) {
        self.try_frame_with_update(update)
            .expect("Scene frame failed");
    }

    pub fn try_frame_with_update(&mut self, update: impl FnOnce()) -> Result<()> {
        let input = self.pointer_manager.prepare_input();
        let frame = SceneFrame {
            dt: macroquad::time::get_frame_time(),
            time: macroquad::time::get_time(),
            viewport: Rect::new(
                0.,
                0.,
                macroquad::window::screen_width(),
                macroquad::window::screen_height(),
            ),
        };
        let result = self.run_checked_frame(input, frame, update, |app, frame| {
            batch(|| {
                crate::viewport::set(crate::viewport::Viewport {
                    width: frame.viewport.w,
                    height: frame.viewport.h,
                });
                transition::advance(frame.dt);
            });
            app.drive_tasks();
        });
        crate::texture::flush_pending_texture_deletions();
        result
    }

    fn run_checked_frame(
        &mut self,
        input: InputBatch,
        frame: SceneFrame,
        update: impl FnOnce(),
        globals: impl FnOnce(&mut Self, SceneFrame),
    ) -> Result<()> {
        let result = self.run_frame(input, frame, update, globals);
        if result.is_err() {
            self.fault();
        }
        result
    }

    fn run_frame(
        &mut self,
        input: InputBatch,
        frame: SceneFrame,
        update: impl FnOnce(),
        globals: impl FnOnce(&mut Self, SceneFrame),
    ) -> Result<()> {
        // Keep the application's clock/delta intact for global work. A resumed
        // Scene must not integrate the suspended interval in its first frame.
        let mut scene_frame = frame;
        if self.host_paused
            || self
                .scenes
                .last()
                .is_some_and(|entry| entry.state == SceneState::Paused)
        {
            scene_frame.dt = 0.;
        }
        self.host_paused = false;
        self.retire_transition();
        let mut switched = false;
        if self.state != AppState::Active {
            // Fault/exit recovery never replays the drained batch. Push/pop
            // cannot resume a failed instance; replacement/reset/exit are explicit.
            self.check_stopped_command(self.command.as_ref().map(|request| &request.command))?;
            if self.command.is_none() {
                return Ok(());
            }
            switched = self.install_command(scene_frame)?;
        } else {
            // Freeze Node targets before results, lifecycle or input callbacks
            // write signals. Node capture and Scene raw input are independent.
            let plan = self.pointer_manager.plan_roots(&self.displayed, &input);
            self.activate(scene_frame)?;
            self.pointer_manager
                .dispatch_roots(plan, &self.displayed, &input);
            let entry = self.scenes.last_mut().unwrap();
            let mut ctx = SceneContext {
                frame: scene_frame,
                transition: self
                    .active_transition
                    .as_ref()
                    .map(|transition| transition.sample),
                command: &mut self.command,
            };
            batched_result(|| entry.scene.event(SceneEvent::Input(&input), &mut ctx))
                .context("Scene input")?;
        }
        if self.state == AppState::Exited {
            return Ok(());
        }
        globals(self, frame);
        batch(update);
        if self.command.is_none()
            && !switched
            && !self
                .active_transition
                .as_ref()
                .is_some_and(|transition| transition.awaiting_resume)
        {
            let entry = self.scenes.last_mut().unwrap();
            let mut ctx = SceneContext {
                frame: scene_frame,
                transition: self
                    .active_transition
                    .as_ref()
                    .map(|transition| transition.sample),
                command: &mut self.command,
            };
            batched_result(|| entry.scene.update(&mut ctx)).context("Scene update")?;
        }
        if self.command.is_some() && !switched {
            switched = self.install_command(scene_frame)?;
        }
        if !switched && self.command.is_none() {
            switched = self.advance_switch_boundary(scene_frame)?;
        }
        if self.state == AppState::Exited {
            return Ok(());
        }
        if switched {
            // The new top may draw now, but receives neither the old input batch
            // nor a Scene update. Its one Node update must not advance time either.
            scene_frame.dt = 0.;
        }
        self.render_scene(scene_frame)
    }

    fn prepare_scene(&mut self, frame: SceneFrame, update_nodes: bool) -> Result<Vec<SceneRoot>> {
        let mut roots = self.collect_roots(frame)?;
        Self::prepare_roots(&roots)?;
        if update_nodes {
            // Snapshot the forest before hooks. Capture-only rendering never ticks it.
            let nodes = Self::validate_roots(&roots)?;
            batch(|| {
                Runtime::with(|rt| {
                    for node in nodes {
                        rt.invoke_hook(node.id, |hooks| &mut hooks.update, &frame.dt);
                    }
                })
            });
            roots = self.collect_roots(frame)?;
            Self::prepare_roots(&roots)?;
        }
        Ok(roots)
    }

    fn draw_scene(
        &mut self,
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
        let result = self.scenes.last_mut().unwrap().scene.render(&mut render);
        if result.is_err() {
            self.pictures.cleanup_scene_error();
        }
        result.context("Scene render")?;
        Ok(render.drawn)
    }

    fn render_scene(&mut self, frame: SceneFrame) -> Result<()> {
        let roots = self.prepare_scene(frame, true)?;
        let sample = self
            .active_transition
            .as_mut()
            .map(|transition| transition.clock.frame(frame.dt, frame.viewport));
        let composite = self
            .active_transition
            .as_ref()
            .is_some_and(|transition| transition.effect.is_composite());
        let drawn = if composite {
            self.pictures.begin_live(frame.viewport)?;
            let drawn = self.draw_scene(frame, &roots, sample, false);
            if drawn.is_ok() {
                self.pictures.finish_live();
            } else {
                self.pictures.finish();
            }
            drawn?
        } else {
            self.draw_scene(frame, &roots, sample, false)?
        };
        if let Some(sample) = sample {
            let active = self.active_transition.as_mut().unwrap();
            if composite {
                self.pictures
                    .compose(sample, &mut active.effect)
                    .context("Scene transition composite")?;
            } else {
                self.pictures
                    .draw_overlay(sample, &mut active.effect)
                    .context("Scene transition overlay")?;
            }
            // A terminal sample is acknowledged only after all drawing/flushes succeed.
            active.clock.did_present();
            active.sample = sample;
        }
        self.displayed = drawn;
        Ok(())
    }

    fn capture_source(&mut self, mut frame: SceneFrame) -> Result<()> {
        frame.dt = 0.;
        let roots = self.prepare_scene(frame, false)?;
        self.pictures.begin_source(frame.viewport)?;
        let drawn = self.draw_scene(frame, &roots, None, true);
        if drawn.is_ok() {
            self.pictures.finish_source();
        } else {
            self.pictures.finish();
        }
        // Current retained picture, not last-screen pixels. No input eligibility or updates.
        drawn.map(|_| ())
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
            batched_result(|| entry.scene.event(SceneEvent::Returned(result), &mut ctx))
                .context("Scene result")?;
        }
        match entry.state {
            SceneState::New => {
                entry.state = SceneState::Running;
                batched_result(|| entry.scene.event(SceneEvent::Enter, &mut ctx))
                    .context("Scene enter")?;
            }
            SceneState::Paused => {
                entry.state = SceneState::Running;
                batched_result(|| entry.scene.event(SceneEvent::Resume, &mut ctx))
                    .context("Scene resume")?;
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
            batched_result(|| entry.scene.event(SceneEvent::Pause, &mut ctx))
                .context("Scene pause")?;
        }
        Ok(command)
    }

    fn check_stopped_command(&self, command: Option<&SceneCommand>) -> Result<()> {
        if self.state == AppState::Faulted {
            ensure!(
                matches!(
                    command,
                    Some(SceneCommand::Replace(_) | SceneCommand::Reset(_) | SceneCommand::Exit)
                ),
                "Scene is faulted; replace or reset it explicitly before continuing"
            );
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

    fn install_command(&mut self, frame: SceneFrame) -> Result<bool> {
        let Some(request) = self.command.take() else {
            return Ok(false);
        };
        self.select_request(request, frame)
    }

    fn select_request(&mut self, mut request: SceneRequestData, frame: SceneFrame) -> Result<bool> {
        // Keep the visual and destination aggregated across fallible capture work:
        // error unwinding must drop the visual before any handles it may reference.
        let reuse_picture = self
            .active_transition
            .as_ref()
            .is_some_and(|active| active.effect.is_composite())
            && (self.pictures.freeze_presented(frame.viewport)?
                || !self.pictures.needs_source(frame.viewport)?);
        drop(self.active_transition.take());
        if !reuse_picture {
            self.pictures.invalidate();
        }
        let entered_source = self.state == AppState::Active
            && self
                .scenes
                .last()
                .is_some_and(|entry| entry.state != SceneState::New);
        if !entered_source {
            // Recovery/never-entered sources have no valid outgoing picture. Commit
            // instantly, even while suspended, without manufacturing Enter/Pause.
            drop(mem::replace(
                &mut request.transition,
                TransitionChoice::Instant,
            ));
            self.pictures.invalidate();
            self.commit_command(request.command, frame)?;
            return Ok(true);
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
            self.pictures.invalidate();
            self.commit_command(request.command, frame)?;
            return Ok(true);
        };
        let timing = effect.timing();
        if matches!(timing, TransitionTiming::AfterSwitch(_))
            && self.command_exits(&request.command)
        {
            // No entering Scene exists. An AroundSwitch exit can still animate this.
            drop(mem::replace(
                &mut request.transition,
                TransitionChoice::Instant,
            ));
            self.pictures.invalidate();
            self.commit_command(request.command, frame)?;
            return Ok(true);
        }
        if matches!(timing, TransitionTiming::AfterSwitch(_))
            && !self.host_paused
            && effect.is_composite()
            && !reuse_picture
            && self.pictures.needs_source(frame.viewport)?
        {
            self.capture_source(frame)?;
        }
        let SceneRequestData {
            transition: TransitionChoice::Custom(effect),
            command,
        } = request
        else {
            unreachable!("request effect was resolved above");
        };
        let wait = matches!(timing, TransitionTiming::AroundSwitch { .. }) || self.host_paused;
        let mut active = ActiveTransition::new(effect, Some(command), frame.viewport);
        active.awaiting_resume =
            self.host_paused && matches!(timing, TransitionTiming::AfterSwitch(_));
        self.active_transition = Some(active);
        if wait {
            return Ok(false);
        }
        let command = self
            .active_transition
            .as_mut()
            .unwrap()
            .pending
            .take()
            .unwrap();
        self.commit_command(command, frame)?;
        Ok(true)
    }

    fn advance_switch_boundary(&mut self, frame: SceneFrame) -> Result<bool> {
        let Some(active) = self.active_transition.as_ref() else {
            return Ok(false);
        };
        if self.host_paused || !(active.awaiting_resume || active.clock.ready_to_switch()) {
            return Ok(false);
        }
        if active.awaiting_resume
            && active.effect.is_composite()
            && self.pictures.needs_source(frame.viewport)?
        {
            let can_draw_source = self
                .scenes
                .last()
                .is_some_and(|entry| entry.state != SceneState::New);
            if can_draw_source {
                self.capture_source(frame)?;
            }
        }
        let active = self.active_transition.as_mut().unwrap();
        let command = active
            .pending
            .take()
            .expect("outgoing transition owns its request");
        active.awaiting_resume = false;
        if self.command_exits(&command) {
            self.cancel_transition();
        } else {
            let active = self.active_transition.as_mut().unwrap();
            if active.clock.phase() == TransitionPhase::BeforeSwitch {
                active.clock.after_switch()?;
            }
            active.sample = active.clock.frame(0., frame.viewport);
        }
        self.commit_command(command, frame)?;
        Ok(true)
    }

    fn commit_command(&mut self, command: SceneCommand, mut frame: SceneFrame) -> Result<()> {
        // Take the selected command before user callbacks. Requests made during
        // pause/result/enter/resume are a separate command for the next frame;
        // they cannot recursively mutate the stack at this boundary.
        self.pointer_manager.cancel_input();
        self.displayed.clear();
        frame.dt = 0.;
        let pause_request = match self.pause_top(frame) {
            Ok(request) => request,
            Err(error) => {
                // The selected destination is still local here. Drop its visual first.
                self.cancel_transition();
                drop(command);
                return Err(error);
            }
        };
        if matches!(command, SceneCommand::Push(_)) {
            if let Some(request) = pause_request {
                self.command = Some(request);
            }
        } else {
            // Pop/replace/reset/exit remove the callback's owner permanently.
            // Drop its requested values before that owner, without discarding
            // unrelated host/task requests already queued during a paused frame.
            drop(pause_request);
        }
        match command {
            SceneCommand::Push(scene) => {
                self.scenes.push(SceneEntry::new(scene));
            }
            SceneCommand::Pop(result) => {
                if self.scenes.len() <= 1 {
                    // No receiver. Discard values/requests before any resources
                    // their destructors may read, including a pause-queued child.
                    drop(result);
                    drop(self.command.take());
                    self.drop_scenes();
                    self.state = AppState::Exited;
                    return Ok(());
                }
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
            SceneCommand::Exit => {
                drop(self.command.take());
                self.drop_scenes();
                self.state = AppState::Exited;
                return Ok(());
            }
        }
        self.state = AppState::Active;
        // While suspended, new instances and return values stay pending. A normal
        // frame will deliver results before enter/resume, with no old input replay.
        self.activate(frame)
    }

    fn drop_scenes(&mut self) {
        // Release children before the retained state/resources they may borrow.
        while let Some(entry) = self.scenes.pop() {
            drop(entry);
        }
    }

    fn collect_roots(&mut self, frame: SceneFrame) -> Result<Vec<SceneRoot>> {
        let mut roots = Vec::new();
        self.scenes
            .last_mut()
            .unwrap()
            .scene
            .layout(&frame, &mut roots)
            .context("Scene layout roots")?;
        Self::validate_roots(&roots)?;
        Ok(roots)
    }

    fn validate_roots(roots: &[SceneRoot]) -> Result<Vec<NodeRef>> {
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        for root in roots {
            ensure!(
                root.bounds.min().is_finite()
                    && root.bounds.size().is_finite()
                    && root.bounds.w >= 0.
                    && root.bounds.h >= 0.,
                "invalid Scene root bounds"
            );
            let live = Runtime::with(|rt| rt.nodes.borrow().contains_key(root.node.id));
            ensure!(live, "Scene submitted a disposed root");
            if let Some(id) = root.node.layout_id() {
                ensure!(
                    Runtime::with(|rt| rt.taffy.borrow().parent(id).is_none()),
                    "Scene must submit a tree root, not a Node attached to another layout"
                );
            }
            for node in root.node.frame_nodes() {
                ensure!(
                    seen.insert(node),
                    "Scene roots overlap or contain a duplicate Node"
                );
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn prepare_roots(roots: &[SceneRoot]) -> Result<()> {
        // Geometry observers may feed back into an earlier independent root.
        for _ in 0..64 {
            Self::validate_roots(roots)?;
            let mut snapshots = Vec::new();
            for root in roots {
                root.node.prepare_layout_in(root.bounds);
                snapshots.push(root.node.prepared_geometry());
            }
            Self::validate_roots(roots)?;
            // A later root observer may change an earlier root's inherited font;
            // resolve that binding before checking measurement dirtiness.
            for root in roots {
                root.node.resolve_font_sizes();
            }
            if snapshots.iter().all(|snapshot| snapshot.is_current()) {
                return Ok(());
            }
        }
        bail!("Scene root geometry did not stabilize");
    }

    fn fault(&mut self) {
        self.state = AppState::Faulted;
        self.command = None;
        self.cancel_transition();
        self.displayed.clear();
        self.pointer_manager.cancel_input();
    }

    /// Cancel/drain input and advance tasks without layout, drawing or node time.
    /// pause runs once. Commands may commit while suspended, but results and
    /// enter/resume wait for the first normal frame, whose Scene/Node dt is zero.
    pub fn frame_paused(&mut self) {
        self.try_frame_paused().expect("Scene pause failed");
    }
    pub fn try_frame_paused(&mut self) -> Result<()> {
        self.pointer_manager.pause_input();
        let frame = SceneFrame {
            dt: 0.,
            time: macroquad::time::get_time(),
            viewport: Rect::new(
                0.,
                0.,
                macroquad::window::screen_width(),
                macroquad::window::screen_height(),
            ),
        };
        self.run_checked_paused_frame(frame, |app| app.drive_tasks())
    }

    fn run_checked_paused_frame(
        &mut self,
        mut frame: SceneFrame,
        globals: impl FnOnce(&mut Self),
    ) -> Result<()> {
        frame.dt = 0.;
        self.host_paused = true;
        // Fix this frame's command before pause can queue another one. Lifecycle
        // requests during this boundary must wait, even without normal rendering.
        let mut selected = self.command.take();
        let paused = self.pause_scene(frame).map(|request| {
            if matches!(
                selected.as_ref().map(|request| &request.command),
                None | Some(SceneCommand::Push(_))
            ) {
                if let Some(request) = request {
                    self.command = Some(request);
                }
            } else {
                // The selected operation permanently removes the paused source.
                drop(request);
            }
        });
        if paused.is_err() {
            // A selected child/visual can borrow source handles. Drop before globals
            // can recover/reset and release that source after a failed pause.
            drop(selected.take());
            self.fault();
        }
        // Non-GPU work still ticks exactly once, including a failed pause frame.
        globals(self);
        let result = paused.and_then(|()| {
            self.check_stopped_command(selected.as_ref().map(|request| &request.command))?;
            if let Some(request) = selected {
                // Animated requests wait for rendering; instantaneous commands retain
                // the original suspended commit/results-deferred contract.
                self.select_request(request, frame)?;
            }
            Ok(())
        });
        if result.is_err() {
            self.fault();
        }
        result
    }

    fn pause_scene(&mut self, frame: SceneFrame) -> Result<Option<SceneRequestData>> {
        self.host_paused = true;
        // Keep the last display's hit geometry when the top is unchanged. Fresh
        // input on the first resumed frame must still reach that displayed UI.
        self.pointer_manager.cancel_input();
        self.pause_top(frame)
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
