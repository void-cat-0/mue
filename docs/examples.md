# Examples

The workspace examples cover the public UI API and local host integration patterns:

- `style`: declaration syntax, transforms, transitions, and component composition;
- `playground`: interactive layout, typography, graphics, input, application tokens, and style declarations;
- `theme`: theme tokens, reactive button styles, hover, and light/dark switching;
- [`tokens`](https://github.com/void-cat-0/mue/blob/main/examples/tokens/src/main.rs) demonstrates reactive application tokens in a complete app with a single dependency on `mue`.
- `animation`: reactive style transitions;
- `document` and `atlas`: typography and glyph atlas behavior;
- [`scenes`](https://github.com/void-cat-0/mue/blob/main/examples/scenes/README.md): retained Scene stack, push/pop business results, preserved gameplay/UI state, independent host/game pause, multiple Node roots and interleaved drawing;
- [`scene-flow`](https://github.com/void-cat-0/mue/blob/main/examples/scene-flow/README.md): cancellable/fallible CPU loading, normal-frame GPU handoff, explicit start, completion/retry/return, and public GPU-only Scene-picture crossfade;
- [`scene-transitions`](https://github.com/void-cat-0/mue/blob/main/examples/scene-transitions/README.md): ordinary interactive use of the public [Scene transition API][scene-transitions-guide], built-in fade, custom slide composite and two-phase overlay cover;
- [`song-transition`](https://github.com/void-cat-0/mue/blob/main/examples/song-transition/README.md): song-card artwork expansion/return, retained selection/scroll, and an application-owned settings Scene; explicit read-only signals, reusable artwork ownership, Dark/Light/reduced-motion preferences and English/Simplified Chinese [Fluent localization][i18n-guide] without a framework global container;
- `portal`: a concrete external renderer, texture composition, frame-start chart input, and a modal pause UI; its input and coordinate helpers are local to the example;
- [`input-timing`](https://github.com/void-cat-0/mue/blob/main/examples/input-timing/README.md): a 1–2 FPS visual comparison of pointer hit-testing against the last displayed Node tree versus the next layout; run with `cargo run -p input-timing --offline -- --fps 1`;
- `nodes`, `basics`, `color`, `gestures`, `pages`, and `tab`: layout, input, graphics, and same-level page selection.

Run one with `cargo run -p style-example`, `cargo run -p theme`, or `cargo run -p portal`. The theme example uses the optional token store (`Theme`, `TokenSet`, `token_set!`, `token!`, and `provide_theme`); [application tokens and context][themes-guide] explains the store and plain `Prop` / `map` / `provide` bindings.

In [the portal host](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/lib.rs), the host owns a concrete `MockChannel` and calls its resize/render/texture operations directly. [input.rs](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/input.rs) contains the local `ChartInput`, event collection/replay, pointer ownership, Hold-on-resume, and cancel behavior; [target.rs](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/target.rs) maps a logical rect to the actual target pixel size. DPI and resize policy belong to the host. The public `GameController`/`GameChannel` layer and related root exports have been removed; Portal, RenderStateGuard, and SharedTexture/SafeTexture remain in their existing modules. Here "channel" can still describe the external renderer.

Chart events replay at frame start; App preplans the complete UI pointer/hover/wheel batch against the displayed tree and dispatches it before transitions/tasks/layout/render. The portal host keeps mode and pause-overlay visibility fixed until both subscribers finish input, then publishes snapshot/mode in `app.frame_with_update(...)`, before layout/render. The new overlay is drawn that frame but cannot consume the earlier batch's Down. Mouse Move uses the local state during replay; it does not query the final global mouse-button state for the entire batch. Retry explicitly cancels the old attempt's pointers; target resize also has an explicit cancel boundary. Input routes through the map for the last displayed texture, with the initial target resized before the first batch.

The portal collector and UI PointerManager use [mue input cursors][input-api], sharing one legacy macroquad input source: `InputSubscriber::new()`, then `read()`, handle `InputBatch::is_interrupted()`, and `repeat(&mut handler)`. No consumer queue is allocated; unread events expire each frame, even with no App. Use cursors in the main application future and synchronous callbacks, before `next_frame`, rather than automatic macroquad coroutines. On a missed frame portal clears its buffered/held state and cancels chart pointers; normal game pause still retains owner/Hold semantics. `App::frame_paused()` discards UI input and cancels capture/gestures before ticking tasks. The [Scene runtime][scenes-guide] documents App scheduling; the portal example demonstrates an external renderer without Scene management.

[scene-transitions-guide]: scene-transitions.md
[i18n-guide]: i18n.md
[themes-guide]: themes.md
[scenes-guide]: scenes.md
[input-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/input.rs
