# Scenes

Run from the workspace root with a graphics display:

```sh
cargo run -p scenes --offline
```

This is an interactive example of the Scene stack API, with no scripted or capture mode. `App` owns scheduling and the stack; each Scene owns its own Node trees, detached `OwnedScope`, local UI command handle, and business state. A result Scene receives only an elapsed-time snapshot, never a shared handle to the game. All Scene callbacks return `anyhow::Result<()>`, and frame errors propagate through `app.try_frame()?`.

## Source layout

- [main.rs](<src/main.rs>): window configuration, bundled font initialization, and the `App` loop.
- [scene/game.rs](<src/scene/game.rs>): gameplay ownership, lifecycle notifications, input, stack commands, and custom drawing.
- [scene/results.rs](<src/scene/results.rs>): the independent result Scene and its typed pop decisions.
- [scene/mod.rs](<src/scene/mod.rs>): the Scene module boundary.
- [model.rs](<src/model.rs>): pure timer/pause state and the business meaning of `Return` and `Retry`.
- [input.rs](<src/input.rs>): a local miniquad `EventHandler` collector for the complete raw input batch.
- [ui.rs](<src/ui.rs>): HUD, pause modal, and result Node construction. Callbacks only enqueue actions in a small `Rc<Cell<GameAction>>` or `Rc<Cell<ResultsAction>>` belonging to that Scene.

UI callbacks do not mutate gameplay or a parent Scene through a shared object. The owning Scene applies its local commands; `ResultsScene` returns a value via the stack API, and the existing `GameScene` handles that value. Node owners drop before their detached UI scope. There is no asynchronous loading task in this example.

## Frame and input contract

1. **UI, then raw input.** App automatically dispatches Node UI before `event(Input(input))`. The Scene replays the complete, unfiltered borrowed `InputBatch` through `RawInputObserver`, including input on HUD and modal UI. Original event order, physical window-pixel coordinates, and touch `time` are preserved; they are never replaced with logical coordinates or host frame time. Mouse Down has no timestamp. An interrupted batch resets local raw state before replay, without clearing commands that UI just produced from the same batch. Wheel and unmatched Move/Up are also replayed; this simple observer intentionally ignores them.
2. **Business update.** The owning Scene applies local UI pause commands before raw keyboard commands, preserving UI-first dispatch order; `update` applies the remaining commands and advances gameplay using `ctx.frame.dt` only while gameplay is running. Buttons and P control the same user pause state. Runtime eligibility due to coverage or host suspension is separate from that user choice.
3. **Pure layout.** `layout` only submits roots and their logical window-space bounds. The HUD starts at (24, 24). The full-viewport pause root is submitted only while user-paused and uses `RootInput::Modal`: even blank parts block new input to lower Node trees, but not Scene raw input. App can prepare layout twice in a frame, so layout must not advance timers or draw.
4. **Interleaved drawing.** `render` draws custom gameplay/background, the HUD, custom foreground, and finally the visible pause modal. The orange band overlaps the bottom of the HUD to show the ordering. A hidden modal is neither submitted nor drawn. Submitted Nodes update once per frame, and the HUD remains active during user pause.

App preplans Node input against roots drawn in the last displayed frame. Existing UI sequences retain Node capture until Up/Cancel. `RootInput::None/Interactive/Modal`, `InputPlan`, and `PointerManager` only concern Node routing, capture, and gestures; none filters the raw Scene batch. UI Down, wheel, modal input, and continuing Move/Up whose Down a newly active Scene never saw all remain visible. App exposes no per-event consume API in this example.

The label `Last raw Down (incl. UI/modal)` is observation, not gameplay hit-testing. Within the raw observer, only keyboard commands affect gameplay; pointer Down records the last left-mouse/touch position and optional touch timestamp, even on UI. A real game must implement its own modal, pause, contact, and unmatched Move/Up policy. This example has no synthetic Down, contact snapshot, held-pointer mechanic, or Hold-on-resume behavior.

## Stack and business results

`Results` / `Show results` calls `ctx.push(ResultsScene::new(elapsed))`, preserving the exact `GameScene`, timer, user pause, HUD/modal Nodes, and scope underneath. Only the top Scene receives ordinary input, update, layout, and rendering. The result Scene offers two business decisions:

- **Return to game** calls `ctx.pop_with_result(GameResult::Return)`. GameScene downcasts the root-exported `SceneResult = Box<dyn Any>` in `event(Returned(result))` and preserves elapsed time and user pause. Returning never implicitly resumes paused gameplay.
- **Retry** calls `ctx.pop_with_result(GameResult::Retry)`. GameScene defines retry as clearing elapsed time and user pause and starting the timer again. This does not construct a new game or replace its HUD, modal, or scope. App transports the enum; it does not interpret or reset business state.

App sends `event(Enter)` once to each Scene instance. Pushing pauses the old running top once. Popping pauses the outgoing running top and delivers `event(Returned(result))` to the exposed Scene **before** `event(Resume)`, without repeating Enter. On a transition frame, the new top does not receive the outgoing raw batch or an ordinary Scene update; its submitted Nodes still update once with `dt = 0`, and it is laid out and drawn. Ordinary Scene updates begin on the following frame.

`Pause` / `Resume` notifications mean losing/regaining runtime eligibility; they never clear the game's user pause. Host suspension via `App::try_frame_paused()` composes with coverage: a covered game is not paused twice, and host restoration resumes only the top result Scene, not the game underneath. The first normal frame after host restoration uses zero delta. The example's ordinary host loop remains live during user pause so the menu can respond; it does not synthesize host suspension. Paused/faulted frames drain old input and cancel UI state as lifecycle cleanup, not raw ownership filtering.

The host uses `App::with_scene(GameScene::new())` and calls `app.try_frame()?` while `!app.is_exited()`. `Pop game` demonstrates `ctx.pop()`: popping the root leaves an empty, Exited App and the host loop stops. `ctx.exit()` instead clears the entire stack, including a covered game when invoked from results. `ctx.replace(scene)` replaces only the top; host-side `reset_scene` installs a new root.

## Controls

- Click **Pause / resume** in the HUD or press **P** to toggle gameplay pause.
- Click **Resume** on the pause layer to resume gameplay.
- Click **Results** / **Show results** to push the result Scene. **Return to game** preserves the game and pause choice; **Retry** restarts the same game's timer and resumes gameplay.
- Click **Pop game** in the HUD to pop the root and stop the host loop.
- Click **Exit** or press **Esc** in either Scene to exit the entire stack.
- While GameScene is active, left-click or touch anywhere, including HUD/modal UI, to update the last raw Down position and optional touch timestamp. This custom background marker is not a gameplay judgment.

The UI uses the existing bundled Noto Sans Regular font, initialized before creating Nodes. Custom drawing uses macroquad's default font. No system font installation or runtime downloads are required. The font notice is [OFL-NotoLatinGreekCyrillic.txt](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>).

## Integration limits

Use the interactive controls above to exercise timer/pause behavior, Return preserving elapsed time and the user's pause choice, and Retry resetting the timer and clearing pause.

Actual Phira loading, audio/chart playback, scoring, resource handoff, and Scene exit animations are intentionally not implemented. This is an ownership-preserving Scene round trip, not a complete Phira integration.
