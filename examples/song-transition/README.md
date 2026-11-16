# Song artwork transition and application settings

An ordinary interactive song-library → details application, with a settings Scene shared by both. The selected illustration expands from its card; returning shrinks it back without rebuilding the library or losing its selection/scroll position.

## Run

```bash
cargo run -p song-transition --offline
```

Click an artwork to open details. Wheel, touch/mouse drag or Up/Down arrows scroll the list; keys 1–3 open a matching visible card. The details button or Esc returns to the original list; Esc on the library exits. Q exits immediately. Navigation/scroll actions during an effect are ignored by the Scene's local policy, not by filtering framework raw input.

Click **Settings** or press **S** in either library or details. **T** switches Dark/Light theme, **M** toggles reduced artwork motion, and **Esc** returns to the same Scene. Changes apply immediately and remain in memory for this application run; no settings file or OS preference integration is provided. Settings navigation itself is immediate. With reduced motion on, the next artwork open/return bypasses animation; with it off, the original expansion/shrink returns.

## Source organization

- [main](<src/main.rs>): font initialization and window entry point.
- [application](<src/application.rs>): owns App, `Rc<Catalog>` and `Settings`, and runs the ordinary App/next_frame loop. Field order drops App before its application resources. There is no validation mode, deadline or automatic navigation script.
- [settings](<src/settings.rs>): an independent application-level `OwnedScope` owns theme and reduced-motion signals. `ReadSettings` exposes read-only handles; only `SettingsScene` receives `SettingsEditor`. Plain application palettes are projected into `Brush` bindings, not a new framework theme container.
- [catalog](<src/catalog.rs>): immutable song metadata and owned, reusable artwork assets. Library gets an owning `Rc<Catalog>`; Details gets one `Song`, and effects own the artwork they need.
- [LibraryScene](<src/scene/library.rs>): owns LibraryView and local selection/scroll intents, pushes details/settings and receives the details result.
- [DetailsScene](<src/scene/details.rs>): owns the selected Song and DetailsView; returns its selected index and an optional, narrowly shared return anchor.
- [SettingsScene](<src/scene/settings.rs>): owns its editor UI, interprets local edit intents and writes the shared settings. It pops without a result: global preferences are not parent/child business results.
- [views](<src/view.rs>) and [settings view](<src/view/settings.rs>): independently owned retained UI; card display geometry and scroll/selection signals remain local to LibraryView.
- [UI components](<src/ui.rs>): `ThemedButton` and `SettingsStatus` resolve the view's typed read-only context during `build`, then retain explicit signal bindings.
- [input](<src/input.rs>): collects keyboard intents locally from the complete InputBatch.
- [effects](<src/effects.rs>): artwork/geometry-only overlays. ReturnTarget is a small copied-geometry channel, not an application-state bag.
- [artwork](<src/artwork.rs>): rounded geometry, UV crop, interpolation and image drawing shared by cards, details and overlays.

## Ownership and dependency boundaries

```text
Application
├── App: retained Library / Details / Settings Scenes and active effect
├── Rc<Catalog>: immutable songs and reusable Rc artwork assets
└── Settings: application OwnedScope with two signals
```

Scenes receive the dependencies they need through constructors. Library/Details carry `ReadSettings` and a narrow `SettingsFactory` capability to open the editor; they do not receive writable setting handles or the whole Application. Each detached view owner explicitly `provide`s `ReadSettings` before constructing its subtree. UI context follows the scope construction chain, not the Scene stack. `inject` is not called from later Scene/effect callbacks.

Copying `Signal`/`ReadSignal` does **not** keep the settings owner alive; cloning `Rc<Artwork>` **does** keep that asset alive. View-derived signals are created in the view owner or individual node build scope, not the application scope. Node trees drop before their view owner; App drops before Settings, including pending destinations and effects. This demo has no asynchronous tasks. If one is added, its cancellation/completion and foreground cleanup must finish before global settings are released; field order alone is not a general task-join mechanism.

There is no all-purpose `Shared`, framework service locator, production test observer, lifecycle counter, pixel readback, `--check` or `--capture` CLI. Details business state returns through `Returned`; global preferences instead update their application-owned source signals. Settings does not reset the selected song or scroll offset.

## Transition behavior

When motion is enabled, opening uses public `overlay + AroundSwitch`: a **0.45 s** exit interpolates the last displayed card rectangle, normalized crop UVs, clip, 18 px corner radius and dimming with `1 - (1-u)^4`. The full-screen endpoint is presented before Details is pushed. A **0.30 s** enter removes that cover to reveal the Details UI.

Return covers details in **0.24 s**, then pops with a typed result. Library receives Returned before Resume and refreshes the return target after real layout/render. The **0.40 s** enter shrinks to the actual card; an unavailable/offscreen target fades out. Reduced motion still returns the typed selected index, but needs no geometry channel or effect. The preference is sampled at request time, not used to rewrite an already running effect. No Scene screenshot or additional RenderTarget is involved.

Three original images are generated by [the deterministic offline generator](<assets/generate.py>); Python is not needed at runtime. The [font](<../playground/assets/fonts/NotoSans-Regular.ttf>) and [license](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>) are reused from the workspace.

## Integration limits

Use the normal application above to exercise artwork navigation, shared settings, theme changes and selected-card/scroll preservation. No full Phira formats/audio, device touch, arbitrary transformed shared-element matching or other GPU backend validation is claimed. See [Scene transitions](<../../docs/scene-transitions.md>) for the framework contract.
