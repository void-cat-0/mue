# Scene transitions

Scene transitions are optional presentation attached to a deferred Scene request. They do not add Scene callbacks, run two Scenes concurrently, start audio, or change the complete raw-input contract. The initial application default is **no animation**, with no picture allocation or shader work.

## Configure one request

`push`, `pop`, `pop_with_result`, `replace` and `exit` return a `SceneRequest` configuration handle. Host-side `push_scene`, `pop_scene`, `pop_scene_with_result`, `replace_scene` and `reset_scene` do the same.

```rust
use mue_macroquad::{Scene, SceneContext, SceneTransition, transition::Transition};

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

`App::set_default_scene_transition(|| SceneTransition::crossfade(Transition::ease(0.25)))` installs a factory for a **fresh** effect at request selection. Only the final selected request invokes it; explicit overrides do not. `clear_default_scene_transition()` restores no animation without cancelling an effect already running. Default-factory captures must own their resources and must not rely on Scene-local borrowed handles.

## Two timings, one scheduler

| Timing | Sequence |
| --- | --- |
| `AfterSwitch(curve)` | Optional outgoing picture capture → normal stack commit → current Scene and effect → complete |
| `AroundSwitch { exit, enter }` | Current Scene and exit effect → successfully presented exit endpoint → next normal safe boundary commits the stack → current Scene and enter effect → complete |

Each non-instant segment first draws at raw progress zero, even if the frame delta is large. Subsequent normal presentation integrates finite positive `dt`; zero, negative and non-finite deltas do not advance it. `TransitionFrame` exposes `phase`, `linear_progress`, eased `progress`, accepted `dt`, capped `elapsed` and `viewport`. Easing uses the existing property `Transition::sample`; the raw fraction determines completion, so spring overshoot cannot switch early. Nonpositive/non-finite durations behave as instant segments, whose one endpoint still must be presented successfully.

A drawing/flush success acknowledges the endpoint, not a prediction during layout. An `AroundSwitch` exit endpoint is therefore visible while the old Scene is still top; commit occurs in the **following normal frame**, after that frame's input/global/host work. A completed entering endpoint remains observable for that frame; the following normal frame retires the effect. These are render-thread submission guarantees, not OS/display scanout fences. Hosts must keep the normal one-App-frame/`next_frame` loop to display frames.

Only the active top receives ordinary Scene/Node updates. During an exit segment it is still active and may continue business; stopping gameplay earlier is an explicit application decision. After its exit endpoint has been presented, the departing Scene skips ordinary update in the commit frame; input/global/host work still precedes that boundary. Submit commands that must take effect on this last input, such as Quit, directly from `SceneEvent::Input` rather than relying on a later `update`. Switching cancels old Node capture and preserves existing Pause/Enter/Returned-before-Resume semantics. New-top Scene update is skipped and its one Node update uses zero delta in the switching frame.

A terminal `pop`/`exit` has no entering Scene. `AroundSwitch` can present its exit, then ends the App without an enter segment; an `AfterSwitch` terminal request is instantaneous. Never-entered sources and fault/exit recovery also commit instantaneously without outgoing drawing, default-factory invocation or manufactured Enter/Pause notifications.

## Picture composite

`SceneTransition::crossfade(curve)` is a built-in RGB mix with opaque output. `SceneTransition::composite(curve, callback)` uses the same `AfterSwitch` scheduling but lets the callback draw the whole composite. The callback receives borrowed `ScenePictures { from, to }`, valid only during that draw, and a `TransitionFrame`.

```rust
use macroquad::prelude::{draw_texture_ex, vec2, DrawTextureParams, WHITE};
use mue_macroquad::{SceneTransition, transition::Transition};

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

GPU resources are lazy: only a selected composite allocates three full-window targets (outgoing, live, presented). App reuses its built-in shaders across requests instead of recompiling each crossfade. Successful source/live/custom-composite draws normalize alpha to one without changing RGB, so ordinary texture copies and interrupted composites do not darken translucent Scene content a second time. No ordinary CPU readback occurs; the output target is never one of the sampled inputs. The old Scene is not retained by the effect after normal removal; the live Scene draws normally each frame.

When no interrupted composite exists, App captures the outgoing **current retained state** once before commit, with layout preparation but **no extra Scene or Node update**. This is not guaranteed to equal the preceding screen: input/global callbacks may already have changed state. `SceneRender::is_transition_source()` identifies this capture, allowing consumers to avoid publishing display bookkeeping from it. For an interrupted composite, the last successfully presented composite becomes the next source without re-rendering the old Scene. Multiple requests selected while suspended retain that same frozen source until one presents.

Resize/DPI changes discard invalid pictures, **not** the phase clock or Scene/input state. If the departed Scene can no longer be captured, `from` and `to` both reference the freshly rendered live picture for the remainder; timing continues. This is a defined fallback, not an old image stretched into a mismatched viewport or a restart.

Current composite restrictions: **OpenGL**, full origin-zero logical window viewport, default-camera host entry, no active raw miniquad pass, balanced model matrices and complete opaque pictures. All Scene/custom/UI rendering must retain App's target camera until returning; do not call `set_default_camera` or start a raw output pass inside a captured Scene. Render-target texture copies require `flip_y: true`. Custom shader/resource setup belongs in the callback, lazily on the render thread. Boundaries flush and restore default camera/pass plus the existing narrow material/mode/scissor/viewport guard, not arbitrary host GPU state. App initializes picture targets with an opaque draw that writes all color channels. App also restores all-channel writes when leaving an effect. Within a callback or ordinary host drawing, submit an all-channel pipeline before `clear_background` if the last submitted draw used a partial mask. Changing the selected QuadGl material alone does not apply its mask. Metal composite requests report an explicit error before allocating targets; overlay drawing has no composite backend restriction.

## Custom overlay without pictures

`overlay(timing, callback)` draws over the normally rendered Scene, using logical window coordinates. It allocates no picture targets and supports either timing. A closure can own an illustration, geometric mask or shader to express shared-asset/cover transitions without capturing full Scene pictures.

```rust
use macroquad::prelude::{draw_rectangle, Color};
use mue_macroquad::{SceneTransition, TransitionPhase, TransitionTiming, transition::Transition};

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

Callbacks only draw or maintain rendering caches, not mutate business/layout/hit-test state or enqueue recursive stack changes. They must own all resources they need through both phases: a copied `NodeRef`, `Signal` or scope handle does **not** keep its source owner alive. Acquire owned texture/asset references rather than reading a disposed outgoing Scene. The callback can capture its own state; no new public effect trait/lifecycle is required.

## Observation, input and interruption

`App::transition_state()`, `SceneContext::transition()` and `SceneRender::transition()` expose the effect sample. Input/update callbacks normally observe the preceding successful sample; a switching lifecycle callback observes the entering phase's initial sample; render observes this frame's candidate. `App::is_transitioning()` and `SceneContext::is_transitioning()` are convenience queries.

There is **no implicit raw-input or Node-press filtering** based on alpha. Node paths are still planned against the last drawn roots. If a new live UI must remain inactive under an old picture, the Scene should submit `RootInput::None` until its observed transition is gone, and grant eligibility through a subsequent normal layout. The runtime never rescues or consumes an old batch, synthesizes a Down, or decides which raw touches count for gameplay. The [scene-flow example](<../examples/scene-flow/README.md>) demonstrates this explicit visual-readiness policy.

A newer request cancels the current effect at the safe boundary. If the old command has not committed, its pending destination/result is dropped; if it has committed, the stack is **not rolled back**. Last-request-wins applies before selection. Composite-to-composite interruption can continue from the last displayed blend; arbitrary overlay continuity, reverse playback and automatic shared-element matching are not promised. Closures and pending requests are released before source owners on cancellation/fault/App drop, including failed commit preparation. There is no unbounded request queue.

Host suspension draws no effects, allocates no pictures and advances no phase clock. Animated requests over an entered source wait for a normal rendering frame; instantaneous requests retain the prior suspended-commit/deferred-results behavior. Resume's first Scene/Node/effect delta is zero. Non-GPU global work runs once per suspended frame when no error stops it. A normal frame with no drawable area also waits without acknowledging picture draws; it does not suppress Scene input. A new request can cancel a waiting effect without rendering it. Instantaneous requests and overlays do not require a valid picture size merely to discard an old composite; an unavailable old picture is invalidated, not treated as a Scene error.

Effect/capture errors propagate through `try_frame`, fault the App and clear input eligibility exactly like Scene errors. Failed endpoint drawing does not commit the pending switch; a failure after installation does not roll back it. Recovery is explicit and instant. See [Scene lifecycle/errors](<scenes.md>) for the shared ownership and stopped-state contract.

The [scene-transitions example](<../examples/scene-transitions/README.md>) demonstrates built-in fade, a custom slide composite and a two-phase cover. The [song-transition example](<../examples/song-transition/README.md>) adds illustration expansion/shrink, rounded shape/crop interpolation and retained list state using the same overlay API, without Scene screenshots. Both examples use Scene-local state and separate modules for effects and views.
