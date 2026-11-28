# Examples

The standalone examples package is excluded from the root workspace and its Rustdoc; its targets cover the public UI API and local host integration patterns. Run them from the repository root with `cargo run --manifest-path examples/Cargo.toml --example NAME`:

- `style`: declaration syntax, transforms, transitions, and component composition;
- `playground`: interactive layout, typography, graphics, input, application tokens, and style declarations;
- `theme`: theme tokens, reactive button styles, hover, and light/dark switching;
- [`tokens`](https://github.com/void-cat-0/mue/blob/main/examples/tokens/src/main.rs) demonstrates reactive application tokens in a complete app with a single dependency on `mue`.
- `animation`: reactive style transitions;
- `document` and `atlas`: typography and glyph atlas behavior;
- [`scenes`](https://github.com/void-cat-0/mue/blob/main/examples/scenes/README.md): retained Scene stack, push/pop business results, preserved gameplay/UI state, independent host/game pause, multiple Node roots and interleaved drawing;
- [`scene-flow`](https://github.com/void-cat-0/mue/blob/main/examples/scene-flow/README.md): cancellable/fallible CPU loading, normal-frame GPU handoff, explicit start, completion/retry/return, and public GPU-only Scene-picture crossfade;
- [`scene-transitions`](https://github.com/void-cat-0/mue/blob/main/examples/scene-transitions/README.md): ordinary interactive use of the public [Scene transition API][scene-transitions-guide], built-in fade, custom slide composite and two-phase overlay cover;
- [`song-transition`](https://github.com/void-cat-0/mue/blob/main/examples/song-transition/README.md): song-card artwork expansion/return, retained selection/scroll, an application-owned settings Scene and cross-Scene Toasts and dialogs; explicit read-only signals, reusable artwork ownership, Dark/Light/reduced-motion preferences and English/Simplified Chinese [Fluent localization][i18n-guide] without a framework global container;
- [`phira-autoplay`](../../examples/phira-autoplay/README.md): local Phira PEZ library, artwork transition, real audio-clock autoplay, pause/retry and results returned to the retained details; enable the `phira-autoplay` feature and put `ManaBlast.pez` and `ExoplanetaryMirage.かめりあ-AT.pez` in `examples/phira-autoplay/charts/`, or pass local PEZ paths;
- `portal`: a concrete external renderer, texture composition, frame-start chart input, and a modal pause UI; its input and coordinate helpers are local to the example;
- [`input-timing`](https://github.com/void-cat-0/mue/blob/main/examples/input-timing/README.md): a 1–2 FPS visual comparison of pointer hit-testing against the last displayed Node tree versus the next layout; run with `cargo run --manifest-path examples/Cargo.toml --example input-timing --offline -- --fps 1`;
- `nodes`, `basics`, `color`, `gestures`, `pages`, and `tab`: layout, input, graphics, and same-level page selection.
- `vulkan-shaders`: offscreen pixel checks for fixed SPIR-V and runtime GLSL, retained pass ordering, texture transfers, MSAA resolve, Scene crossfade, and Vulkan shadow-mask eviction and reuse. The example presents a window but never reads back the window itself. On desktop Linux, append `-- --wayland` for native Wayland or `-- --x11-with-wayland-fallback` to prefer X11. Regenerate/check the SPIR-V with `cargo run --locked --manifest-path examples/tools/Cargo.toml --features vulkan --bin precompile-vulkan-shaders [-- --check]`.
- `vulkan-startup`: Linux-only check that the selected renderer opens a window and presents its first frame. Pass `-- --expect-vulkan` or `-- --expect-opengl`.

Run one with `cargo run --manifest-path examples/Cargo.toml --example style-example`, `cargo run --manifest-path examples/Cargo.toml --example theme`, or `cargo run --manifest-path examples/Cargo.toml --example portal`. The theme example uses the optional token store (`Theme`, `TokenSet`, `token_set!`, `token!`, and `provide_theme`); [application tokens and context][themes-guide] explains the store and plain `Prop` / `map` / `provide` bindings.

## Vulkan development checkout

This branch builds against sibling `miniquad/` and `macroquad/` checkouts. Use the two full commit SHAs published with the Mue revision you are testing:

```sh
MINIQUAD_REV=<full-mini-commit> MACROQUAD_REV=<full-macro-commit> ./scripts/checkout-graphics.sh
```

The script leaves existing checkouts alone; they must already be clean and at those exact commits. The [integration workflow](../../.github/workflows/vulkan.yml) uses `MINIQUAD_VULKAN_REV` and `MACROQUAD_VULKAN_REV` repository variables, or explicit revisions on manual runs. Both revisions must be reachable from the fork repositories before enabling the workflow.

On Windows and desktop Linux, `playground`, `atlas`, and `scene-transitions` built with `--features vulkan` default to `MUE_RENDER_BACKEND=auto`. Miniquad tries Vulkan on the real window and initializes OpenGL if Vulkan startup fails. Set `MUE_RENDER_BACKEND=opengl` to force OpenGL or `MUE_RENDER_BACKEND=vulkan` to require Vulkan and report initialization errors. This is a startup choice, not a switch after rendering has started. The `vulkan-shaders` example remains strict when built with the feature on a supported target.

## Integration examples

In [the portal host](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/lib.rs), the host owns a concrete `MockChannel` and calls its resize/render/texture operations directly. [input.rs](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/input.rs) contains the local `ChartInput`, event collection/replay, pointer ownership, Hold-on-resume, and cancel behavior; [target.rs](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/target.rs) maps a logical rect to the actual target pixel size. DPI and resize policy belong to the host. The public `GameController`/`GameChannel` layer and related root exports have been removed; Portal, RenderStateGuard, and SharedTexture/SafeTexture remain in their existing modules. Here "channel" can still describe the external renderer.

Chart events replay at frame start; App preplans the complete UI pointer/hover/wheel batch against the displayed tree and dispatches it before transitions/tasks/layout/render. The portal host keeps mode and pause-overlay visibility fixed until both subscribers finish input, then publishes snapshot/mode in `app.frame_with_update(...)`, before layout/render. The new overlay is drawn that frame but cannot consume the earlier batch's Down. Mouse Move uses the local state during replay; it does not query the final global mouse-button state for the entire batch. Retry explicitly cancels the old attempt's pointers; target resize also has an explicit cancel boundary. Input routes through the map for the last displayed texture, with the initial target resized before the first batch.

The portal collector and UI PointerManager use [mue input cursors][input-api], sharing one legacy macroquad input source: `InputSubscriber::new()`, then `read()`, handle `InputBatch::is_interrupted()`, and `repeat(&mut handler)`. No consumer queue is allocated; unread events expire each frame, even with no App. Use cursors in the main application future and synchronous callbacks, before `next_frame`, rather than automatic macroquad coroutines. On a missed frame portal clears its buffered/held state and cancels chart pointers; normal game pause still retains owner/Hold semantics. `App::frame_paused()` discards UI input and cancels capture/gestures before ticking tasks. The [Scene runtime][scenes-guide] documents App scheduling; the portal example demonstrates an external renderer without Scene management.

## Android APK builds

From the repository root, with the existing Android app project, NDK, `cargo-ndk` and the `aarch64-linux-android` Rust target:

```sh
./build.sh playground                       # GLES debug APK (default)
./build.sh playground vulkan debug
./build.sh playground vulkan release
./build.sh playground vulkan release --skip-lint  # Local packaging only
./build.sh vulkan-shaders vulkan debug       # Strict Vulkan smoke test
```

Both APK variants build the Rust library in Cargo release mode. The script stages a stripped copy of only the selected example, without modifying the original Cargo binary or the app's saved Java/JNI inputs. Gradle packages from a fresh build directory and publishes the APK after success. Debug and release APKs have separate output directories, but another build of the same variant replaces its output. The default release APK is unsigned and needs signing before installation. Release lint stays enabled unless `--skip-lint` is explicitly passed for local testing.

### Experimental Android Vulkan

With `--features vulkan`, `playground-android` and `atlas-android` try Vulkan on the native window and fall back to GLES at startup. Builds without the feature use GLES. The Vulkan path normally needs Android 13 or newer **and** a compatible GPU driver (Vulkan 1.3 with dynamic rendering, or Vulkan 1.2 with `VK_KHR_dynamic_rendering`). For these two examples, setting `MUE_RENDER_BACKEND=gles` or `MUE_RENDER_BACKEND=vulkan` when building the APK also selects a default for launchers that don't inherit the shell environment. For a standalone native library, use the Android NDK, the `aarch64-linux-android` Rust target, and `cargo-ndk`:

```sh
CXXSTDLIB=c++_static CXXSTDLIB_STATIC=1 cargo ndk --manifest-path examples/Cargo.toml \
  -t arm64-v8a --platform 23 -o /tmp/mue-jni \
  rustc --release --features vulkan --example playground-android -- \
  -C link-arg=-Wl,--no-undefined -C link-arg=-Wl,-Bstatic,-lc++abi,-lunwind,-Bdynamic
```

Package the resulting example library from `/tmp/mue-jni/arm64-v8a` under `jniLibs/arm64-v8a`. The Miniquad Java host must load it by its packaged name. With the static C++ runtime above, the APK does not need a separate `libc++_shared.so`. Vulkan is loaded dynamically, so API 23 remains the GLES build floor; availability of the Vulkan path is checked separately at runtime.

Build `vulkan-shaders-android` instead for a strict smoke test that does not fall back to GLES. It checks the first frame and runs the offscreen pixel checks, including Scene crossfade. After logging success to logcat, it keeps presenting so you can rotate the device or background and resume the app.

### Vulkan shadow comparison

The `shadow-probe` example runs a scrollable grid of identical shadow cards.
Use **Replay scroll** after choosing **Disabled** (analytic, with early culling),
**Enabled** (cache whenever stable and eligible) or **Auto** (normal policy).
Compare the resulting frame times at the same resolution and refresh setting;
these include presentation, so a refresh cap can hide a GPU-time improvement.
`shadow-probe-android` is the same demo as a native library with no GLES fallback.
Use `./build.sh shadow-probe vulkan debug` to package it with the Space Mono
font notice.

On desktop Vulkan, the shader smoke can also benchmark a fixed set of 12
shadows after warm-up. The output reports CPU recording, Vulkan timestamps when
available, and a separate draw-and-wait interval including a one-pixel readback.

```sh
cargo run --release --manifest-path examples/Cargo.toml --features vulkan --example shadow-probe
cargo run --release --manifest-path examples/Cargo.toml --features vulkan --example vulkan-shaders -- --shadow-analytic
cargo run --release --manifest-path examples/Cargo.toml --features vulkan --example vulkan-shaders -- --shadow-mask
```

[input-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/input.rs

[themes-guide]: ../learn/themes.md
[scenes-guide]: ../learn/scenes.md
[scene-transitions-guide]: ../learn/scenes.md#scene-transitions
[i18n-guide]: ../learn/i18n.md
