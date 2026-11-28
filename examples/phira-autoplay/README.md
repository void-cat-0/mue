# Phira autoplay — local PEZ library and play flow

A desktop Vulkan/OpenGL and Android Vulkan-first/GLES-fallback example on the same Mue Scene stack: **Library → song details → loading → autoplay → results → retry or the original details**. Back from details shrinks the artwork back to the retained library card. `song-transition` and `scene-flow` remain separate examples.

## Run

Put your own `ManaBlast.pez` and `ExoplanetaryMirage.かめりあ-AT.pez` in `examples/phira-autoplay/charts/`. The desktop example also looks for them beside the executable, in the current directory, and in the checkout root, or you can pass PEZ paths explicitly. Chart packs stay out of Git and are not embedded in the desktop executable. On Linux, the optional audio build needs ALSA development files (`alsa-lib` on Arch, `libasound2-dev` on Debian/Ubuntu), `pkg-config`, and a working default output device.

```sh
MUE_RENDER_BACKEND=vulkan cargo run --release --locked --manifest-path examples/Cargo.toml \
  --features vulkan,phira-autoplay --example phira-autoplay

# Explicit silent clock for a machine without an audio device; still decodes the real charts and tracks.
MUE_RENDER_BACKEND=vulkan cargo run --release --locked --manifest-path examples/Cargo.toml \
  --features vulkan,phira-autoplay --example phira-autoplay -- --silent /path/to/track.pez
```

Desktop development accepts `MUE_RENDER_BACKEND=opengl`, `gles`, or `vulkan`. The Windows release feature `phira-autoplay-vulkan-only` forces Vulkan with no fallback. The Android APK uses `phira-autoplay-vulkan-preferred`: it tries Vulkan first, automatically falls back to GLES if startup checks fail, and can cold-restart in GLES after repeated surface failures or a Vulkan panic. If Vulkan cannot initialize, the startup log reports the loader API and each detected GPU’s Vulkan API version, or why they could not be read. On desktop the report goes to the terminal, including in Vulkan-only builds. On Android it is included in the exportable log and logcat. Audio output errors still appear on a retryable screen rather than silently switching to silence.

- **Library:** click a card or press `1`–`9` to open it; `Esc` exits the app.
- **Details:** Play, `Enter` or `Space` starts loading; Back returns to the same scroll and selection.
- **Loading:** use Cancel or `Esc` to return. The audio starts only after the first playing frame can be presented.
- **Playing:** `Esc`, `P`, `Space` or the pause button pauses. Resume continues at the same music position; `R` in Pause starts over with the loaded data.
- **Results:** Retry (`R`) starts a fresh attempt from the same PCM and parsed chart; Continue (`Enter`) returns the result to the retained details. Results are marked `AUTOPLAY` and are not saved or submitted anywhere.

## Local note skin and hitsounds

Put `live 测试5.zip` (or `live-skin.zip`), `click.ogg`, `drag.ogg`, and `flick.ogg` in `charts/` alongside the two PEZ files, beside the executable, in the current directory, or at the Mue checkout root. The **Live** skin by XingLuELEC supplies the normal and simultaneous-note textures. Hold caps and body use `holdAtlas` / `holdAtlasMH` from its `info.yml`; the pack's `hit_fx.png` is not used. `sasa` mixes `click.ogg` at tap and hold starts, `drag.ogg` at drags, and `flick.ogg` at flicks. Fake notes and hold sustain/release don't play hitsounds. `--silent` suppresses music and hitsounds, not the skin.

Missing skin or sounds fall back to the procedural notes or no hitsound, respectively. A malformed optional asset is reported without blocking song playback. The original user-provided files stay outside Git.

## Build release packages

On a Linux host with the Rust Windows GNU target and the MinGW-w64 toolchain:

```sh
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++ \
CXXSTDLIB_STATIC=1 \
cargo build --release --locked --manifest-path examples/Cargo.toml \
  --target x86_64-pc-windows-gnu --features phira-autoplay-vulkan-only --example phira-autoplay
```

For a local Windows test ZIP, place both PEZ files beside `phira-autoplay.exe` under their original filenames and include the optional media there as well. Check DLL imports with `x86_64-w64-mingw32-objdump -p`. The user's graphics driver must provide the Vulkan loader and dynamic rendering.

For Android, set `ANDROID_HOME` and `ANDROID_NDK_HOME` (NDK r27), and install the Rust `aarch64-linux-android` target and `cargo-ndk`. The packaging script builds an installable, debug-signed ARM64 APK with both PEZ files from `charts/`. The APK stores Exoplanetary Mirage under the ASCII asset name `ExoplanetaryMirage-AT.pez` without changing the pack. Optional skin and hitsounds are taken from `charts/` or the checkout root if present. On launch the app copies the assets intact to app-private storage. The script doesn't modify the other examples' Android project. Set `ANDROID_DEBUG_KEYSTORE` to the same debug keystore used for a previous APK to install as an update.

```sh
examples/phira-autoplay/package-android.sh --output /tmp/phira-autoplay-vulkan-arm64.apk
```

Android 启动时优先探测 Vulkan；初始化不通过会记录原因、加载器 API 版本和各 GPU 报告的 Vulkan API 版本（无法查询时会注明），并在当前启动流程直接回退 GLES。右上角原生诊断面板会显示实际生效的图形 API：Vulkan 同时显示应用使用的版本、渲染路径与 GPU 报告版本，GLES 显示实际上下文的 `GL_VERSION` 版本。即使游戏没有画面也可查看面板、导出日志。运行时若 Vulkan 活跃期间出现 panic，或 Surface／交换链连续 8 次恢复失败，会记录 backtrace／VkResult 后自动冷启动 GLES。屏幕上方的“导出启动日志”打开 Android 文件选择器，保存设备/GPU/API、Surface、后端、fallback、panic/backtrace 等记录；“GLES 重启”可手动强制 GLES 对照，“自动模式重启”恢复 Vulkan-first。APK 横屏，音频后端是 sasa Oboe（最低 API 26），Vulkan 动态渲染及窗口呈现能力在运行时探测。Windows Vulkan-only 包没有 GL fallback。

The shared demo, including Android's `quad_main` and JNI exports, lives in `src/lib.rs`. The desktop executable uses it as an ordinary module from `src/main.rs`; the APK doesn't include or call the desktop entry point. The audio dependency matches Phira's pinned `sasa` revision (`e76229b`), with CPAL on desktop and Oboe on Android. The Android build doesn't select CPAL's AAudio backend.

## Dependency licensing

Projects that depend on `sasa` may be distributed as open source, including executables and APKs that link it, without a requirement to use a particular downstream license. Mue and this example remain MIT; `sasa` itself is **All Rights Reserved** and is referenced as an external Git dependency, not vendored or covered by Mue's MIT license. The permission is for downstream projects and their builds, not for distributing `sasa` on its own.

## What is reproduced

- `info.yml` supplies the real title, credits, illustration, media paths and offsets. The loader probes audio bytes, so ManaBlast's `.wav` member is correctly opened as MP3 and Exoplanetary Mirage's track as Ogg/Vorbis.
- The example supports RPE legacy speed (ManaBlast) and PGR v3 (Exoplanetary Mirage), with rotated and moving judge lines, opacity, summed event layers, stops/reversals and holds spanning speed changes. Autoplay counts real point notes at their start and holds at their end; fake notes do not score. The notes use the supplied Live textures when available, otherwise original procedural geometry. Judge lines and hit effects are still procedural. The renderer only samples time and does not handle input or advance during a transition capture.
- A background worker reads/parses/decodes; Mue uploads artwork in an ordinary Scene update. One PlayFlow owns loading, the audio device/clock and the result, so there is no gameplay Scene to wake when returning directly from results. Sasa decodes the music and its `Music::position()` drives the chart and progress bar. Retry keeps the audio manager and decoded clip, replacing only the `Music` handle so the next attempt starts at zero.
- This is not a full Phira client or a compatibility claim for arbitrary chart packs. No manual judgement, replay, online scores, chart shaders, video, external resource packs, `attach_ui`, or PEC parser. It does not import Phira's GPL code or include music or art in the source repository.

With both local PEZ files, run the opt-in regression test:

```sh
cargo test --locked --manifest-path examples/Cargo.toml --features vulkan,phira-autoplay \
  --example phira-autoplay supplied_charts_decode_at_the_expected_timing -- --ignored --nocapture
```
