//! Application-owned screens and their lifecycle, input, and rendering contract.
//!
//! A [`Scene`] owns its application state and Node trees; [`crate::App`] owns
//! scheduling and retains the Scene stack. Only the top Scene participates in
//! normal callbacks and Node updates. Covered Scenes keep their state, but are
//! not implicitly drawn behind the active Scene.
//!
//! `event` delivers lifecycle notifications and the complete raw
//! [`InputBatch`], after Node UI dispatch. UI routing
//! does not consume that raw batch or decide which events count as gameplay.
//! Implement the policy your application needs, including cancellation after an
//! interrupted batch via [`InputBatch::is_interrupted`].
//!
//! Stack operations from [`SceneContext`] register deferred [`SceneRequest`]s.
//! They commit at a safe boundary after input/global/host work, before the
//! subsequent layout and drawing. Use [`SceneEvent`] for lifecycle timing and
//! [`SceneTransition`] for optional draw-only effects;
//! neither replaces the host's frame loop.
use crate::{
    CallbackResult, SceneError,
    input::InputBatch,
    math::Rect,
    node::{Node, NodeRef},
    scene_transition::{SceneTransition, TransitionFrame},
};
use std::any::Any;

/// How a displayed root participates in Node pointer routing.
///
/// This policy never filters the raw batch delivered through
/// [`SceneEvent::Input`]. See [`SceneRoot::input`] and the [`input`](crate::input)
/// module for the separate Node and Scene input paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RootInput {
    /// Draw/update only; input passes through this root.
    None,
    /// Only interactive Node paths block lower Node roots.
    /// This never filters the raw input delivered to SceneEvent::Input.
    #[default]
    Interactive,
    /// Block lower Node roots for new pointers within the submitted bounds.
    /// The Scene still receives the complete raw input batch.
    Modal,
}

/// A borrowed root and its logical window-space layout constraints.
///
/// The Scene must retain the owning tree through input, preparation, and
/// drawing. [`SceneRoot::bounds`] constrain available layout space and new Node
/// input queries; they do not clip drawing or Scene raw input.
#[derive(Clone, Copy, PartialEq)]
pub struct SceneRoot {
    pub node: NodeRef,
    pub bounds: Rect,
    pub input: RootInput,
}
impl SceneRoot {
    pub fn new(node: NodeRef, bounds: Rect) -> Self {
        Self {
            node,
            bounds,
            input: RootInput::Interactive,
        }
    }
    pub fn input(mut self, input: RootInput) -> Self {
        self.input = input;
        self
    }
}

/// Timing and viewport data shared by one Scene frame's callbacks.
///
/// `dt` is frame-driven UI/gameplay time and `time` is macroquad host time. Neither
/// is an audio or judgement clock; applications that need those semantics should
/// own the corresponding clock. See [`Scene::update`] and [`Scene::layout`].
#[derive(Clone, Copy, Debug)]
pub struct SceneFrame {
    pub dt: f32,
    pub time: f64,
    pub viewport: Rect,
}

/// An owned return value from a popped Scene. The receiving Scene defines its
/// concrete business type and may downcast it; no Send bound is needed.
pub type SceneResult = Box<dyn Any>;

/// A notification delivered at its own scheduling phase, not necessarily during
/// the ordinary per-frame update. Input is borrowed; returned business values
/// are owned and consumed once. Matching an event never changes its timing.
pub enum SceneEvent<'a> {
    /// This instance's first activation. Delivered once before normal input/update.
    Enter,
    /// A running instance loses permission to run: coverage, host suspension or
    /// permanent removal. Old UI capture/hover is canceled first. Already paused
    /// or never-entered instances do not receive duplicate/spurious notifications.
    /// Requests from a retained instance wait until the next frame; requests from
    /// a permanently departing instance are dropped before its owner.
    Pause,
    /// An entered instance becomes runnable again, after pending return values.
    /// Coverage and host suspension must both have ended. This does not clear a
    /// Scene's internal game-pause state automatically. A switch may skip the
    /// resumed Scene's ordinary update; restart external playback from that
    /// update rather than this notification.
    Resume,
    /// The complete original raw batch, after automatic Node UI dispatch and
    /// before global tasks and the Scene's ordinary update. UI does not consume
    /// this batch; handle InputBatch::is_interrupted before replaying its events.
    Input(&'a InputBatch),
    /// A popped child's owned business value, before reactivation, never during
    /// host suspension. Deferred values are FIFO. A never-entered receiver can
    /// receive these before first Enter (host-side push/pop while suspended);
    /// construct the state needed to handle them. Ignored values drop normally.
    Returned(SceneResult),
}

pub(crate) enum SceneCommand {
    Push(Box<dyn Scene>),
    Pop(Option<SceneResult>),
    Replace(Box<dyn Scene>),
    Reset(Box<dyn Scene>),
    Exit,
}

pub(crate) enum TransitionChoice {
    Default,
    Instant,
    Custom(SceneTransition),
}

pub(crate) struct SceneRequestData {
    // A visual closure may borrow handles from a queued destination. Drop it first.
    pub(crate) transition: TransitionChoice,
    pub(crate) command: SceneCommand,
}

/// Configure an already registered, deferred Scene request.
///
/// Ignoring this value still leaves the request queued; Drop never commits it.
/// A later request replaces the earlier request and its transition together.
/// Without an override, App resolves its default factory at the safe boundary.
pub struct SceneRequest<'a> {
    data: &'a mut SceneRequestData,
}
impl<'a> SceneRequest<'a> {
    pub(crate) fn register(slot: &'a mut Option<SceneRequestData>, command: SceneCommand) -> Self {
        *slot = Some(SceneRequestData {
            transition: TransitionChoice::Default,
            command,
        });
        Self {
            data: slot.as_mut().unwrap(),
        }
    }

    /// Override the App default for this request only.
    pub fn transition(self, transition: SceneTransition) -> Self {
        self.data.transition = TransitionChoice::Custom(transition);
        self
    }

    /// Bypass both the App default and any previously configured effect.
    pub fn without_transition(self) -> Self {
        self.data.transition = TransitionChoice::Instant;
        self
    }
}

/// Frame data, transition state, and deferred Scene-stack operations.
///
/// Calls such as [`SceneContext::push`] and [`SceneContext::pop`] register a
/// [`SceneRequest`] rather than mutating the stack inline. Requests are applied
/// at the frame's safe boundary. `transition()` returns the last successfully
/// presented sample for callback-side readiness decisions.
pub struct SceneContext<'a> {
    pub frame: SceneFrame,
    pub(crate) command: &'a mut Option<SceneRequestData>,
    pub(crate) transition: Option<TransitionFrame>,
}
impl SceneContext<'_> {
    /// Last successfully presented transition state; never a prediction of input geometry.
    /// A switching lifecycle callback sees the new phase at its initial progress.
    pub fn transition(&self) -> Option<TransitionFrame> {
        self.transition
    }

    /// Whether a Scene transition is active or waiting for presentation.
    pub fn is_transitioning(&self) -> bool {
        self.transition.is_some()
    }

    /// Cover the current Scene without dropping it. Only the new top is scheduled.
    /// The current Scene pauses before the new Scene enters at the safe boundary.
    pub fn push(&mut self, scene: impl Scene + 'static) -> SceneRequest<'_> {
        SceneRequest::register(self.command, SceneCommand::Push(Box::new(scene)))
    }
    /// Pop the active top at the next safe boundary. Removing the last Scene
    /// ends the App, not the host process. Requests target the top at commit time.
    pub fn pop(&mut self) -> SceneRequest<'_> {
        SceneRequest::register(self.command, SceneCommand::Pop(None))
    }
    /// Pop and deliver an owned business result to the newly exposed Scene before
    /// it resumes. A result without a receiving Scene is dropped normally.
    /// Pass the concrete business value, not an already boxed SceneResult.
    pub fn pop_with_result(&mut self, result: impl Any) -> SceneRequest<'_> {
        SceneRequest::register(self.command, SceneCommand::Pop(Some(Box::new(result))))
    }
    /// Replace only the top Scene, preserving any covered Scenes below it.
    /// A later request in the same frame replaces the earlier request.
    pub fn replace(&mut self, scene: impl Scene + 'static) -> SceneRequest<'_> {
        SceneRequest::register(self.command, SceneCommand::Replace(Box::new(scene)))
    }
    /// End the whole App and release its stack. The host owns loop termination.
    pub fn exit(&mut self) -> SceneRequest<'_> {
        SceneRequest::register(self.command, SceneCommand::Exit)
    }
}

/// One retained screen in an [`crate::App`]-owned Scene stack.
///
/// A Scene owns its Node trees, gameplay state, and custom drawing resources.
/// App schedules only the top Scene; covered instances retain their state but do
/// not receive ordinary updates. Implement [`event`](Scene::event) for lifecycle
/// and raw input, [`update`](Scene::update) for frame-driven business,
/// [`layout`](Scene::layout) to submit roots, and [`render`](Scene::render) to
/// order custom drawing with UI.
///
/// Keep hit-test state unchanged between drawing and the next input callback.
/// App preplans and dispatches Node input, then passes the complete original
/// InputBatch to SceneEvent::Input, including events handled by UI and continuing
/// Move/Up events whose Down belonged to an earlier Scene. Touch timestamps and
/// physical positions remain unchanged; handle InputBatch::is_interrupted first.
/// A Scene decides how raw input affects gameplay or other content. Node capture
/// is independent; there is no mandatory UI/Scene press-ownership filter.
///
/// Callback errors stop ordinary Scene scheduling and propagate through
/// App::try_frame; no callback is automatically retried after an error.
pub trait Scene {
    /// Respond to input, lifecycle changes and child returns. The default ignores
    /// notifications (and drops owned return values). All events keep their
    /// scheduling boundaries: Input before global work/update, Returned before
    /// Resume/Enter, and no Enter/Resume/Returned while the host is suspended.
    fn event(&mut self, _event: SceneEvent<'_>, _ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        Ok(())
    }
    /// Advance ordinary frame-driven business. Not called on a covered Scene or
    /// the newly activated top in a switching frame; notifications are not updates.
    fn update(&mut self, _ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        Ok(())
    }
    /// Submit independent roots. Do not draw or advance time here.
    /// App may call this again after node updates change root membership.
    fn layout(&mut self, _frame: &SceneFrame, _roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        Ok(())
    }
    /// Draw custom content and submitted roots in their intended visual order.
    ///
    /// Use [`SceneRender::draw`] rather than calling `NodeRef::render` directly,
    /// so App can track displayed root order for the next input batch.
    fn render(&mut self, _render: &mut SceneRender<'_>) -> CallbackResult<()> {
        Ok(())
    }
}

/// Controlled UI drawing inside a Scene's custom drawing sequence.
///
/// [`SceneRender::draw`] records actual root order for the next input batch;
/// omitted roots do not receive new Node input. External rendering belongs
/// between complete draw calls. Transition state describes this candidate draw;
/// see [`SceneRender::transition`] and [`SceneRender::is_transition_source`].
pub struct SceneRender<'a> {
    pub frame: SceneFrame,
    pub(crate) prepared: &'a [SceneRoot],
    pub(crate) drawn: Vec<SceneRoot>,
    pub(crate) transition: Option<TransitionFrame>,
    pub(crate) transition_source: bool,
}
impl SceneRender<'_> {
    /// Transition sample used for this drawing, before presentation is acknowledged.
    pub fn transition(&self) -> Option<TransitionFrame> {
        self.transition
    }

    /// This drawing captures the outgoing picture, not a new displayed frame.
    /// It never advances Scene or Node updates or changes input eligibility.
    pub fn is_transition_source(&self) -> bool {
        self.transition_source
    }

    pub fn draw(&mut self, node: NodeRef) -> std::result::Result<(), SceneError> {
        let root = self
            .prepared
            .iter()
            .find(|root| root.node == node)
            .ok_or(SceneError::UndeclaredDrawRoot)?;
        if self.drawn.iter().any(|root| root.node == node) {
            return Err(SceneError::DuplicateDrawRoot);
        }
        if !crate::runtime::Runtime::with(|rt| rt.nodes.borrow().contains_key(node.id)) {
            return Err(SceneError::DisposedDrawRoot);
        }
        node.render();
        self.drawn.push(*root);
        Ok(())
    }
}

pub(crate) struct NodeScene {
    pub root: Node,
}
impl Scene for NodeScene {
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(SceneRoot::new(*self.root, frame.viewport));
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        render.draw(*self.root)?;
        Ok(())
    }
}
