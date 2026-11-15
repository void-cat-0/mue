//! Scenes own application content; App owns frame scheduling.
use crate::{
    input::InputBatch,
    math::{Rect, Vector},
    node::{Node, NodeRef},
};
use anyhow::{Result, ensure};
use std::any::Any;

/// How a displayed root participates in pointer routing.
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
/// The Scene retains its owner through input, preparation and drawing.
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

/// One application frame. UI delta time and host time are not an audio clock.
#[derive(Clone, Copy, Debug)]
pub struct SceneFrame {
    pub dt: f32,
    pub time: f64,
    pub viewport: Rect,
}
impl SceneFrame {
    pub fn size(&self) -> Vector {
        self.viewport.size()
    }
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
    /// Scene's internal game-pause state automatically.
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

/// Frame data and a deferred request applied after input and update callbacks.
pub struct SceneContext<'a> {
    pub frame: SceneFrame,
    pub(crate) command: &'a mut Option<SceneCommand>,
}
impl SceneContext<'_> {
    /// Cover the current Scene without dropping it. Only the new top is scheduled.
    /// The current Scene pauses before the new Scene enters at the safe boundary.
    pub fn push(&mut self, scene: impl Scene + 'static) {
        *self.command = Some(SceneCommand::Push(Box::new(scene)));
    }
    /// Pop the active top at the next safe boundary. Removing the last Scene
    /// ends the App, not the host process. Requests target the top at commit time.
    pub fn pop(&mut self) {
        *self.command = Some(SceneCommand::Pop(None));
    }
    /// Pop and deliver an owned business result to the newly exposed Scene before
    /// it resumes. A result without a receiving Scene is dropped normally.
    /// Pass the concrete business value, not an already boxed SceneResult.
    pub fn pop_with_result(&mut self, result: impl Any) {
        *self.command = Some(SceneCommand::Pop(Some(Box::new(result))));
    }
    /// Replace only the top Scene, preserving any covered Scenes below it.
    /// A later request in the same frame replaces the earlier request.
    pub fn replace(&mut self, scene: impl Scene + 'static) {
        *self.command = Some(SceneCommand::Replace(Box::new(scene)));
    }
    /// End the whole App and release its stack. The host owns loop termination.
    pub fn exit(&mut self) {
        *self.command = Some(SceneCommand::Exit);
    }
}

/// A retained Scene owning its Node trees and gameplay/custom drawing resources.
/// App schedules only the top Scene; covered instances retain their state.
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
    fn event(&mut self, _event: SceneEvent<'_>, _ctx: &mut SceneContext<'_>) -> Result<()> {
        Ok(())
    }
    /// Advance ordinary frame-driven business. Not called on a covered Scene or
    /// the newly activated top in a switching frame; notifications are not updates.
    fn update(&mut self, _ctx: &mut SceneContext<'_>) -> Result<()> {
        Ok(())
    }
    /// Submit independent roots. Do not draw or advance time here.
    /// App may call this again after node updates change root membership.
    fn layout(&mut self, _frame: &SceneFrame, _roots: &mut Vec<SceneRoot>) -> Result<()> {
        Ok(())
    }
    fn render(&mut self, _render: &mut SceneRender<'_>) -> Result<()> {
        Ok(())
    }
}

/// Controlled UI drawing inside a Scene's custom drawing sequence.
/// draw() records actual root order for the next input batch; omitted roots do
/// not receive new input. External rendering belongs between complete draw calls.
pub struct SceneRender<'a> {
    pub frame: SceneFrame,
    pub(crate) prepared: &'a [SceneRoot],
    pub(crate) drawn: Vec<SceneRoot>,
}
impl SceneRender<'_> {
    pub fn draw(&mut self, node: NodeRef) -> Result<()> {
        let root = self
            .prepared
            .iter()
            .find(|root| root.node == node)
            .ok_or_else(|| {
                anyhow::anyhow!("Scene tried to draw a root it did not submit for layout")
            })?;
        ensure!(
            !self.drawn.iter().any(|root| root.node == node),
            "Scene root drawn more than once in one frame"
        );
        ensure!(
            crate::runtime::Runtime::with(|rt| rt.nodes.borrow().contains_key(node.id)),
            "Scene tried to draw a disposed root"
        );
        node.render();
        self.drawn.push(*root);
        Ok(())
    }
}

pub(crate) struct NodeScene {
    pub root: Node,
}
impl Scene for NodeScene {
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        roots.push(SceneRoot::new(*self.root, frame.viewport));
        Ok(())
    }
    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        render.draw(*self.root)
    }
}
