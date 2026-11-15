# Scene flow

Run from the workspace root:

```bash
cargo run -p scene-flow --offline
cargo run -p scene-flow --offline -- --check
```

This is a runnable **offline consumer** of the existing Scene API. It exercises retained details → cancellable/fallible CPU loading → GPU resource handoff → explicitly started session → completion/exit → original details or retry. It is not a Phira chart parser, audio engine, judgement system or native-device timing test.

## Ownership and phases

`Details` is the retained root Scene. It holds a selected entry, filter, summary and the original Node owner. It pushes a `PlayFlow` Scene whose internal states are Loading, Failure, Ready, Running and Results. Those states do not each require a separate Scene. Returning uses `pop_with_result(FlowResult)` and the root's `event(Returned)` applies the outcome before Resume; the selection, filter and original Node survive. Root pop ends the App and the host loop.

[The resource source](<src/assets.rs>) encodes a 32×32 RGBA image, a three-second duration and six timed trajectory points. [The loader](<src/flow.rs>) actually validates/decodes those bytes on one `mue_core::spawn_blocking` OS thread and computes a source hash. A caller-owned foreground Task awaits the worker; background code touches no GPU, reactive handles or Rc. `Request::update` consumes the owned CPU result once, in the normal Scene update after Loading has fully presented; CPU completion itself may happen earlier. The interactive view keeps loading visible briefly, without sleeping or blocking its worker.

Cancel invalidates the generation and drops the foreground publisher before pop. The parser has cooperative cancellation checks, but dropping a blocking handle does not forcibly stop its OS thread. Late results are discarded and cannot reactivate the cancelled Scene. Loading failure is a business value with a message, not `App::is_faulted()`. Retry after failure starts a fresh generation. Actual parsing errors and the failure-demo option share the same UI/return path.

Ready uploads the decoded RGBA into a real Texture2D **only in a normal Scene update** and creates a Session that is not running. No GPU work or session start occurs in the worker, an effect, or `frame_paused`. The Start command starts the session explicitly; time/trajectory updates wait for its running picture to be fully presented. Preparation, permission to show, and starting gameplay are separate conditions.

Natural completion clamps the timeline and stops the attempt once; Stop attempt stops it explicitly. Results retain the same Rc Session, Rc Prepared and GPU texture for a retry, without redecoding/reuploading. Returning drops the child resources after producing an independent business result. The shared monitor/weak references do not own those resources. `event(Pause/Resume)` controls lifecycle eligibility separately from user pause; host restoration never clears a user pause and the first resumed update has zero delta.

## Controls

- Details: Load resource, Load with a business failure, change selected entry/filter, or Exit root Scene.
- Loading: Cancel loading / Back. Esc can cancel immediately even during a fade.
- Failure: Retry loading successfully, or Return to original details.
- Ready: Start prepared session (Space), or return without starting.
- Running: Pause/resume (P), Stop attempt, or Esc to return immediately with an exited result.
- Results: Retry with the same resources (R), or Return to original details.

The Scene sees the complete raw InputBatch; pointer Downs are observations, not gameplay judgement. Node UI dispatch remains automatic. The example implements its own presentation eligibility for UI/buttons/keyboard actions rather than changing the framework's raw-input contract.

## Switching animation

[Presenter](<src/presentation.rs>) is **example-local**, not a new App/Scene callback or render-graph API. It crossfades complete, opaque Scene pictures in three GPU targets: live, previously presented composite, and frozen previous composite. Changing the Scene/internal phase visual key freezes the last fully displayed picture. If interrupted mid-fade, that last composite becomes the source for the next transition, avoiding a jump back to the original Scene. Only pixels are retained; old Scenes, tasks, audio and sessions are not kept running by the animation.

The first presentation of a change has progress zero; later normal presentations integrate dt. During host suspension the Presenter is not called; first restoration uses dt zero. Resize/DPI rebuild only the picture buffers and settle the obsolete fade, without resetting the Scene/session or touching pointer capture. The three targets never read from the one being written. Their blit preserves opaque alpha and flips Y correctly for target-to-target and target-to-screen copies.

Roots use `RootInput::None` until their visual key has finished presenting. Existing Node hit planning is still based on the previously drawn root list, so qualification takes effect through normal layout submission, not by rerouting a current batch. A new live UI cannot receive new Node presses while hidden under an old picture. Raw Scene input is not filtered; game/modal/contact policy is still Scene-owned.

The implementation requires **OpenGL**, the full origin-zero logical window viewport, a default-camera host entry, no active raw miniquad pass and balanced model matrices. Scene/UI draws remain in the live target camera until `finish_draw`; host composition restores the default camera. It deliberately does not promise arbitrary camera/pass capture or transparent overlay composition. The main framework's existing pass contract is unchanged. GPU-only caches have a three-full-window-texture cost; no screenshot readback is used for ordinary animation.

## Finite validation

`--check` uses the real Scene runtime, worker, resource upload, custom drawing and GPU composition. It supplies finite actions through the same command slot used by controls; it does not inject physical device input. It checks:

1. First request is cancelled during a nonzero picture fade, before acceptance, and returns once. The cancelled monitor eventually finishes but never becomes accepted.
2. A CPU/business failure returns to retained details; a second failure retries to a fresh, successful generation.
3. Host pauses during loading until actual CPU completion: no acceptance, GPU upload or session start while suspended. Normal restoration hands resources over, but Ready still has no running session.
4. Start is explicit; local game pause survives host pause/resume without accumulated dt; natural completion reaches all six decoded points and stops once.
5. Retry reuses the same CPU/GPU/session allocation and starts a second attempt. Explicit exit stops it once and returns to original details; independent results survive resource disposal.
6. Exactly one details Enter, one Returned/Resume per round trip, original selection/filter/Node preservation, child lifecycle counts and zero live resource weak references after return.
7. Crossfade RGBA pixels, target orientation, nonzero-fade interruption source continuity, no early presentation readiness, frozen animation during suspension and real window resize. App's public hit-test query also checks that Ready roots do not acquire Node input while faded out and become eligible through normal subsequent layout; this is not native event dispatch.
8. Final root pop leaves stack zero/Exited and ends the host loop.

On success it prints three bounded `check:` result lines and exits within a 15-second deadline. GPU pixel readback is used only by this finite check, not by normal presentation. Counts of ordinary host frames vary with the display/worker; the animation check delta is fixed without changing App/session dt.

[Model tests](<src/flow/tests.rs>) use channels to hold **real decoded data** at the worker handoff boundary, demonstrating cancellation/replacement after decode and late resource destruction without sleep-based races. They also cover invalid source data, caller-owned tasks, ready/paused/stopped time, one-time stop and retry. Presenter has six headless clock/size tests. The bundled existing [font asset](<../playground/assets/fonts/NotoSans-Regular.ttf>) is initialized before Nodes, with its [license](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>); no downloads or system-font installation are required.

Remaining integration work: real Phira source formats, audio/music-clock judgement, platform touch validation and an application decision on whether this proven host-local presentation should become a reusable public API. This example does not add asynchronous Scene lifecycle callbacks or a general multi-Scene render stack.
