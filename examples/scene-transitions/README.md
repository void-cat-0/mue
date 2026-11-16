# Scene transitions

## Run

From the workspace root, launch the normal interactive application:

```bash
cargo run -p scene-transitions --offline
```

- **1** pushes a new colored Scene with the built-in crossfade.
- **2** pushes a new Scene with a custom sliding picture composite.
- **3** pushes a new Scene with a two-phase black-cover overlay.
- **Esc** pops the child using the effect that opened it; on the root it exits.
- **Q** exits the whole App immediately, without a transition.

The window shows the controls and a moving marker. Each new child owns a fresh
animation clock, pending action, palette and return effect. Covered Scenes keep
their state but do not receive ordinary updates, so their marker pauses until
returning. Animation uses the normal Scene frame delta, not a scripted clock.
No external font, chart or image assets are required.

## Modules

- [main.rs](<src/main.rs>): window configuration and the normal
  `App::try_frame()` / `next_frame().await` loop.
- [color_scene.rs](<src/color_scene.rs>): Scene-local state, input handling,
  deferred `SceneContext::push/pop/exit` requests and custom drawing.
- [input.rs](<src/input.rs>): a local keyboard handler translates each input
  batch into an `Action`; repeated keys are ignored and the last recognized
  command in the batch wins.
- [effects.rs](<src/effects.rs>): fresh public `SceneTransition` values for
  fade, slide and cover. Draw callbacks have no shared Scene state.

## Rendering limits and tests

Crossfade and slide require the OpenGL backend and the full, origin-zero logical
window. Keep App's supplied capture/composition camera active, balance model
matrices, and do not enter raw miniquad passes inside Scene/effect drawing. The
slide draws the borrowed render-target textures with `flip_y: true`. Cover uses
`TransitionTiming::AroundSwitch`, needs no picture targets, and covers the exit
endpoint before the stack changes. See the [transition tutorial](<../../docs/scene-transitions.md>)
for timing, interruption and rendering restrictions.

Module-local unit tests cover keyboard mapping, independent child state, slide
geometry and cover opacity without constructing an App or GPU resources:

```bash
cargo test -p scene-transitions --offline
```

These pure behavior tests are not rendering, device-input, Metal or audio tests;
use the normal application above for interactive rendering.
