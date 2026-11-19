# Song artwork transition, settings and reactive localization

An ordinary interactive song-library → details application, with a settings Scene shared by both. The selected illustration expands from its card; returning shrinks it back without rebuilding the library or losing its selection/scroll position. The retained UI consumes `mue-i18n` in English (`en-US`) and Simplified Chinese (`zh-CN`).

## Run

```bash
cargo run -p song-transition --offline
```

Click an artwork to open details. Wheel, touch/mouse drag or Up/Down arrows scroll the list; keys 1–3 open a matching visible card. The details button or Esc returns to the original list; Esc on the library exits. Q exits immediately. Navigation/scroll actions during an effect are ignored by the Scene's local policy, not by filtering framework raw input.

Click **Settings** or press **S** in either library or details. **T** switches Dark/Light theme, **M** toggles reduced artwork motion, **L** switches English/Simplified Chinese (also available as the language button), and **Esc** returns to the same Scene. Changes apply immediately and remain in memory for this application run; no settings file or OS preference integration is provided. Settings navigation itself is immediate. With reduced motion on, the next artwork open/return bypasses animation; with it off, the original expansion/shrink returns.

## Source organization

- [main](<src/main.rs>): font initialization and window entry point.
- [application](<src/application.rs>): owns App, `Rc<Catalog>`, `Settings` and `Rc<I18n>`, and runs the ordinary App/next_frame loop. Field order drops App before its application resources. There is no validation mode, deadline or automatic navigation script.
- [settings](<src/settings.rs>): an independent application-level `OwnedScope` owns theme and reduced-motion signals. `ReadSettings` exposes read-only handles; only `SettingsScene` receives `SettingsEditor`. Plain application palettes are projected into `Brush` bindings, not a new framework theme container.
- [localization](<src/localization.rs>): embeds the eight Fluent resources, builds the catalog and provides `ReadTranslations` (four strong read-only namespace translators and the locale signal).
- [catalog](<src/catalog.rs>): immutable song metadata, localized description keys and owned, reusable artwork assets. Library gets an owning `Rc<Catalog>`; Details gets one `Song`, and effects own the artwork they need.
- [LibraryScene](<src/scene/library.rs>): owns LibraryView and local selection/scroll intents, pushes details/settings and receives the details result.
- [DetailsScene](<src/scene/details.rs>): owns the selected Song and DetailsView; returns its selected index and an optional, narrowly shared return anchor.
- [SettingsScene](<src/scene/settings.rs>): owns its editor UI, interprets local edit intents and writes the shared settings/locale. Only this Scene receives the writable `Rc<I18n>` capability. It pops without a result: global preferences are not parent/child business results.
- [views](<src/view.rs>) and [settings view](<src/view/settings.rs>): independently owned retained UI; card display geometry and scroll/selection signals remain local to LibraryView.
- [UI components](<src/ui.rs>): `ThemedButton` and `SettingsStatus` resolve the view's typed read-only contexts during `build`, then retain explicit signal/translation bindings.
- [input](<src/input.rs>): collects keyboard intents locally from the complete InputBatch.
- [effects](<src/effects.rs>): artwork/geometry-only overlays. ReturnTarget is a small copied-geometry channel, not an application-state bag.
- [artwork](<src/artwork.rs>): rounded geometry, UV crop, interpolation and image drawing shared by cards, details and overlays.

## Ownership and dependency boundaries

```text
Application
├── App: retained Library / Details / Settings Scenes and active effect
├── Rc<Catalog>: immutable songs and reusable Rc artwork assets
├── Settings: application OwnedScope with two signals
└── Rc<I18n>: immutable Fluent catalog and owned locale signal
```

Scenes receive the dependencies they need through constructors. Library/Details carry `ReadSettings`, `ReadTranslations` and a narrow `SettingsFactory` capability to open the editor; they do not receive writable setting/locale handles or the whole Application. The factory captures writable `Rc<I18n>` only to construct `SettingsScene`; none is installed in UI context. Each detached view owner explicitly `provide`s `ReadSettings` and `ReadTranslations` before constructing its subtree. UI context follows the scope construction chain, not the Scene stack. `inject` is not called from later Scene/effect callbacks.

Copying `Signal`/`ReadSignal` does **not** keep the corresponding owner alive; cloning `Rc<Artwork>` **does** keep that asset alive, and cloning `Translator` **does** keep the i18n service and its locale owner alive. `ReadTranslations` is Clone, not Copy; `ReadSettings` and `SettingsEditor` remain the original non-owning Copy handles. View-derived signals and translation Props are created in the view owner or individual node build scope, not the application/i18n scope. Node trees drop before their view owner; App drops before Settings and I18n, including pending destinations and effects. This demo has no asynchronous tasks. If one is added, its cancellation/completion and foreground cleanup must finish before global settings are released; field order alone is not a general task-join mechanism.

There is no all-purpose `Shared`, framework service locator, production test observer, lifecycle counter, pixel readback, `--check` or `--capture` CLI. Details business state returns through `Returned`; global preferences instead update their application-owned source signals. Settings does not reset the selected song or scroll offset.

## Reactive i18n consumption

The initial UI language is explicitly `en-US`, not detected from the system. Settings cycles the exact identifiers returned by `available_locales()`: `en-US` and `zh-CN`. A locale update invalidates the existing text bindings; it does not recreate Library/Details/Settings, reload artwork, reset selection/scroll, or alter the animation policy. Theme and reduced motion remain independent settings.

Resources are split into [common English](<locales/en-US/common.ftl>) / [Chinese](<locales/zh-CN/common.ftl>) (status, metadata), [library English](<locales/en-US/library.ftl>) / [Chinese](<locales/zh-CN/library.ftl>), [details English](<locales/en-US/details.ftl>) / [Chinese](<locales/zh-CN/details.ftl>), and [settings English](<locales/en-US/settings.ftl>) / [Chinese](<locales/zh-CN/settings.ftl>). Song titles, artist names and difficulty codes stay as catalog metadata; their descriptive text and the BPM label are localized.

The library summary passes the real song count to the English Fluent `[one]`/`[other]` plural and reads the retained selection signal for a live one-based selected index (`0` means no selection). Open a visible card and return to see that parameter update without rebuilding the list. A representative binding, created inside `owner.run`, is:

```rust
let count = songs.len();
let summary = tl!(translations.library, "summary",
    "count" => count,
    "selected" => selected.get().map_or(0, |index| index + 1),
);
```

`text`, `text_with`, `bind` and `tl!` immediately create their graph in a live, non-null **consumer** scope. Every use here is in view `owner.run` or a component `build` body. Argument providers read existing Signal/Prop handles and assemble `FluentArgs`; they do not call `map`/`computed`, allocate reactive resources, perform I/O or mutate state. Locale strings and translated description Props use `.get_clone().to_string()` to supply owned dynamic Fluent arguments on each evaluation.

**Intentional fallback:** Chinese details omit the nonessential `artwork-tip`, so that one tip remains English via the fixed `en-US` fallback. Details uses `bind("artwork-tip", FluentArgs::new)` and projects the returned text with `map` in the same owner scope; the full `Translation` also carries `resolved_locale` (here `en-US`) and issues. This is ordinary UI consumption, not a diagnostic/check mode. All essential navigation and settings strings are translated.

## Transition behavior

When motion is enabled, opening uses public `overlay + AroundSwitch`: a **0.45 s** exit interpolates the last displayed card rectangle, normalized crop UVs, clip, 18 px corner radius and dimming with `1 - (1-u)^4`. The full-screen endpoint is presented before Details is pushed. A **0.30 s** enter removes that cover to reveal the Details UI.

Return covers details in **0.24 s**, then pops with a typed result. Library receives Returned before Resume and refreshes the return target after real layout/render. The **0.40 s** enter shrinks to the actual card; an unavailable/offscreen target fades out. Reduced motion still returns the typed selected index, but needs no geometry channel or effect. The preference is sampled at request time, not used to rewrite an already running effect. No Scene screenshot or additional RenderTarget is involved.

Three original images are generated by [the deterministic offline generator](<assets/generate.py>); Python is not needed at runtime. The existing [Noto Sans Regular](<../playground/assets/fonts/NotoSans-Regular.ttf>) (Copyright 2022 The Noto Project Authors; [SIL OFL license](<../playground/assets/fonts/OFL-NotoLatinGreekCyrillic.txt>)) and [Noto Sans CJK SC Regular](<../playground/assets/fonts/NotoSansCJKsc-Regular.otf>) (© 2014–2021 Adobe; [SIL OFL license](<../playground/assets/fonts/OFL-NotoCJK.txt>)) are embedded unchanged from the workspace, without a download or new font asset. Latin-only Noto Sans is insufficient for the Chinese UI; the views explicitly choose `Noto Sans CJK SC`. Font-system initialization remains one-shot and separate from message locale switching. The SC font supplies Simplified Chinese regional glyph forms, not a promise of all-language Unicode coverage; see the existing [font notices](<../playground/assets/fonts/NOTICE.txt>).

## Integration limits

Use the normal application above to exercise artwork navigation, shared settings, theme/locale changes and selected-card/scroll preservation. The example does not implement full Phira formats or audio, device touch, arbitrary transformed shared-element matching, or other GPU backends. See [Scene transitions](<../../docs/scene-transitions.md>) for the framework contract.
