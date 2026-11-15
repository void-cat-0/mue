#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod stack_tests;

use crate::{
    event::pointer::PointerManager,
    input::InputBatch,
    math::{Rect, Vector},
    node::{IntoNode, NodeRef},
    runtime::{Runtime, TimeoutKey, get_time},
    scene::{
        NodeScene, Scene, SceneCommand, SceneContext, SceneEvent, SceneFrame, SceneRender,
        SceneResult, SceneRoot,
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

/// Retains a Scene stack, scheduling only its top for input, layout and drawing.
/// Covered instances keep their resources, but receive no Scene or Node updates.
/// Global work runs once per application frame, not once per retained Scene.
/// `new(root)` retains the convenient Node-only application entry point.
pub struct App {
    scenes: Vec<SceneEntry>,
    state: AppState,
    // Coverage is determined by stack position; host suspension is independent.
    host_paused: bool,
    command: Option<SceneCommand>,
    displayed: Vec<SceneRoot>,
    pointer_manager: PointerManager,
}

impl Drop for App {
    fn drop(&mut self) {
        // A queued child can borrow retained parent handles without owning them.
        drop(self.command.take());
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
            displayed: Vec::new(),
            pointer_manager: PointerManager::new(),
        }
    }

    /// Test the roots drawn in the last successful frame, in actual drawing order.
    /// Positions are logical window pixels. Use before frame/update mutates UI.
    pub fn hit_test_ui(&self, pos: Vector) -> bool {
        !crate::event::pointer::hit_scene_roots(&self.displayed, pos).is_empty()
    }

    /// Queue a push at the safe boundary. Covered Scene state stays owned.
    pub fn push_scene(&mut self, scene: impl Scene + 'static) {
        self.command = Some(SceneCommand::Push(Box::new(scene)));
    }
    /// Queue a pop. An empty final stack ends this App, not the host process.
    pub fn pop_scene(&mut self) {
        self.command = Some(SceneCommand::Pop(None));
    }
    /// Queue a pop with a concrete business value (not an already boxed result).
    pub fn pop_scene_with_result(&mut self, result: impl Any) {
        self.command = Some(SceneCommand::Pop(Some(Box::new(result))));
    }
    /// Replace only the top Scene. Old input completes before it enters;
    /// on a faulted App, the next frame drains input and installs a new instance.
    /// Covered Scenes are retained; use reset_scene to abandon the whole stack.
    pub fn replace_scene(&mut self, scene: impl Scene + 'static) {
        self.command = Some(SceneCommand::Replace(Box::new(scene)));
    }
    /// Replace the whole stack, including any queued return values.
    /// This does not enter an intermediate exited state.
    pub fn reset_scene(&mut self, scene: impl Scene + 'static) {
        self.command = Some(SceneCommand::Reset(Box::new(scene)));
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
        let mut switched = false;
        if self.state != AppState::Active {
            // Fault/exit recovery never replays the drained batch. Push/pop
            // cannot resume a failed instance; replacement/reset/exit are explicit.
            self.check_stopped_command(self.command.as_ref())?;
            if self.command.is_none() {
                return Ok(());
            }
            self.install_command(scene_frame)?;
            switched = true;
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
        if self.command.is_none() && !switched {
            let entry = self.scenes.last_mut().unwrap();
            let mut ctx = SceneContext {
                frame: scene_frame,
                command: &mut self.command,
            };
            batched_result(|| entry.scene.update(&mut ctx)).context("Scene update")?;
        }
        if self.command.is_some() && !switched {
            self.install_command(scene_frame)?;
            switched = true;
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

    fn render_scene(&mut self, frame: SceneFrame) -> Result<()> {
        let mut roots = self.collect_roots(frame)?;
        Self::prepare_roots(&roots)?;
        // Capture all roots before callbacks, so cross-root writes cannot cause
        // newly mounted nodes to update early or repeat an update.
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
        let mut render = SceneRender {
            frame,
            prepared: &roots,
            drawn: Vec::new(),
        };
        self.scenes
            .last_mut()
            .unwrap()
            .scene
            .render(&mut render)
            .context("Scene render")?;
        self.displayed = render.drawn;
        Ok(())
    }

    fn activate(&mut self, frame: SceneFrame) -> Result<()> {
        if self.host_paused {
            return Ok(());
        }
        let entry = self.scenes.last_mut().unwrap();
        let mut ctx = SceneContext {
            frame,
            command: &mut self.command,
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

    fn pause_top(&mut self, frame: SceneFrame) -> Result<Option<SceneCommand>> {
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

    fn install_command(&mut self, frame: SceneFrame) -> Result<()> {
        let Some(command) = self.command.take() else {
            return Ok(());
        };
        self.commit_command(command, frame)
    }

    fn commit_command(&mut self, command: SceneCommand, mut frame: SceneFrame) -> Result<()> {
        // Take the selected command before user callbacks. Requests made during
        // pause/result/enter/resume are a separate command for the next frame;
        // they cannot recursively mutate the stack at this boundary.
        self.pointer_manager.cancel_input();
        self.displayed.clear();
        frame.dt = 0.;
        let pause_request = self.pause_top(frame)?;
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
        let selected = self.command.take();
        let paused = self.pause_scene(frame).map(|request| {
            if matches!(selected.as_ref(), None | Some(SceneCommand::Push(_))) {
                if let Some(request) = request {
                    self.command = Some(request);
                }
            } else {
                // The selected operation permanently removes the paused source.
                drop(request);
            }
        });
        if paused.is_err() {
            self.fault();
        }
        // Non-GPU work still ticks exactly once, including a failed pause frame.
        globals(self);
        let result = paused.and_then(|()| {
            self.check_stopped_command(selected.as_ref())?;
            if let Some(command) = selected {
                // Leave pause/task requests owned by self.command. They wait for
                // the next frame, or are dropped before the stack if this is exit.
                self.commit_command(command, frame)?;
            }
            Ok(())
        });
        if result.is_err() {
            self.fault();
        }
        result
    }

    fn pause_scene(&mut self, frame: SceneFrame) -> Result<Option<SceneCommand>> {
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
