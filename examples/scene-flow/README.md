# Scene flow

Run from the workspace root:

```bash
cargo run -p scene-flow --offline
```

An interactive **offline consumer** of the public Scene APIs: retained details → cancellable/fallible CPU loading → normal-update GPU upload → Ready → explicit Start → Running → Results → retry or original details. It is not a Phira chart parser, audio engine, judgement system or device-timing test.

## Project layout

- [Entry point](<src/main.rs>): bundled font initialization, window configuration and the ordinary App loop.
- [Scene module](<src/scene/mod.rs>): the public crossfade requested for Details/PlayFlow topology changes.
- [Details](<src/scene/details.rs>): retained selection/filter, its own command slot and the returned-result summary.
- [PlayFlow](<src/scene/play_flow/mod.rs>): one Scene with local Loading, Failure, Ready, Running and Results states; it owns the request, session and texture.
- [Views](<src/ui.rs>): declarative Node trees and result text. Each button captures only its Scene's `Rc<Cell<Action>>`; its signals belong to the Scene's `OwnedScope`, kept alive through Node cleanup.
- [Keyboard collectors](<src/input.rs>): each Scene owns its own edge-triggered collector. Raw keyboard handling is independent of automatic Node UI dispatch; no global input/action state is shared between Scenes.
- [Resource source](<src/assets.rs>) and [CPU/session model](<src/flow.rs>): the offline asset and UI-independent loading/attempt logic.

## Ownership and normal behavior

Details pushes PlayFlow without reconstructing itself. Its selected entry, filter, summary and original Node tree remain owned by the covered Scene. `pop_with_result(FlowResult)` delivers an independent business value to Details' `Returned` event before Resume; that event updates the summary directly. Popping the root ends the App and its host loop.

The bundled resource encodes a 32×32 RGBA image, a three-second duration and six timed trajectory points. One `mue_core::spawn_blocking` worker genuinely validates/decodes the source and computes its hash. A caller-owned foreground Task awaits the owned CPU result. No GPU object, reactive handle, Rc, Scene or window enters the worker.

Loading consumes `Request::update` only in a normal Scene update, after its Loading picture has rendered and `SceneContext::is_transitioning()` is false. Only that normal update uploads RGBA to a real `Texture2D` and creates a non-running Session. CPU completion alone never accepts the resource, uploads it or starts an attempt. There is no artificial loading delay; this small source may finish while the Details/PlayFlow crossfade is still active.

Cancel invalidates the request and drops its foreground publisher before returning. The parser checks a cooperative cancellation token; dropping a blocking handle does not forcibly stop its OS thread. Late work cannot revive a cancelled Scene. Malformed data and the post-decode business-failure option use ordinary failure values, not App faults. Failure retry decodes a fresh request generation.

Ready requires an explicit Start. Starting does not advance the timeline in that frame: Running must first render its own picture, and the Session's first start/resume update is zero-delta. Natural completion clamps the timeline and stops once. Stop attempt produces Results early; Escape returns directly with the exited result.

Results keep the same Rc Session, Rc Prepared and GPU texture. Retry explicitly starts another attempt without decoding or uploading again. The returned result owns only business data and remains independent of those resources after PlayFlow is dropped.

User pause and Scene lifecycle pause remain distinct. `Pause` stops the session immediately. `Resume` makes it eligible to run; the session starts running again only in its next ordinary, presentation-eligible update and only if the user has not paused it. An outgoing Scene can therefore receive `Resume` and input without restarting its session. The first start/resume update still discards its elapsed delta. This example models the timing; it does not play audio.

## Presentation and Scene transitions

Only Details/PlayFlow stack changes request an animation:

```rust
use mue_macroquad::{SceneTransition, transition::Transition};

ctx.push(play_flow)
    .transition(SceneTransition::crossfade(Transition::linear(0.25)));
ctx.pop_with_result(outcome)
    .transition(SceneTransition::crossfade(Transition::linear(0.25)));
```

Loading → Ready → Running → Results and retry are local states of the same Scene, not extra Scenes and not automatic Scene fades. App owns picture capture, composition, cameras and animation time; the consumer draws an opaque background, its texture/trajectory and `render.draw(root)` in App's current target.

Presentation gating is local: a live render records PlayFlow's `last_rendered_phase` (or Details' own drawn-picture flag). Subsequent input/update observes `SceneContext::is_transitioning()` and enables only the already-rendered local state. A phase change clears that evidence. Layout submits `RootInput::None` until that condition holds; pointer eligibility changes through the next ordinary layout submission, not by changing the raw input batch.

Rendering never consumes loading results, starts a session or advances business time. App may render a departing Scene once to capture its source; `SceneRender::is_transition_source()` prevents that pass from publishing readiness. No host-side shared App-status bridge, custom presenter, pixel-readback check or alternate animation clock is involved.

Escape remains available during a crossfade, including to cancel loading. Its result pop requests another public crossfade; App handles interruption using the last presented composite rather than retaining a second running Scene.

## Controls

- **Details:** Load resource, Load with a business failure, change selected entry/filter, or Exit root Scene. Space loads; Escape exits.
- **Loading:** Cancel loading / Back. Escape can cancel even during the crossfade.
- **Failure:** Retry loading successfully (R), or return to original details.
- **Ready:** Start prepared session (Space), or return without starting.
- **Running:** Pause/resume (P), Stop attempt to show Results, or Escape to exit and return immediately.
- **Results:** Retry with the same resources (R), or return to original details.

## Integration limits

Use the interactive controls above to exercise loading failure/retry, cancellation, explicit start, pause/resume, stop and resource reuse. `LoadMonitor` provides progress/cancellation observation; it does not own the session or GPU texture.

The existing bundled [font](<../playground/assets/fonts/NotoSans-Regular.ttf>) is initialized before Nodes, with its [license](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>); no download or system-font installation is required. Real source formats, audio/music-clock judgement and platform touch validation remain separate integration work.
