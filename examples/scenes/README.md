# Scenes

Run from the workspace root:

```
cargo run -p scenes --offline
```

This example uses the Scene stack API. `App` owns a stack and the frame schedule; only the top Scene receives ordinary `event(Input)`, `update`, `layout`, and `render` callbacks. `GameScene` owns gameplay state, two independent Node trees, and an `OwnedScope` containing their signals. Pushing another Scene preserves that exact game owner, its elapsed time, Node instances, and pause signal until it is exposed again. Pop, top-only replacement, reset, or full-stack exit releases the removed owners. All Scene callbacks return `anyhow::Result<()>` and frame errors propagate through `app.try_frame()?`.

Input and lifecycle notifications share a single `Scene::event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()>` entry point, which ignores notifications by default. The root-exported `SceneEvent<'a>` has exactly `Enter`, `Pause`, `Resume`, `Input(&'a InputBatch)`, and `Returned(SceneResult)` variants. Both demo Scenes match every variant in this method; `ResultScene` explicitly ignores `Returned`. `update`, `layout`, and `render` remain separate. Only the public notification API shape is simplified; callback timing, input contents, and stack behavior are unchanged.

Each gameplay frame follows these API stages:

1. App automatically dispatches Node UI first, then `event(Input(input))` replays its complete, unfiltered borrowed `InputBatch` into the miniquad `EventHandler` named `RawInputObserver`. The observer uses keyboard commands and records the most recent left-mouse or touch Down, including clicks on HUD/modal UI; this is rawinput observation, not gameplay hit-testing. Event order, physical window-pixel coordinates and original touch `time` are preserved, never replaced with logical coordinates or host frame time. Mouse Down has no timestamp. An interrupted batch clears local input state before replay. The full batch also includes wheel and unmatched Move/Up; this simple observer intentionally ignores them.
2. `update` applies UI commands and advances gameplay time using `ctx.frame.dt` only while gameplay is running. Buttons and the P key change the same in-game pause signal. That signal is independent of the lifecycle state used when the Scene is covered or the host is suspended.
3. `layout` only submits the roots and their logical window-space bounds. The HUD starts at (24, 24). The full-viewport pause root is submitted only while paused and explicitly uses `RootInput::Modal`, so blank parts of the modal block new input to lower Node trees, not Scene rawinput. App may call layout twice in a frame, so it must not advance timers or perform drawing.
4. `render` performs gameplay/custom background drawing, `render.draw(hud)`, custom foreground drawing, then `render.draw(modal)` when paused. The orange band overlaps the bottom of the HUD to make the order visible. The modal is both omitted from layout and not drawn when hidden. Submitted Node trees receive updates once per frame; the HUD continues to update during in-game pause.

App preplans Node input against the roots drawn in the last displayed frame; existing UI sequences retain their Node capture until Up/Cancel. `RootInput::None/Interactive/Modal` and `InputPlan`/`PointerManager` concern only Node routing, capture and gestures. None filters the Scene raw batch: UI Down, wheel and modal events remain visible. The earlier UI/Scene press-owner filter has been withdrawn.

Gameplay policy belongs to the Scene. This demo's pointer marker is labeled `Last raw Down (incl. UI/modal)` and never judges a gameplay hit. Within the raw observer, only keyboard commands affect gameplay; UI buttons still issue their existing pause/results/exit commands. A real game must implement its own modal, pause and contact policy, including unmatched Move/Up. App currently dispatches UI automatically before Scene rawinput and exposes no per-event consume API in this iteration. There is no synthetic Down or new contact-snapshot API; this example does not implement a held-pointer mechanic or Hold-on-resume.

## Stack and business results

The `Results` / `Show results` buttons call `ctx.push(ResultScene::new(...))`, rather than replace the game. The pure UI result Scene offers two explicit business decisions:

- `Return to game` calls `ctx.pop_with_result(GameResult::Return)`. The original GameScene receives the result through `event(Returned(result))`, downcasts the root-exported `SceneResult = Box<dyn Any>` to the local `GameResult` enum, and preserves elapsed time and the user's in-game pause. Returning from results never implicitly unpauses gameplay.
- `Retry` calls `ctx.pop_with_result(GameResult::Retry)`. GameScene itself defines retry as resetting `elapsed` to zero, clearing the business pause, and starting its demo timer again. The stack runtime does not interpret the enum, reset gameplay, or construct a new GameScene. Its HUD, modal, and signal scope remain the same instances.

Each Scene receives `event(Enter)` once. Push sends `event(Pause)` to the old running top once; pop sends `event(Pause)` to the outgoing running top and delivers `event(Returned(result))` to the exposed Scene **before** `event(Resume)`, without repeating `event(Enter)`. Only the top is scheduled while the game is covered. The transition frame does not replay the outgoing input batch as `event(Input)` or run the new top's ordinary `Scene::update`; it still lays out, updates submitted Nodes exactly once with `dt = 0`, and draws that top. Ordinary Scene updates begin on the following frame. Later raw batches reach the active top in full, including continuing Move/Up whose Down it never saw.

The host creates `App::with_scene(scene)` and calls `app.try_frame()?` while `!app.is_exited()`. `app.scene_count()` reports stack depth. `ctx.replace(scene)` replaces only the top, `ctx.pop()` removes only the top, and `ctx.exit()` clears the whole stack. Popping the root sets the empty App to Exited; the host must break instead of continuing its window loop. Host-level `push_scene`, `pop_scene`, `pop_scene_with_result`, `replace_scene`, and `reset_scene` queue the corresponding operations; reset installs a new root rather than replacing only the top.

In-game pause is a Scene signal. `event(Pause)` / `event(Resume)` mean losing/regaining runtime eligibility, whether from coverage or host suspension through `App::try_frame_paused()`. Coverage and host suspension compose: suspending while GameScene is already covered does not send it `event(Pause)` twice, and resuming the host sends `event(Resume)` only to the top result Scene, not the game underneath. These notifications never clear the user's game pause. The first normal frame after host restoration uses `dt = 0` for both Scene and Node updates. The demo's ordinary loop stays live during in-game pause so the menu can update; only the finite check explicitly suspends the host. Paused/faulted frames still drain old input and cancel UI state as lifecycle cleanup, not UI/Scene ownership filtering.

Controls:

- Click `Pause / resume` in the HUD or press P to toggle the pause layer.
- Click `Resume` on the pause layer to resume gameplay.
- Click `Results` / `Show results` to push the pure UI result Scene, then `Return to game` to preserve the game or `Retry` to restart its demo timer.
- Click `Exit` or press Esc in either Scene to exit the entire stack.
- While the game Scene is active, left-click or touch anywhere, including HUD/modal UI, to update the last raw Down position and optional touch timestamp. The marker is custom background drawing beneath the UI, not a gameplay judgment.

The UI initializes mue fonts in main from the existing bundled Noto Sans Regular font before creating nodes. Custom drawing uses macroquad's built-in default font. No system font installation or runtime font downloads are required. Noto's existing notice is in [OFL-NotoLatinGreekCyrillic.txt](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>).

## Finite check

```
cargo run -p scenes --offline -- --check
```

This bounded run stops after **16 host frames: 14 normal frames plus 2 paused frames**. It writes the same pause and action signals directly during Scene update; it does not inject device input. Both round trips execute real `ctx.push` / `ctx.pop_with_result` commands, followed by a real root `ctx.pop`.

| Host frames | Checked operation |
| --- | --- |
| 1–5 | Run, pause for two frames, then resume; HUD=5/modal=2 Node updates. |
| 6 | Pause gameplay and push results; retain the original GameScene below the top. |
| 7–8 | Call `app.try_frame_paused()` twice while the game is already covered. |
| 9–11 | Restore only the result Scene, pop `Return`, then run the same still-user-paused game; HUD=7/modal=4. |
| 12–15 | Push results again, pop `Retry`, and let GameScene apply its own reset policy; HUD=9/modal=4. |
| 16 | Pop the root; verify `scene_count() == 0` and `is_exited()`, then stop the host loop. |

The finite check asserts:

- Covered GameScene receives no ordinary `event(Input)` / `update` and its Node counters do not advance. The same HUD/modal Node references, gameplay elapsed time, frame counter, and in-game pause signal survive both pushes until result delivery.
- `Return` preserves elapsed and user pause through `event(Resume)` and the next gameplay frame. `Retry` is handled by GameScene in `event(Returned(result))` before `event(Resume)` and resets only its chosen gameplay state.
- Exactly one GameScene `event(Enter)` and two result Scene `event(Enter)` notifications occur; exposing the game never repeats `event(Enter)`. Lifecycle order is checked as notifications occur, including `event(Returned)` before `event(Resume)`, no duplicate `event(Pause)` during repeated host suspension, and no early `event(Resume)` of the covered game.
- Transition frames skip the new top's ordinary `event(Input)` / `update`, but update its Nodes once with `dt = 0`; the host-restored Scene and Nodes also use zero delta. The final totals are **HUD=9, modal=4, result roots=4** despite repeated layout preparation.
- Actual game root render hooks occur in background → HUD → custom foreground → modal order. GPU pixel readback after the final draw retains the HUD and orange foreground while running and verifies that the modal covers both while paused, including after returning to the old game.
- The root pop is the operation that empties the App; no further host frame runs after Exited.

On success the check prints exactly three bounded `check:` result lines. It needs a graphics display, as normal macroquad rendering does. `cargo check -p scenes --offline` and `cargo test -p scenes --offline` are compilation checks for this example; there are no standalone unit tests in this binary. The finite run provides signal, stack lifecycle, render-phase, and GPU coverage, **not native device-input or routing verification**. Manually exercise the controls above to explore that behavior; do not treat the direct signal script as device-input evidence.

Scene exit animations and actual Phira loading, audio/chart playback, scoring, and resource handoff are intentionally not implemented here. This demonstrates an ownership-preserving Scene round trip, not a verified complete Phira integration.
