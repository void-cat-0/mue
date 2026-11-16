# Scenes

`App::with_scene(scene)` starts a retained Scene stack with one active top. A Scene retains its Node trees, gameplay state and resources, submits layout roots, and chooses the order of custom drawing and UI. Only the top receives Scene events and participates in update/layout/render and Node updates; covered instances remain owned but are not automatically drawn behind it. `App::new(root)` remains the Node-only convenience entry point and uses the same scheduler.

```rust
use anyhow::Result;
use mue_macroquad::{
    App, Scene, SceneFrame, SceneRender, SceneRoot,
    node::{Node, IntoNode, flexbox},
    style::{Styleable, StyleableExt},
};

struct Menu { ui: Node }

impl Scene for Menu {
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        roots.push(SceneRoot::new(*self.ui, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        // Custom gameplay/background drawing can precede this call.
        render.draw(*self.ui)?;
        // Custom foreground drawing can follow it.
        Ok(())
    }
}

// Construct after macroquad starts.
let mut app = App::with_scene(Menu { ui: flexbox().size_full().into_node() });
// In the host loop: app.try_frame()?; macroquad::window::next_frame().await;
```

See the [scenes example](../examples/scenes/README.md) for a runnable gameplay/HUD/pause layer, push/pop return values and retained-state round trips.

## Scene interface

A Scene has four entry points. `event` responds to notifications; `update` advances ordinary frame-driven business; `layout` submits UI roots; `render` organizes drawing. Notifications keep their own timing instead of being delayed until `update`.

```rust
fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()>;
fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()>;
fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()>;
fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()>;
```

`SceneEvent` is exported at the crate root and contains `Enter`, `Pause`, `Resume`, `Input(&InputBatch)` and `Returned(SceneResult)`. Match only the notifications your Scene needs. The default `event` ignores them, dropping any owned returned value. `update`, `layout` and `render` also have no-op defaults. Earlier independent `enter/input/on_result/pause/resume` trait methods have been removed, not retained as wrappers; migrate their bodies to the corresponding event arms.

## Frame scheduling

A normal frame preplans Node input for the complete batch against roots actually drawn in the last successful frame. It then delivers any pending `Returned` values and required `Enter`/`Resume` notifications, automatically dispatches UI input and calls `Scene::event(SceneEvent::Input(&batch), ctx)` with the complete, unfiltered raw batch. UI dispatch does not consume events from that batch. Global transition, timeout and ready-task work runs once, followed by the optional `App::frame_with_update` host callback and `Scene::update`. An empty input batch is still a normal Input notification; Enter/Pause/Resume/Returned are not per-frame updates.

The Scene submits roots through `layout`; App prepares all geometry, snapshots all active nodes across every root, calls each node update once, asks for roots again, and prepares final geometry. Finally `Scene::render` interleaves custom drawing with `SceneRender::draw(node)`. Frame-end texture deletion remains App-owned. Layout hooks and root submission may repeat; neither should advance time.

`SceneFrame` contains `dt`, host `time` and a logical window `viewport`. Global time is never rewound when returning to a Scene. Game audio/judgement may use its own clock. A switching frame skips the newly active Scene's ordinary update, but prepares/draws its roots and updates its nodes once with `dt = 0`. On the first normal host-resume frame, Scene callbacks and Node updates also receive `dt = 0`, not the time spent suspended. Initial App entry otherwise keeps the ordinary frame delta. Nodes mounted during the node-update snapshot start updating next frame. Roots newly submitted in the second layout pass are prepared and may draw immediately, without an extra node update.

## Layout roots and drawing

`SceneRoot` borrows a `NodeRef`; the Scene keeps the owning Node alive. Its `bounds` provide a logical origin and available width/height, matching `NodeRef::prepare_layout_in(bounds)`. These are available-space constraints, not an override of explicit root dimensions and not an implicit drawing clip. Root styling still controls layout; use Node clipping when needed. The submitted bounds also limit new Node input queries for that root; Node capture can continue outside them. They do not limit Scene raw input.

Roots must be independent. Duplicate/shared nodes, ancestor/descendant submissions and a layout root attached to another Taffy tree return an error. App snapshots the whole forest before any node update, and stabilizes geometry across roots so later rect observers cannot leave an earlier root stale.

Call `render.draw(root)` for each submitted tree at its intended position in the drawing sequence. The order of these calls determines Node input stacking in the next frame; the submission Vec does not. A submitted root omitted from drawing receives no new Node input. The controlled entry point rejects unsubmitted, disposed or repeatedly drawn roots and brackets the existing UI text-atlas pass. Custom camera/target/pass operations belong between complete UI draw calls and retain the [existing rendering contract](phira-integration.md#33-app-passcamera-reset).

During Scene rendering, call this entry point rather than bypassing it with `NodeRef::render()`, which cannot record displayed membership. Render callbacks should only draw or update rendering caches. Neither a borrowed root list nor reactive `batch` is a full display snapshot: keep hit-test state unchanged from drawing until the next preplanning phase.

## Input

Root input policy applies only to routing between Node trees. None of these policies filters `SceneEvent::Input`:

| Policy | Node behavior |
| --- | --- |
| `RootInput::None` | The root is prepared/updated/drawn but new Node input passes to lower roots. |
| `RootInput::Interactive` | A path with pointer, gesture, custom-hit, hover or wheel handling is routed to this Node tree; empty layout space passes to lower Node roots. |
| `RootInput::Modal` | New UI input inside the submitted bounds is blocked from lower Node roots, including empty space. Outside the bounds it can pass through. Scene raw input is unaffected. |

`InputPlan` and `PointerManager` preplan and dispatch Node paths, capture and gestures; they do not assign presses to UI versus Scene. Pointer, hover and wheel use the preplanned UI paths before user callbacks. Hover/wheel-only paths also participate in Node routing; `App::hit_test_ui` retains its narrower pointer-interactive query. Wheel targets the cursor independently of pressed-button capture. A callback opening another root cannot redirect later Node events of that batch. Node capture continues from Down through Move/Up/Cancel; hiding/removing a captured UI root cancels its Node capture, not the Scene's raw events. Existing Node capture is independent of new bounds or a newly visible modal layer.

The `SceneEvent::Input` arm receives the complete original batch in its original event order, with physical positions and touch timestamps unchanged. This includes Down on UI, wheel over UI, events inside a modal root and Move/Up without a matching Down in that Scene. Replay with `InputBatch::repeat(&mut handler)` and handle `is_interrupted()` first. Keyboard/character/native window events are also available. No new backend subscription or per-Scene queue is created.

App currently dispatches UI automatically before delivering `SceneEvent::Input` to `Scene::event`; this API does not expose per-event consume or a way for Scene raw handling to undo that dispatch. A Scene decides which raw events count as gameplay, but must manage its own modal, pause and contact policies, including unmatched Move/Up and Hold-on-resume if needed. Seeing a raw Down is not a gameplay hit-test result. No synthetic Down or public contact-snapshot API is provided.

Earlier revisions used `SceneInputRouter` to filter batches by UI/Scene press ownership. That filter has been withdrawn; Node capture/gesture routing remains separate from the complete Scene raw batch.

First-frame Scene UI becomes eligible after its first successful drawing; Scene raw input does not wait for UI geometry. The Node-only `App::new(root)` preserves existing custom first-frame hit bounds. Resize/DPI changes do not discard batches or cancel Node capture. `frame_paused` drains suspended input and cancels old UI state; fresh input on the first normal frame is accepted. This lifecycle cleanup, and draining old input while faulted, is not UI/Scene ownership filtering.

### Continuing touches across replacement

```text
Loading Scene: receive Down and keep the finger pressed
→ App cancels the old Node capture and replaces the Scene
→ Game Scene: receive subsequent raw Move/Up for that same touch ID
```

The completed loading-frame batch is not replayed into the new Scene. Later Move/Up do not require that Scene to have seen Down; the game decides whether those contacts participate in hold/drag judgments. An unmatched Up cannot by itself create a Node tap, and no replacement Down is synthesized.

A motionless finger may produce no new native events. A complete event batch is not a snapshot of all currently held contacts: this API does not automatically initialize the new game with every stationary finger. Retaining or synchronizing contact state is an application policy until a separate contact-state API is implemented.

## Lifecycle and commands

- `SceneEvent::Enter` is delivered once per instance before its first normal Input/update. Returning to a retained Scene does not repeat Enter or rebuild its Node trees.
- `SceneEvent::Pause` is delivered once when a running instance loses permission to run: push coverage, host suspension or permanent removal. App cancels its old UI capture/hover first. Covered/already paused instances do not receive another Pause, and a never-entered Scene receives no spurious Pause/Resume pair.
- `SceneEvent::Resume` is delivered when an entered instance becomes runnable again. Being top is not sufficient while the host is suspended. Game pause is still internal Scene/session state; Resume does not automatically clear a user's game pause.
- `SceneEvent::Returned(value)` delivers an owned business value before reactivation. No-result pop still reactivates the receiver: Resume for an entered instance, first Enter for a new instance, or deferred activation while the host is suspended. Scene fields and their Node owners remain the same instances, but canceled UI capture/gestures are not revived.

A normal running frame is `event(Input) → global work → update → layout/render`. For an entered receiver while the host is running, a pop return is `event(Returned) → event(Resume) → layout/render`; it does not add an ordinary update to the switching frame. Combining notifications in one method does not combine these scheduling phases.

### Stack requests

| Request | Safe-boundary behavior |
| --- | --- |
| `ctx.push(child)` / `app.push_scene(child)` | Pause the current top, retain it, push the child, then enter the child once when runnable. |
| `ctx.pop()` / `app.pop_scene()` | Pause/drop the top and resume the exposed retained instance. Pop of the last Scene sets `is_exited()`. |
| `ctx.pop_with_result(value)` / `app.pop_scene_with_result(value)` | Pop the top, deliver the result to the exposed Scene, then resume it. With no receiver, drop the value normally and exit. |
| `ctx.replace(next)` / `app.replace_scene(next)` | Replace only the top; covered instances remain retained. A replacement on an exited App starts a new root. |
| `app.reset_scene(root)` | Release the whole old stack and install a new root, without treating the intermediate empty stack as exit. |
| `ctx.exit()` | Release the whole stack and set `is_exited()`. The host, not the library, ends its outer loop/process. |

Requests can be made from context-bearing Scene callbacks. They commit after the current input batch and global/host updates, before layout/render; a pending request skips the departing Scene's ordinary update. At most one topology change commits per application frame. The chosen command is taken before boundary callbacks: requests from a retained Scene's pause or the new top's result/enter/resume wait until the next frame. A permanently removed Scene's pause requests are dropped before its owner, so they cannot accidentally pop/replace its parent or retain invalid handles; unrelated host/task requests remain separate. A later request before a boundary replaces the earlier one. Initial enter or a host-resume/result callback before ordinary input may request that frame's one safe-boundary change. Explicit exit/last pop discard remaining Scene-originated commands; the host can explicitly start a new instance afterward. `scene_count()` reports the retained count before any queued command is committed.

A newly exposed/installed top can draw in the switching frame, but receives neither the completed old input batch nor an ordinary Scene update. Subsequent batches reach it in full, including continuing Move/Up whose Down it never saw. App does not synthesize Down, restore the old Node capture or suppress raw contacts until release.

### Return values

`SceneResult` is `Box<dyn Any>` with no Send bound; the receiving Scene defines a concrete business type and downcasts it. The default `event` drops unhandled Returned values. This is runtime type erasure, not compile-time parent/child result matching; handle an unexpected type explicitly. Pass a concrete business value to `pop_with_result`/`pop_scene_with_result`, not an already boxed `SceneResult` (which would be boxed again). Results must own or share any resources needed after the child is dropped; they cannot rely on child-owned reactive handles staying alive.

```rust
use anyhow::{bail, Result};
use mue_macroquad::{Scene, SceneContext, SceneEvent};

struct PlayResult { score: u32 }
struct SongDetails { last_score: Option<u32> }
impl Scene for SongDetails {
    fn event(&mut self, event: SceneEvent<'_>, _ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Returned(value) => {
                let Ok(result) = value.downcast::<PlayResult>() else {
                    bail!("expected PlayResult");
                };
                self.last_score = Some(result.score);
            }
            SceneEvent::Resume => {
                // Refresh from the already-applied score; don't reset user pause.
            }
            _ => {}
        }
        Ok(())
    }
}
// A child requests: ctx.pop_with_result(PlayResult { score: 1_000_000 });
```

### Host suspension and resource ownership

`try_frame_paused()` drains suspended input, cancels UI, pauses the running top on the first edge and ticks timeout/tasks once. It may commit one already queued instantaneous stack request; animated requests over an entered source wait for normal rendering. It runs no layout/drawing/Node/effect updates and delivers no Enter/Resume/Returned notifications. A returned value remains queued on its receiving instance, even if that instance is covered again; multiple returns use FIFO order. The first normal frame delivers those Returned values before Enter/Resume. A result receiver that never entered (for example after host-side stack changes while suspended) receives its pending Returned values before first Enter, not a fake Resume. An unchanged top retains its last displayed UI geometry so fresh input is accepted immediately on host resume; topology changes clear that eligibility until the new top draws.

App does not pause/cancel arbitrary tasks merely because their Scene is covered. Scene-owned Task handles stay owned; dropping them requests cancellation under the existing caller-owned rules, while detached or host-side work may continue. Field/drop ownership must keep resources alive as long as needed. Normal permanent removal pauses a running top before releasing it, but App destruction does not run fallible lifecycle callbacks. Stacks are released top-down. Optional draw-only [Scene transitions](scene-transitions.md) can delay a stack commit; asynchronous lifecycle callbacks, cross-Scene business/input propagation and automatic audio/contact policy are not implemented.

## Application-owned resources and settings

The [song-transition application](<../examples/song-transition/README.md>) demonstrates a concrete ownership pattern with existing APIs, not a new framework container. Its [host](<../examples/song-transition/src/application.rs>) owns App, an immutable `Rc<Catalog>` and Settings; Settings keeps an independent `OwnedScope` alive for theme/reduced-motion signals. App is the first field and therefore drops before those external resources. Library/Details constructors receive read-only settings and an editor-Scene factory; only SettingsScene receives writable access.

Each independently owned view explicitly provides a typed `ReadSettings` in its detached construction scope. Deep UI components inject during synchronous build and retain the handles they need; Scene callbacks do not automatically enter a Scene scope, and scope context follows the construction chain rather than the Scene stack. Local maps/computed belong to the view or node owner. Theme bindings update retained UI without resetting selection/scroll, while artwork transitions sample the reduced-motion preference at request time. Settings pops without a result; Details still returns its typed selected index even when animation is bypassed.

Signal/Scope copies do not keep their owner alive; owning Rc assets do. Keep the global owner alive through all consumers and teardown. This example has no asynchronous tasks: if a consumer adds caller-owned or host tasks, cancel/finish them and drain foreground cleanup before disposing shared reactive resources. Dropping App or a Task handle is not a general synchronous task join.

## Loading flows and picture transitions

The [scene-flow consumer](../examples/scene-flow/README.md) demonstrates retained details → Loading/Failure/Ready/Running/Results → return or retry using these existing entry points. Its Request owns a foreground Task awaiting CPU-only blocking work; normal `update` consumes an accepted generation and uploads a GPU texture. Ready does not automatically start the session. Cancel revokes publication before pop, and a late worker result cannot reactivate the cancelled flow. Internal business states do not each require another Scene.

The consumer now attaches the public [Scene transition interface](scene-transitions.md) to push/pop requests. `SceneRequest::transition` configures one deferred switch; `SceneTransition::crossfade/composite/overlay` share App-owned timing, while custom drawing stays replaceable. `AfterSwitch` commits before animation; `AroundSwitch` presents an exit endpoint before committing at a later safe boundary. Constructors/default-free applications are GPU-free; only selected composites capture opaque pictures. The retired Scene is not kept running by the effect. The previous example-local Presenter has been removed. Internal Loading/Ready/Running/Results state changes remain within one Scene and do not automatically animate.

The consumer submits roots with `RootInput::None` until the actual current picture has drawn and the public transition is complete. Raw Scene input remains complete; Esc/back and gameplay policy are Scene-owned. Host suspension freezes effect time; the first restoration delta is zero. Resize rebuilds picture buffers without restarting the clock or resetting input/Scene state; if an old source is gone, composites use the live picture for both inputs. GPU composition currently requires OpenGL/default-camera/full origin-zero window rendering, not arbitrary camera/pass capture. Overlay effects allocate no picture targets. See [the transition tutorial](scene-transitions.md) and the examples for exact timing, interruption, input and ownership rules.

## Errors

Scene callbacks return `anyhow::Result<()>`. `App::try_frame` / `try_frame_with_update` / `try_frame_paused` return errors with phase context. Any callback error stops the rest of that frame, clears displayed-root eligibility, cancels UI capture/hover and marks the App faulted. Later ordinary frames drain old input and return an error without retrying the failed Scene. The host can explicitly call `replace_scene` to replace the failed top or `reset_scene` to abandon the whole stack; a recovery frame drains old input. Push/pop cannot recover a faulted App, and explicit exit is still allowed. An error from any event arm stops scheduling just like update/layout/render errors. Error context still identifies the notification phase (`Scene input/result/enter/pause/resume`) rather than hiding it behind a generic event error. A selected transition is not committed if the old pause fails; failure after pop/installation does not roll back already dropped Scenes or business side effects. Failed results are not retried; reset/drop releases any remaining pending values. Faulted-frame draining and UI cancellation are lifecycle cleanup, not a press-ownership filter on a normal Scene raw batch. This does not restore a partially rendered framebuffer or recover panics.

`frame`, `frame_with_update` and `frame_paused` retain convenience signatures and panic on a returned Scene error. Fallible applications should use the `try_` variants. Existing Node hook/layout panics still follow the project's fatal-panic policy.
