# Scene runtime

[Reference contents][guide-reference] · [Related lesson][scenes-guide]

- [Scene interface](#scene-interface)
- [Frame scheduling](#frame-scheduling)
- [Layout roots and drawing](#layout-roots-and-drawing)
- [Input](#input)
- [Lifecycle and commands](#lifecycle-and-commands)
- [Errors](#errors)
- [Scene transitions](#scene-transitions)

## Active Scene and retained stack

`App::with_scene(scene)` starts a retained Scene stack with one active top. A Scene retains its Node trees, gameplay state and resources, submits layout roots, and chooses the order of custom drawing and UI.

Only the top receives Scene events and participates in update/layout/render and Node updates; covered instances remain owned but are not automatically drawn behind it. `App::new(root)` remains the Node-only convenience entry point and uses the same scheduler; it wraps the root in a `layer_host` so teleports, overlays, and popovers work. A custom Scene builds each root that should host overlays as `layer_host(view).into_node()` (see [layers][layers-reference]).

```rust,no_run
use mue::{
    App, Scene, prelude::*,
    renderer::{CallbackResult, SceneFrame, SceneRender, SceneRoot},
};

struct Menu { ui: Node }

impl Scene for Menu {
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(SceneRoot::new(*self.ui, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
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

See the [scenes example](https://github.com/void-cat-0/mue/blob/main/examples/scenes/README.md) for a runnable gameplay/HUD/pause layer, push/pop return values and retained-state round trips.

## Scene interface

A Scene has four entry points. `event` responds to notifications; `update` advances ordinary frame-driven business; `layout` submits UI roots; `render` organizes drawing. Notifications keep their own timing instead of being delayed until `update`.

```text
fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()>;
fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()>;
fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()>;
fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()>;
```

`SceneEvent` is exported at the crate root and contains `Enter`, `Pause`, `Resume`, `Input(&InputBatch)` and `Returned(SceneResult)`. Match only the notifications your Scene needs.

The default `event` ignores them, dropping any owned returned value. `update`, `layout` and `render` also have no-op defaults.

Earlier independent `enter/input/on_result/pause/resume` trait methods have been removed, not retained as wrappers; migrate their bodies to the corresponding event arms.

## Frame scheduling

A normal frame preplans Node input for the complete batch against roots actually drawn in the last successful frame. It then delivers any pending `Returned` values and required `Enter`/`Resume` notifications, automatically dispatches UI input and calls `Scene::event(SceneEvent::Input(&batch), ctx)` with the complete, unfiltered raw batch.

UI dispatch does not consume events from that batch. Global transition, timeout and ready-task work runs once, followed by the optional `App::frame_with_update` host callback and `Scene::update`.

An empty input batch is still a normal Input notification; Enter/Pause/Resume/Returned are not per-frame updates.

The Scene submits roots through `layout`; App prepares all geometry, snapshots all active nodes across every root, calls each node update once, asks for roots again, and prepares final geometry. Finally `Scene::render` interleaves custom drawing with `SceneRender::draw(node)`.

Frame-end texture deletion remains App-owned. Layout hooks and root submission may repeat; neither should advance time.

Use this frame order when deciding where to sample a value or issue a request.

| Phase | State you can use | Example |
| --- | --- | --- |
| Plan Node input | Roots from the last successful drawing | A root you create in this frame cannot take the current press. |
| Dispatch UI, then deliver raw input | Effects of the Node handlers that just ran | A Node button can set a pause flag that the Scene checks in its raw input handler. |
| Run tasks and host update | Completed foreground work and host changes | Publish a loaded file before the next layout. |
| Resolve stack request | The last pending command at the boundary | Push the destination of a selected menu item. |
| Prepare and draw | The current top and its submitted roots | Draw the arena, then its HUD. |

If the window temporarily has no drawable area, a normal frame still handles input and business work but skips Node layout, Node updates, presentation and render-thread texture deletion. Animated switches wait for drawable frames.

Use `try_frame_paused` when the host actually suspends.

`SceneFrame` contains `dt`, host `time` and a logical window `viewport`. Global time is never rewound when returning to a Scene.

Game audio/judgement may use its own clock. A switching frame skips the newly active Scene's ordinary update, but prepares/draws its roots and updates its nodes once with `dt = 0`.

On the first normal host-resume frame, Scene callbacks, Node updates, and frame-driven style and Scene transitions receive `dt = 0`. Wall-clock time and timer deadlines are not reset.

App measures subsequent normal-frame deltas at its own frame boundary, so it does not apply the suspended interval on the following frame. Nodes mounted during the node-update snapshot start updating next frame.

Roots newly submitted in the second layout pass are prepared and may draw immediately, without an extra node update.

## Layout roots and drawing

`SceneRoot` borrows a `NodeRef`; the Scene keeps the owning Node alive. Its `bounds` provide a logical origin and available width/height, matching `NodeRef::prepare_layout_in(bounds)`.

These are available-space constraints, not an override of explicit root dimensions and not an implicit drawing clip. Root styling still controls layout; use Node clipping when needed.

The submitted bounds also limit new Node input queries for that root; Node capture can continue outside them. They do not limit Scene raw input.

Roots must be independent. Duplicate/shared nodes, ancestor/descendant submissions and a layout root attached to another Taffy tree return an error. App snapshots the whole forest before any node update, and stabilizes geometry across roots so later rect observers cannot leave an earlier root stale.

Call `render.draw(root)` for each submitted tree at its intended position in the drawing sequence. The order of these calls determines Node input stacking in the next frame; the submission Vec does not.

A submitted root omitted from drawing receives no new Node input. The controlled entry point rejects unsubmitted, disposed or repeatedly drawn roots and brackets the existing UI text-atlas pass.

Custom camera/target/pass operations belong between complete UI draw calls. See the [`mue-macroquad` Rustdoc][renderer-api] and [`RenderStateGuard`][render-state-guard-api] for the host rendering contract.

During Scene rendering, call this entry point rather than bypassing it with `NodeRef::render()`, which cannot record displayed membership. Render callbacks should only draw or update rendering caches.

Neither a borrowed root list nor reactive `batch` is a full display snapshot: keep hit-test state unchanged from drawing until the next preplanning phase.

## Input

Root input policy applies only to routing between Node trees. None of these policies filters `SceneEvent::Input`:

| Policy | Node behavior |
| --- | --- |
| `RootInput::None` | The root is prepared/updated/drawn but new Node input passes to lower roots. |
| `RootInput::Interactive` | A path with pointer, gesture, custom-hit, hover or wheel handling is routed to this Node tree; empty layout space passes to lower Node roots. |
| `RootInput::Modal` | New UI input inside the submitted bounds is blocked from lower Node roots, including empty space. Outside the bounds it can pass through. Scene raw input is unaffected. |

`InputPlan` and `PointerManager` preplan and dispatch Node paths, capture and gestures; they do not assign presses to UI versus Scene. Pointer, hover and wheel use the preplanned UI paths before user callbacks.

Hover/wheel-only paths also participate in Node routing; `App::hit_test_ui` retains its narrower pointer-interactive query. Wheel targets the cursor independently of pressed-button capture.

A callback opening another root cannot redirect later Node events of that batch. Node capture continues from Down through Move/Up/Cancel.

For a HUD and a modal in the same Scene, submit both roots and draw the modal last. Its bounds block lower Node roots even between buttons. Gate gameplay in the raw handler as a separate step.

```rust
use mue::renderer::{RootInput, SceneFrame, SceneRoot, node::NodeRef};

fn add_roots(
    frame: &SceneFrame,
    roots: &mut Vec<SceneRoot>,
    hud: NodeRef,
    modal: Option<NodeRef>,
) {
    roots.push(SceneRoot::new(hud, frame.viewport));
    if let Some(modal) = modal {
        roots.push(SceneRoot::new(modal, frame.viewport).input(RootInput::Modal));
    }
}
```

Detaching or hiding a node with `show_if` on the captured path, or omitting its root from drawing, revokes the capture without filtering Scene raw events. A path absent from a presented frame cannot regain the press by reappearing before the next input.

Existing Node capture does not depend on current pointer bounds or a newly visible modal layer.

The `SceneEvent::Input` arm receives the complete original batch in its original event order, with physical positions and touch timestamps unchanged. This includes Down on UI, wheel over UI, events inside a modal root and Move/Up without a matching Down in that Scene.

Replay with `InputBatch::repeat(&mut handler)` and handle `is_interrupted()` first. Keyboard/character/native window events are also available.

No new backend subscription or per-Scene queue is created.

App currently dispatches UI automatically before delivering `SceneEvent::Input` to `Scene::event`; this API does not expose per-event consume or a way for Scene raw handling to undo that dispatch. A Scene decides which raw events count as gameplay, but must manage its own modal, pause and contact policies, including unmatched Move/Up and Hold-on-resume if needed.

Seeing a raw Down is not a gameplay hit-test result. No synthetic Down or public contact-snapshot API is provided.

Earlier revisions used `SceneInputRouter` to filter batches by UI/Scene press ownership. That filter has been withdrawn; Node capture/gesture routing remains separate from the complete Scene raw batch.

First-frame Scene UI becomes eligible after its first successful drawing; Scene raw input does not wait for UI geometry. The Node-only `App::new(root)` preserves existing custom first-frame hit bounds.

Resize/DPI changes do not discard batches or cancel Node capture. Node hit-testing and dispatch use the event's source DPI scale when available, and the current window scale otherwise.

The raw batch is not rescaled. `frame_paused` drains suspended input and cancels old UI state; fresh input on the first normal frame is accepted.

This lifecycle cleanup, and draining old input while faulted, is not UI/Scene ownership filtering.

### Continuing touches across replacement

```text
Loading Scene: receive Down and keep the finger pressed
→ App cancels the old Node capture and replaces the Scene
→ Game Scene: receive subsequent raw Move/Up for that same touch ID
```

The completed loading-frame batch is not replayed into the new Scene. Later Move/Up do not require that Scene to have seen Down; the game decides whether those contacts participate in hold/drag judgments. An unmatched Up cannot by itself create a Node tap, and no replacement Down is synthesized.

A motionless finger may produce no new native events. A complete event batch is not a snapshot of all currently held contacts: this API does not automatically initialize the new game with every stationary finger. Retaining or synchronizing contact state is an application policy until a separate contact-state API is implemented.

### Pointer and gesture hooks

| Hook | What to expect |
| --- | --- |
| `on_pointer_event` | A routed logical pointer event. Track `Down`, `Move`, `Up`, and `Cancel` by pointer ID. `position` is the latest sample, `start_position` is the original press, and `delta` is displacement since the previous sample. |
| `on_hover_event` | Hover entry, movement, or exit. Hover follows the current pointer path, independent of a pressed pointer's capture. |
| `on_wheel_event` | Wheel movement on the path under the cursor. A scroll area can consume wheel input or chain it to an ancestor. |
| `on_tap`, `on_long_press`, `on_swipe` | Discrete gesture callbacks. Use a control's activation hook when the control supplies one. |
| `on_pan`, `on_pinch`, `on_rotate`, `on_transform` | Continuous gesture callbacks with `Start`, `Update`, `End`, and `Cancel` phases. End and Cancel follow Start. |

Custom recognizers implement `Gesture::on_event` and receive a `ClaimToken`. Claim only once you recognize the motion. Eligibility depends on the live capture path and other recognizers' claims. A recognizer can lose eligibility when a captured node detaches. For coordinated gestures that need several pointers, use the token's batch claim operation to claim the whole set or leave it unchanged.

### Read raw input from a host cursor

Inside a Scene, use the `InputBatch` borrowed by `SceneEvent::Input`. A separate host subsystem, such as another renderer, can own an `InputSubscriber`. Each cursor reads from the same host stream without consuming events for other subscribers. New cursors start after events that preceded construction. Read from the main application future before yielding to `next_frame`. Events expire at the end of the frame, so clear held input on interruption instead of waiting for a release you might have missed.

```rust
use mue::{macroquad::miniquad::EventHandler, renderer::input::InputSubscriber};

fn collect_for_host(cursor: &mut InputSubscriber, host: &mut impl EventHandler,
    mut clear_held: impl FnMut())
{
    let batch = cursor.read();
    if batch.is_interrupted() {
        clear_held();
    }
    batch.repeat(host);
}
```

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

Requests can be made from context-bearing Scene callbacks. They commit after the current input batch and global/host updates, before layout/render; a pending request skips the departing Scene's ordinary update.

At most one topology change commits per application frame. The chosen command is taken before boundary callbacks: requests from a retained Scene's pause or the new top's result/enter/resume wait until the next frame.

When an operation actually removes a Scene at that boundary, its Pause request is dropped before its owner and cannot affect its parent. If an animated operation only starts waiting, the source's Pause request becomes eligible at the next boundary and can interrupt the waiting operation.

A later request before a boundary replaces the earlier one. Initial enter or a host-resume/result callback before ordinary input may request that frame's one safe-boundary change.

Explicit exit/last pop discard remaining Scene-originated commands; the host can explicitly start a new instance afterward. `scene_count()` reports the retained count before any queued command is committed.

A newly exposed/installed top can draw in the switching frame, but receives neither the completed old input batch nor an ordinary Scene update. Subsequent batches reach it in full, including continuing Move/Up whose Down it never saw. App does not synthesize Down, restore the old Node capture or suppress raw contacts until release.

### Return values

`SceneResult` is `Box<dyn Any>` with no Send bound; the receiving Scene defines a concrete business type and downcasts it. The default `event` drops unhandled Returned values.

This is runtime type erasure, not compile-time parent/child result matching; handle an unexpected type explicitly. Pass a concrete business value to `pop_with_result`/`pop_scene_with_result`, not an already boxed `SceneResult` (which would be boxed again).

Results must own or share any resources needed after the child is dropped; they cannot rely on child-owned reactive handles staying alive.

```rust
use mue::{Scene, renderer::{CallbackResult, SceneContext, SceneEvent}};
use std::io;

struct PlayResult { score: u32 }
struct SongDetails { last_score: Option<u32> }
impl Scene for SongDetails {
    fn event(&mut self, event: SceneEvent<'_>, _ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        match event {
            SceneEvent::Returned(value) => {
                let Ok(result) = value.downcast::<PlayResult>() else {
                    return Err(io::Error::other("expected PlayResult").into());
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

`try_frame_paused()` drains suspended input, cancels UI, pauses the running top on the first edge and, if that succeeds, ticks timeout/tasks once. It may commit one already queued instantaneous stack request; animated requests over an entered source wait for normal rendering.

It runs no layout/drawing/Node/effect updates and delivers no Enter/Resume/Returned notifications. A returned value remains queued on its receiving instance, even if that instance is covered again; multiple returns use FIFO order.

The first normal frame delivers those Returned values before Enter/Resume. A result receiver that never entered (for example after host-side stack changes while suspended) receives its pending Returned values before first Enter, not a fake Resume.

A waiting AfterSwitch request can resume the old top for input and pause it again at the same frame's switch boundary. An unchanged top retains its last displayed UI geometry so fresh input is accepted immediately on host resume; topology changes clear that eligibility until the new top draws.

For external playback, stop immediately on `Pause`, but treat `Resume` as eligibility. Start audio from the first ordinary `update` in which the session should run, after processing user pause or stop actions. A switch can commit after the resumed Scene sees input but before it gets an update.

App does not pause/cancel arbitrary tasks merely because their Scene is covered. Scene-owned Task handles stay owned; dropping them requests cancellation under the existing caller-owned rules, while detached or host-side work may continue.

Field/drop ownership must keep resources alive as long as needed. Normal permanent removal pauses a running top before releasing it, but App destruction does not run fallible lifecycle callbacks.

Stacks are released top-down. Optional draw-only [Scene transitions][scene-transitions-guide] can delay a stack commit; asynchronous lifecycle callbacks, cross-Scene business/input propagation and automatic audio/contact policy are not implemented.

## Application-owned resources and settings

The [song-transition application](https://github.com/void-cat-0/mue/blob/main/examples/song-transition/README.md) demonstrates a concrete ownership pattern with existing APIs, not a new framework container. Its [host](https://github.com/void-cat-0/mue/blob/main/examples/song-transition/src/application.rs) owns App, an immutable `Rc<Catalog>` and Settings; Settings keeps an independent `OwnedScope` alive for theme/reduced-motion signals.

App is the first field and therefore drops before those external resources. Library/Details constructors receive read-only settings and an editor-Scene factory; only SettingsScene receives writable access.

Each independently owned view explicitly provides a typed `ReadSettings` in its detached construction scope. Deep UI components inject during synchronous build and retain the handles they need; Scene callbacks do not automatically enter a Scene scope, and scope context follows the construction chain rather than the Scene stack.

Local maps/computed belong to the view or node owner. Theme and [Fluent localization][i18n-guide] bindings update retained UI without resetting selection/scroll, while artwork transitions sample the reduced-motion preference at request time.

Settings pops without a result; Details still returns its typed selected index even when animation is bypassed. Locale/namespace Translators are passed explicitly to independently owned views; only SettingsScene receives the language-changing capability.

Signal/Scope copies do not keep their owner alive; owning Rc assets do. Keep the global owner alive through all consumers and teardown.

This example has no asynchronous tasks: if a consumer adds caller-owned or host tasks, cancel/finish them and drain foreground cleanup before disposing shared reactive resources. Dropping App or a Task handle is not a general synchronous task join.

## Loading flows and picture transitions

The [scene-flow consumer](https://github.com/void-cat-0/mue/blob/main/examples/scene-flow/README.md) demonstrates retained details → Loading/Failure/Ready/Running/Results → return or retry using these existing entry points. Its Request owns a foreground Task awaiting CPU-only blocking work; normal `update` consumes an accepted generation and uploads a GPU texture.

Ready does not automatically start the session. Cancel revokes publication before pop, and a late worker result cannot reactivate the cancelled flow.

Internal business states do not each require another Scene.

The consumer now attaches the public [Scene transition interface][scene-transitions-guide] to push/pop requests. `SceneRequest::transition` configures one deferred switch; `SceneTransition::crossfade/composite/overlay` share App-owned timing, while custom drawing stays replaceable.

`AfterSwitch` commits before animation; `AroundSwitch` presents an exit endpoint before committing at a later safe boundary. Constructors/default-free applications are GPU-free; only selected composites capture opaque pictures.

The retired Scene is not kept running by the effect. The previous example-local Presenter has been removed.

Internal Loading/Ready/Running/Results state changes remain within one Scene and do not automatically animate.

The consumer submits roots with `RootInput::None` until the actual current picture has drawn and the public transition is complete. Raw Scene input remains complete; Esc/back and gameplay policy are Scene-owned.

Host suspension freezes effect time; the first restoration delta is zero. Resize rebuilds picture buffers without restarting the clock or resetting input/Scene state; if an old source is gone, composites use the live picture for both inputs.

GPU composition currently requires OpenGL/default-camera/full origin-zero window rendering, not arbitrary camera/pass capture. Overlay effects allocate no picture targets.

See [the transition tutorial][scene-transitions-guide] and the examples for exact timing, interruption, input and ownership rules.

## Errors

Scene callbacks return `CallbackResult<()>` (`Result<(), mue_macroquad::BoxError>`), so application-owned `thiserror` errors can propagate with `?`. Callback failures are represented by `CallbackError` and retain their scheduling phase and source.

`SceneRender::draw` and Scene layout invariants use `SceneError`; transition picture/backend/material failures use `TransitionError`. `App::try_frame` / `try_frame_with_update` / `try_frame_paused` return `mue_macroquad::Result<()>` with `AppError` as its error type.

`AppError` wraps the domain errors. A `SceneError` propagated from `SceneRender::draw` through a render callback appears as the source of a `CallbackError`.

### Fault and recovery

Any callback error stops the rest of that frame, clears displayed-root eligibility, cancels UI capture/hover and marks the App faulted. Later ordinary frames drain old input and return `Error::Faulted` until the host requests recovery.

The host can explicitly call `replace_scene` to replace the failed top or `reset_scene` to abandon the whole stack; a recovery frame drains old input. Push/pop cannot recover a faulted App.

The host can also drop it; its fallible lifecycle callbacks do not run during destruction. A faulted frame without recovery drains input but does not drive ordinary timeouts or tasks, including while suspended.

An error from any event arm stops scheduling just like update/layout/render errors. Error context still identifies the notification phase (`Scene input/result/enter/pause/resume`) rather than hiding it behind a generic event error.

A selected transition is not committed if the old pause fails; failure after pop/installation does not roll back already dropped Scenes or business side effects. Failed results are not retried; reset/drop releases any remaining pending values.

Faulted-frame draining and UI cancellation are lifecycle cleanup, not a press-ownership filter on a normal Scene raw batch. This does not restore a partially rendered framebuffer or recover panics.

`frame`, `frame_with_update` and `frame_paused` retain convenience signatures and panic on a returned Scene error. Fallible applications should use the `try_` variants. Existing Node hook/layout panics still follow the project's fatal-panic policy.


[renderer-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/lib.rs
[render-state-guard-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/render.rs

## Scene transitions

[Back to the tutorial][scene-transitions-guide].

Scene transitions are optional presentation attached to a deferred Scene request. They do not add Scene callbacks, run two Scenes concurrently, start audio, or change the complete raw-input contract. The initial application default is **no animation**, with no picture allocation or shader work.

### Configure one request

`push`, `pop`, `pop_with_result`, `replace` and `exit` return a `SceneRequest` configuration handle. Host-side `push_scene`, `pop_scene`, `pop_scene_with_result`, `replace_scene` and `reset_scene` do the same.

```rust
use mue::{Scene, renderer::{SceneContext, SceneTransition, transition::Transition}};

struct Details;
impl Scene for Details {}

fn open(ctx: &mut SceneContext<'_>) {
    ctx.push(Details)
        .transition(SceneTransition::crossfade(Transition::ease(0.35)));
}

fn close(ctx: &mut SceneContext<'_>) {
    ctx.pop_with_result(42_u32)
        .transition(SceneTransition::crossfade(Transition::linear(0.25)));
}
```

The command is registered **before** the handle is returned: ignoring or forgetting it still queues the command. Drop never commits anything. A later request replaces the earlier command and effect together. `.without_transition()` bypasses an application default for one request.

`App::set_default_scene_transition(|| SceneTransition::crossfade(Transition::ease(0.25)))` installs a factory for a **fresh** effect at request selection. Only the final selected request invokes it; explicit overrides do not.

`clear_default_scene_transition()` restores no animation without cancelling an effect already running. Default-factory captures must own their resources and must not rely on Scene-local borrowed handles.

### Two timings, one scheduler

| Timing | Sequence |
| --- | --- |
| `AfterSwitch(curve)` | Optional outgoing picture capture → normal stack commit → current Scene and effect → complete |
| `AroundSwitch { exit, enter }` | Current Scene and exit effect → successfully presented exit endpoint → next normal safe boundary commits the stack → current Scene and enter effect → complete |

Follow the three frames around the commit when you need to know which Scene can handle an event.

| Presented frame | Scene receiving input before the boundary | Scene drawn |
| --- | --- | --- |
| Exit reaches 1 | Outgoing | Outgoing, with the exit endpoint applied |
| Following normal frame | Outgoing | Incoming, with the first enter sample applied |
| Enter reaches 1 | Incoming | Incoming, with the enter endpoint applied |

<figure class="mue-chart">
<div class="mue-motion">

![The previous Scene picture fades out while the new view fades in][shot-crossfade]

</div>
<div class="mue-still">

![Sunlit clearing at the end of the crossfade][shot-crossfade-end]

</div>
<figcaption>A crossfade uses AfterSwitch. The old pixels stay in the composite after the old Scene leaves the stack.</figcaption>
</figure>

Each non-instant segment first draws at raw progress zero, even if the frame delta is large. Subsequent normal presentation integrates finite positive `dt`; zero, negative and non-finite deltas do not advance it.

`TransitionFrame` exposes `phase`, `linear_progress`, eased `progress`, accepted `dt`, capped `elapsed` and `viewport`. Easing uses the existing property `Transition::sample`; the raw fraction determines completion, so spring overshoot cannot switch early.

Nonpositive/non-finite durations behave as instant segments, whose one endpoint still must be presented successfully.

A drawing/flush success acknowledges the endpoint, not a prediction during layout. An `AroundSwitch` exit endpoint is therefore visible while the old Scene is still top; commit occurs in the **following normal frame**, after that frame's input/global/host work.

A completed entering endpoint remains observable for that frame; the following normal frame retires the effect. These are render-thread submission guarantees, not OS/display scanout fences.

Hosts must keep the normal one-App-frame/`next_frame` loop to display frames.

Only the active top receives ordinary Scene/Node updates. During an exit segment it is still active and may continue business; stopping gameplay earlier is an explicit application decision.

After its exit endpoint has been presented, the departing Scene skips ordinary update in the commit frame; input/global/host work still precedes that boundary. Submit commands that must take effect on this last input, such as Quit, directly from `SceneEvent::Input` rather than relying on a later `update`.

Switching cancels old Node capture and preserves existing Pause/Enter/Returned-before-Resume semantics. New-top Scene update is skipped and its one Node update uses zero delta in the switching frame.

A terminal `pop`/`exit` has no entering Scene. `AroundSwitch` can present its exit, then ends the App without an enter segment; an `AfterSwitch` terminal request is instantaneous.

Never-entered sources and fault/exit recovery also commit instantaneously without outgoing drawing, default-factory invocation or manufactured Enter/Pause notifications.

### Picture composite

`SceneTransition::crossfade(curve)` is a built-in RGB mix with opaque output. `SceneTransition::composite(curve, callback)` uses the same `AfterSwitch` scheduling but lets the callback draw the whole composite. The callback receives borrowed `ScenePictures { from, to }`, valid only during that draw, and a `TransitionFrame`.

```rust
use mue::{
    macroquad::prelude::{DrawTextureParams, WHITE, draw_texture_ex, vec2},
    renderer::{SceneTransition, transition::Transition},
};

fn slide() -> SceneTransition {
    SceneTransition::composite(Transition::ease_out(0.4), |frame, pictures| {
        let v = frame.viewport;
        let p = frame.progress;
        for (texture, x) in [
            (pictures.from, v.x - v.w * p),
            (pictures.to, v.x + v.w * (1.0 - p)),
        ] {
            draw_texture_ex(texture, x, v.y, WHITE, DrawTextureParams {
                dest_size: Some(vec2(v.w, v.h)),
                flip_y: true,
                ..Default::default()
            });
        }
        Ok(())
    })
}
```

GPU resources are lazy: only a selected composite allocates three full-window targets (outgoing, live, presented). App reuses its built-in shaders across requests instead of recompiling each crossfade.

Successful source/live/custom-composite draws normalize alpha to one without changing RGB, so ordinary texture copies and interrupted composites do not darken translucent Scene content a second time. No ordinary CPU readback occurs; the output target is never one of the sampled inputs.

The old Scene is not retained by the effect after normal removal; the live Scene draws normally each frame.

When no interrupted composite exists, App captures the outgoing **current retained state** once before commit, with layout preparation but **no extra Scene or Node update**. This is not guaranteed to equal the preceding screen: input/global callbacks may already have changed state.

`SceneRender::is_transition_source()` identifies this capture, allowing consumers to avoid publishing display bookkeeping from it. For an interrupted composite, the last successfully presented composite becomes the next source without re-rendering the old Scene.

Multiple requests selected while suspended retain that same frozen source until one presents.

Resize/DPI changes discard invalid pictures, **not** the phase clock or Scene/input state. If the departed Scene can no longer be captured, `from` and `to` both reference the freshly rendered live picture for the remainder; timing continues. This is a defined fallback, not an old image stretched into a mismatched viewport or a restart.

Current composite restrictions: **OpenGL**, full origin-zero logical window viewport, default-camera host entry, no active raw miniquad pass, balanced model matrices and complete opaque pictures. All Scene/custom/UI rendering must retain App's target camera until returning; do not call `set_default_camera` or start a raw output pass inside a captured Scene.

Render-target texture copies require `flip_y: true`. Custom shader/resource setup belongs in the callback, lazily on the render thread.

Boundaries flush and restore default camera/pass plus the existing narrow material/mode/scissor/viewport guard, not arbitrary host GPU state. App initializes picture targets with an opaque draw that writes all color channels.

App also restores all-channel writes when leaving an effect. Within a callback or ordinary host drawing, submit an all-channel pipeline before `clear_background` if the last submitted draw used a partial mask.

Changing the selected QuadGl material alone does not apply its mask. Metal composite requests report an explicit error before allocating targets; overlay drawing has no composite backend restriction.

### Custom overlay without pictures

`overlay(timing, callback)` draws over the normally rendered Scene, using logical window coordinates. It allocates no picture targets and supports either timing. A closure can own an illustration, geometric mask or shader to express shared-asset/cover transitions without capturing full Scene pictures.

```rust
use mue::{
    macroquad::prelude::{Color, draw_rectangle},
    renderer::{SceneTransition, TransitionPhase, TransitionTiming, transition::Transition},
};

fn black_cover() -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::ease_out(0.25),
            enter: Transition::ease_out(0.25),
        },
        |frame| {
            let alpha = match frame.phase {
                TransitionPhase::BeforeSwitch => frame.progress,
                TransitionPhase::AfterSwitch => 1.0 - frame.progress,
            };
            let v = frame.viewport;
            draw_rectangle(v.x, v.y, v.w, v.h, Color::new(0., 0., 0., alpha));
            Ok(())
        },
    )
}
```

Callbacks only draw or maintain rendering caches, not mutate business/layout/hit-test state or enqueue recursive stack changes. They must own all resources they need through both phases: a copied `NodeRef`, `Signal` or scope handle does **not** keep its source owner alive.

Acquire owned texture/asset references rather than reading a disposed outgoing Scene. The callback can capture its own state; no new public effect trait/lifecycle is required.

### Observing a transition

`App::transition_state()`, `SceneContext::transition()` and `SceneRender::transition()` expose the effect sample. Input/update callbacks normally observe the preceding successful sample; a switching lifecycle callback observes the entering phase's initial sample; render observes this frame's candidate.

`App::is_transitioning()` and `SceneContext::is_transitioning()` are convenience queries.

#### Input eligibility

There is **no implicit raw-input or Node-press filtering** based on alpha. Node paths are still planned against the last drawn roots.

If a new live UI must remain inactive under an old picture, the Scene should submit `RootInput::None` until its observed transition is gone, and grant eligibility through a subsequent normal layout. The runtime never rescues or consumes an old batch, synthesizes a Down, or decides which raw touches count for gameplay.

The [scene-flow example](https://github.com/void-cat-0/mue/blob/main/examples/scene-flow/README.md) demonstrates this explicit visual-readiness policy.

#### Effect interruption

A newer request cancels the current effect at the safe boundary. If the old command has not committed, its pending destination/result is dropped; if it has committed, the stack is **not rolled back**.

Last-request-wins applies before selection. Composite-to-composite interruption can continue from the last displayed blend; arbitrary overlay continuity, reverse playback and automatic shared-element matching are not promised.

Closures and pending requests are released before source owners on cancellation/fault/App drop, including failed commit preparation. There is no unbounded request queue.

#### Host suspension and resize

Host suspension draws no effects, allocates no pictures and advances no phase clock. Animated requests over an entered source wait for a normal rendering frame; instantaneous requests retain the prior suspended-commit/deferred-results behavior.

Resume's first Scene/Node/effect delta is zero. Non-GPU global work runs once per suspended frame when no error stops it.

A normal frame with no drawable area also waits without acknowledging picture draws; it does not suppress Scene input. A new request can cancel a waiting effect without rendering it.

Instantaneous requests and overlays do not require a valid picture size merely to discard an old composite; an unavailable old picture is invalidated, not treated as a Scene error.

#### Effect errors

Effect/capture errors propagate through `try_frame`, fault the App and clear input eligibility exactly like Scene errors. Failed endpoint drawing does not commit the pending switch; a failure after installation does not roll back it.

Recovery is explicit and instant. See [Scene lifecycle and errors][scenes-guide] for the shared ownership and stopped-state contract.

The [scene-transitions example](https://github.com/void-cat-0/mue/blob/main/examples/scene-transitions/README.md) demonstrates built-in fade, a custom slide composite and a two-phase cover. The [song-transition example](https://github.com/void-cat-0/mue/blob/main/examples/song-transition/README.md) adds illustration expansion/shrink, rounded shape/crop interpolation and retained list state using the same overlay API, without Scene screenshots.

Both examples use Scene-local state and separate modules for effects and views.


[shot-crossfade]: ../images/crossfade.gif
[shot-crossfade-end]: ../images/crossfade-end.png

[scenes-guide]: ../learn/scenes.md
[scene-transitions-guide]: ../learn/scenes.md#scene-transitions
[i18n-guide]: ../learn/i18n.md

[guide-reference]: index.md
[layers-reference]: layers-and-overlays.md
