# Build a release checklist

Build a desktop editor for a release checklist in a plain text file. You will edit and filter rows, save from a button or keyboard shortcut, and open a preferences screen to change the theme and language. The editor stays mounted while preferences cover it.

| Step | What you implement | Course chapter |
| --- | --- | --- |
| 1 | Window project and file format | [Your first window][getting-started-guide] |
| 2 | Shared theme and message catalog | [Shared appearance][themes-guide] · [Localized interfaces][i18n-guide] |
| 3 | Keyed rows, editor controls, and animated progress | [Reactive state and children][basics-guide] · [Layout, paint, and motion][style-guide] · [Reusable views and custom nodes][components-guide] |
| 4 | Scene ownership, shortcuts, and a preferences screen | [Screen navigation][scenes-guide] · [Input routing][scene-input-guide] |
| 5 | Background loads and saves | [Asynchronous work][tasks-guide] |

Copy the Rust blocks in order into one `src/main.rs`. The next section adds the two Fluent files and the checklist data.

## 1. Create the project and file format

Run `cargo new release-checklist`. Add Mue under `[dependencies]` and enable Fluent support.

```toml
mue = { git = "https://github.com/void-cat-0/mue", branch = "main", features = ["i18n"] }
```

Put `release.txt` beside `Cargo.toml`.

```text
[x] Tag the release
[ ] Smoke-test installer
[ ] Publish release notes
```

Each line contains a completion marker and a title. Keep file order so the original index can identify a row throughout this editing session. If you add reordering or insertion, store an ID in the file and resolve it when a control activates.

```rust,ignore
use std::{
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use mue::{
    App, Scene,
    components::{Theme, TokenSet, provide_theme, scroll_area, token_set},
    core::{
        Size,
        task::{Task, spawn, spawn_blocking},
    },
    i18n::{CatalogBuilder, I18n, Translator, langid, tl},
    macroquad,
    macroquad::miniquad::{EventHandler, KeyCode, KeyMods},
    prelude::*,
    renderer::{
        CallbackResult, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
        SceneTransition, TransitionPhase, TransitionTiming,
        color::Color,
        node::{flexbox::use_container, map_keyed_prop},
        shader::{Brush, solid},
        transition::Transition,
    },
};

#[derive(Clone, PartialEq)]
struct Item {
    title: String,
    done: bool,
}

fn load(path: &Path) -> io::Result<Vec<Item>> {
    fs::read_to_string(path)?
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let row = line
                .strip_prefix("[ ] ")
                .map(|title| (false, title))
                .or_else(|| line.strip_prefix("[x] ").map(|title| (true, title)))
                .filter(|(_, title)| !title.trim().is_empty());
            let (done, title) = row.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid checklist line {}", index + 1),
                )
            })?;
            Ok(Item {
                title: title.into(),
                done,
            })
        })
        .collect()
}

fn save(path: &Path, items: &[Item]) -> io::Result<()> {
    let mut contents = String::new();
    for item in items {
        contents.push_str(if item.done { "[x] " } else { "[ ] " });
        contents.push_str(&item.title);
        contents.push('\n');
    }
    let pending = path.with_extension("txt.tmp");
    fs::write(&pending, contents)?;
    fs::rename(pending, path)
}
```

A parse error reports the line number and leaves the editor unable to save. On write, create a sibling temporary file first and rename it over the original after the write succeeds. Run one editor per file. This example expects exclusive use of the temporary filename and does not synchronize edits from other programs.

## 2. Share appearance and messages

Create `src/en.ftl` and `src/fr.ftl`. Either screen can use the `release` namespace. Keep checklist titles in the file and translate the interface around them.

`src/en.ftl`

```ftl
title = Release checklist
progress = { $done } of { $total } complete
save = Save
open-only = Open only
show-all = Show all
mark = Done
undo = Undo
preferences = Preferences
back = Back
light = Light
dark = Dark
language = Language
appearance = Appearance
loading = Opening release.txt…
saving = Saving…
saved = All changes saved
unsaved = Unsaved changes
load-error = Open failed: { $error }
save-error = Save failed: { $error }
```

`src/fr.ftl`

```ftl
title = Liste de publication
progress = { $done } sur { $total } terminés
save = Enregistrer
open-only = À faire
show-all = Tout afficher
mark = Terminer
undo = Annuler
preferences = Préférences
back = Retour
light = Clair
dark = Sombre
language = Langue
appearance = Apparence
loading = Ouverture de release.txt…
saving = Enregistrement…
saved = Modifications enregistrées
unsaved = Modifications en attente
load-error = Échec de l’ouverture : { $error }
save-error = Échec de l’enregistrement : { $error }
```

Both Scene roots will install the same theme handle. Register brush keys once and apply a new set in place. Reuse the same `I18n` for both screens so changing its locale updates their bindings.

```rust,ignore
struct Session {
    theme: Rc<Theme>,
    i18n: I18n,
}

impl Session {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let theme = Rc::new(Theme::new());
        theme.register(&palette(false));
        let mut catalog = CatalogBuilder::new();
        catalog.add_ftl(langid!("en-US"), "release", include_str!("en.ftl"))?;
        catalog.add_ftl(langid!("fr"), "release", include_str!("fr.ftl"))?;
        let i18n = I18n::new(catalog.build()?, langid!("en-US"))?;
        Ok(Self { theme, i18n })
    }

    fn messages(&self) -> Translator {
        self.i18n.translator("release").expect("release catalog")
    }
}

fn palette(dark: bool) -> TokenSet<Brush> {
    let (canvas, panel, ink, track, accent) = if dark {
        (0x172338, 0x24344c, 0xf0f5ff, 0x394d6c, 0x83c4ff)
    } else {
        (0xe8eff6, 0xffffff, 0x182d43, 0xcddbea, 0x335fc4)
    };
    let brush = |rgb: u32| {
        solid(Color::from_rgba(
            (rgb >> 16) as u8,
            (rgb >> 8) as u8,
            rgb as u8,
            255,
        ))
    };
    token_set! {
        canvas: brush(canvas), panel: brush(panel), ink: brush(ink),
        track: brush(track), accent: brush(accent),
    }
}
```

## 3. Bind rows, controls, and progress

`Editor` groups signal handles that the view and Scene both need. The Scene owns their scope. The view owns its filter and projections. It uses `map_keyed_prop` so each surviving row can read its current completion state without remounting its button.

```rust,ignore
#[derive(Clone, PartialEq)]
enum DiskState {
    Loading,
    Ready,
    Saving,
    LoadError(String),
    SaveError(String),
}

#[derive(Clone, Copy)]
struct Editor {
    items: Signal<Vec<Item>>,
    saved: Signal<Vec<Item>>,
    disk: Signal<DiskState>,
    save_requested: Signal<bool>,
    preferences_requested: Signal<bool>,
}

impl Editor {
    fn new() -> Self {
        Self {
            items: signal(Vec::new()),
            saved: signal(Vec::new()),
            disk: signal(DiskState::Loading),
            save_requested: signal(false),
            preferences_requested: signal(false),
        }
    }

    fn busy(self) -> bool {
        matches!(
            self.disk.get_clone(),
            DiskState::Loading | DiskState::Saving | DiskState::LoadError(_)
        )
    }

    fn dirty(self) -> bool {
        self.items.get_clone() != self.saved.get_clone()
    }
}

fn control_style() -> Style {
    style! { font-size: 16px; min-height: 38px; padding: 6px 12px; }
}

#[node]
fn progress_fill(ctx: NodeContext) -> Node {
    ctx.finish(Style::new(), use_container)
}

#[component]
fn release_progress(done: usize, total: usize) -> impl Component {
    let width = computed(move |_| {
        let total = total.get();
        let fraction = if total == 0 {
            0.
        } else {
            done.get() as f32 / total as f32
        };
        Size::Percent(fraction * 100.)
    });
    flexbox()
        .style(style! {
            width: 100%; height: 8px; border-radius: 4px;
            background: token!(track); overflow: hidden;
        })
        .children(progress_fill().style(style! {
            width: {width}; height: 100%; border-radius: 4px;
            background: token!(accent); transition: 240ms ease-out;
        }))
}

#[component]
fn checklist(
    #[model] items: Vec<Item>,
    open_only: bool,
    busy: bool,
    messages: Translator,
) -> impl Component {
    let messages = messages.get_clone();
    let visible = computed(move |_| {
        let open_only = open_only.get();
        items
            .get_clone()
            .into_iter()
            .enumerate()
            .filter(|(_, item)| !open_only || !item.done)
            .collect::<Rc<[_]>>()
    });
    let rows =
        map_keyed_prop(
            visible,
            |entry| entry.0,
            move |row| {
                let index = row.key;
                let label = row.value.map(|(_, item)| Rc::<str>::from(item.title));
                let done = row.value.map(|(_, item)| item.done);
                hstack().style(style! {
            width: 100%; padding: 8px 12px; gap: 12px; border-radius: 8px;
            background: token!(panel); justify-content: space-between; align-items: center;
        }).children((
            text(label).style(style! { font-size: 18px; color: token!(ink); }),
            button(done.pick(messages.text("undo"), messages.text("mark")))
                .style(control_style()).disabled(busy.clone())
                .on_activate(move || items.update(|items| {
                    if let Some(item) = items.get_mut(index) { item.done = !item.done; }
                })),
        ))
            },
        );
    scroll_area()
        .style(style! { width: 100%; height: 218px; max-height: 45vh; })
        .content_style(style! { gap: 8px; })
        .children(rows)
}
```

The filter carries each item's original index into `map_keyed_prop`. Completion changes update the `value` property for rows that remain visible. When **Open only** hides a completed row, its node leaves the tree and the data stays in `items`. The progress component takes read-only properties. The parent calculates completion from the full checklist. The terminal `progress_fill` node installs layout and background consumers with `use_container`. Its caller supplies the animated width and color.

```rust,ignore
#[component]
fn editor_view(editor: Editor, session: Rc<Session>) -> impl Component {
    let editor = editor.get();
    let session = session.get_clone();
    let messages = session.messages();
    let open_only = signal(false);
    let busy = computed(move |_| editor.busy());
    let dirty = computed(move |_| editor.dirty());
    let phase = editor.disk;
    let done = editor
        .items
        .map(|items| items.iter().filter(|item| item.done).count());
    let total = editor.items.map(|items| items.len());

    let status = {
        let locale = session.i18n.locale();
        let messages = messages.clone();
        computed(move |_| {
            locale.track();
            let state = phase.get_clone();
            let key = match state {
                DiskState::Loading => "loading",
                DiskState::Saving => "saving",
                DiskState::LoadError(_) => "load-error",
                DiskState::SaveError(_) => "save-error",
                DiskState::Ready if dirty.get() => "unsaved",
                DiskState::Ready => "saved",
            };
            let args = match &state {
                DiskState::LoadError(error) | DiskState::SaveError(error) => {
                    Some(mue::i18n::fluent_args!("error" => error.as_str()))
                }
                _ => None,
            };
            messages.format(key, args.as_ref()).text
        })
    };
    let toolbar = hstack().gap(8.).children((
        button(open_only.pick(messages.text("show-all"), messages.text("open-only")))
            .style(control_style())
            .on_activate(move || open_only.toggle()),
        button(messages.text("save"))
            .primary(true)
            .style(control_style())
            .loading(computed(move |_| phase.get_clone() == DiskState::Saving))
            .disabled(computed(move |_| busy.get() || !dirty.get()))
            .on_activate(move || editor.save_requested.set(true)),
        button(messages.text("preferences"))
            .style(control_style())
            .on_activate(move || editor.preferences_requested.set(true)),
    ));
    centered(
        vstack()
            .style(style! { width: 100%; max-width: 480px; gap: 10px; })
            .children((
                text(messages.text("title")).style(style! { font-size: 30px; color: token!(ink); }),
                text(tl!(messages, "progress", "done" => done.get(), "total" => total.get()))
                    .style(style! { font-size: 17px; color: token!(ink); }),
                release_progress(done, total),
                toolbar,
                checklist(editor.items, open_only, busy, Prop::Static(messages)),
                text(status).style(style! { font-size: 16px; color: token!(ink); }),
            )),
    )
}

fn centered(view: impl Component + 'static) -> impl Component {
    center()
        .style(style! {
            width: 100%; height: 100%; padding: 20px; background: token!(canvas);
        })
        .children(view)
}
```

`tl!` reads the progress arguments as they change. The status needs a variable key, so it calls `format` inside a computation. `format` reads the locale without subscribing, so the computation tracks `I18n::locale()` before formatting. The button derives its disabled state from the disk phase and the saved snapshot. Undoing an edit can make the list clean without a save.

<figure class="mue-chart">

![An unsaved edit shown in the editor after the theme and locale changed][shot-release-dirty]

<figcaption>The progress bar counts all three items. The list shows only the remaining open item.</figcaption>
</figure>

## 4. Keep the editor below preferences

A `Scene` holds its owned `Node` throughout layout and drawing. Reuse a small screen wrapper because these screens each submit one full-window root. A preferences Scene owns its Back flag and shares `Session` with the editor. The overlay draws a solid cover at the commit point, so this navigation needs no scene picture capture.

```rust,ignore
struct ViewRoot(Node);

impl ViewRoot {
    fn themed<V: IntoNode>(session: &Session, build: impl FnOnce() -> V) -> Self {
        Self(provide_theme(session.theme.clone(), build).into_node())
    }

    fn layout(&self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) {
        roots.push(SceneRoot::new(*self.0, frame.viewport));
    }

    fn render(&self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        render.draw(*self.0)?;
        Ok(())
    }
}

#[derive(Default)]
struct Shortcuts {
    save: bool,
    preferences: bool,
    back: bool,
}

impl EventHandler for Shortcuts {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, mods: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::S if mods.ctrl || mods.logo => self.save = true,
            KeyCode::F2 => self.preferences = true,
            KeyCode::Escape => self.back = true,
            _ => {}
        }
    }
}

fn cover() -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::ease_out(0.12),
            enter: Transition::ease_out(0.18),
        },
        |frame| {
            let opacity = match frame.phase {
                TransitionPhase::BeforeSwitch => frame.progress,
                TransitionPhase::AfterSwitch => 1. - frame.progress,
            };
            let v = frame.viewport;
            macroquad::shapes::draw_rectangle(
                v.x,
                v.y,
                v.w,
                v.h,
                macroquad::color::Color::new(0.08, 0.12, 0.20, opacity),
            );
            Ok(())
        },
    )
}

struct Preferences {
    root: ViewRoot,
    back: Signal<bool>,
    _scope: OwnedScope,
}

impl Preferences {
    fn new(session: Rc<Session>) -> Self {
        let scope = OwnedScope::new();
        let back = scope.run(|| signal(false));
        let root = ViewRoot::themed(&session, || preferences_view(session.clone(), back));
        Self {
            root,
            back,
            _scope: scope,
        }
    }
}

fn preferences_view(session: Rc<Session>, back: Signal<bool>) -> impl Component {
    let messages = session.messages();
    let light = session.clone();
    let dark = session.clone();
    let english = session.clone();
    centered(
        vstack()
            .style(style! { width: 100%; max-width: 480px; gap: 12px; })
            .children((
                text(messages.text("preferences"))
                    .style(style! { font-size: 30px; color: token!(ink); }),
                text(messages.text("appearance"))
                    .style(style! { font-size: 18px; color: token!(ink); }),
                hstack().gap(8.).children((
                    button(messages.text("light"))
                        .style(control_style())
                        .on_activate(move || {
                            light.theme.apply(&palette(false));
                        }),
                    button(messages.text("dark"))
                        .style(control_style())
                        .on_activate(move || {
                            dark.theme.apply(&palette(true));
                        }),
                )),
                text(messages.text("language"))
                    .style(style! { font-size: 18px; color: token!(ink); }),
                hstack().gap(8.).children((
                    button("English")
                        .style(control_style())
                        .on_activate(move || {
                            english
                                .i18n
                                .set_locale(langid!("en-US"))
                                .expect("English catalog")
                        }),
                    button("Français")
                        .style(control_style())
                        .on_activate(move || {
                            session
                                .i18n
                                .set_locale(langid!("fr"))
                                .expect("French catalog")
                        }),
                )),
                button(messages.text("back"))
                    .style(control_style())
                    .on_activate(move || back.set(true)),
            )),
    )
}

impl Scene for Preferences {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        if let SceneEvent::Input(batch) = event {
            let mut keys = Shortcuts::default();
            if !batch.is_interrupted() {
                batch.repeat(&mut keys);
            }
            let back = self.back.get();
            self.back.set(false);
            if !ctx.is_transitioning() && (keys.back || back) {
                ctx.pop().transition(cover());
            }
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        self.root.layout(frame, roots);
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        self.root.render(render)
    }
}
```

The Back button writes its flag during Node dispatch. `SceneEvent::Input` then sees that flag and any Escape press. Both request the same pop. During a transition, the example drops these navigation requests to prevent a second stack change before the current cover finishes.

## 5. Publish disk results on the UI thread

Keep the load and save handles on the editor Scene. The worker moves owned paths and items across threads. The Scene writes signals when it collects a result. Button activation and Ctrl+S or Command+S both feed the save request, so the Scene checks whether saving is allowed before starting a worker.

```rust,ignore
struct ChecklistScreen {
    root: ViewRoot,
    editor: Editor,
    session: Rc<Session>,
    path: PathBuf,
    opening: Option<Task<io::Result<Vec<Item>>>>,
    writing: Option<Task<io::Result<Vec<Item>>>>,
    _scope: OwnedScope,
}

impl ChecklistScreen {
    fn new(session: Rc<Session>, path: PathBuf) -> Self {
        let scope = OwnedScope::new();
        let editor = scope.run(Editor::new);
        let root = ViewRoot::themed(&session, || {
            editor_view(Prop::Static(editor), Prop::Static(session.clone()))
        });
        let read_path = path.clone();
        let opening = spawn(async move { spawn_blocking(move || load(&read_path)).await });
        Self {
            root,
            editor,
            session,
            path,
            opening: Some(opening),
            writing: None,
            _scope: scope,
        }
    }

    fn start_save(&mut self) {
        if self.editor.busy() || !self.editor.dirty() {
            return;
        }
        let snapshot = self.editor.items.get_clone();
        let path = self.path.clone();
        self.editor.disk.set(DiskState::Saving);
        self.writing = Some(spawn(async move {
            spawn_blocking(move || {
                save(&path, &snapshot)?;
                Ok(snapshot)
            })
            .await
        }));
    }
}

impl Scene for ChecklistScreen {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        if let SceneEvent::Input(batch) = event {
            let mut keys = Shortcuts::default();
            if !batch.is_interrupted() {
                batch.repeat(&mut keys);
            }
            let preferences = self.editor.preferences_requested.get() || keys.preferences;
            self.editor.preferences_requested.set(false);
            if !ctx.is_transitioning() && preferences {
                ctx.push(Preferences::new(self.session.clone()))
                    .transition(cover());
            }
            if self.editor.save_requested.get() || keys.save {
                self.editor.save_requested.set(false);
                self.start_save();
            }
        }
        Ok(())
    }

    fn update(&mut self, _: &mut SceneContext<'_>) -> CallbackResult<()> {
        if let Some(result) = self.opening.as_mut().and_then(Task::try_take) {
            self.opening = None;
            match result {
                Ok(items) => {
                    self.editor.saved.set(items.clone());
                    self.editor.items.set(items);
                    self.editor.disk.set(DiskState::Ready);
                }
                Err(error) => self
                    .editor
                    .disk
                    .set(DiskState::LoadError(error.to_string())),
            }
        }
        if let Some(result) = self.writing.as_mut().and_then(Task::try_take) {
            self.writing = None;
            match result {
                Ok(snapshot) => {
                    self.editor.saved.set(snapshot);
                    self.editor.disk.set(DiskState::Ready);
                }
                Err(error) => self
                    .editor
                    .disk
                    .set(DiskState::SaveError(error.to_string())),
            }
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        self.root.layout(frame, roots);
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        self.root.render(render)
    }
}

#[macroquad::main("Release checklist")]
async fn main() {
    let session = Rc::new(Session::new().expect("load bundled message files"));
    let mut app = App::with_scene(ChecklistScreen::new(session, PathBuf::from("release.txt")));
    loop {
        app.frame();
        macroquad::window::next_frame().await;
    }
}
```

A worker can finish while Preferences is open, since the foreground executor belongs to the App. The covered editor collects its result when it can update again. A successful save updates `saved` with the exact snapshot the worker wrote. A failed save keeps the edited list available for retry.

## Run the workflow

<figure class="mue-chart">
<div class="mue-motion">

![The progress bar advances, preferences opens, and the editor returns in French with the dark theme and saves the edit][shot-release-workflow]

</div>
<div class="mue-still">

![The dark French editor shows one open item, two completed items, and a saved status][shot-release-saved]

</div>
<figcaption>The filter and edits survive a trip to Preferences. Save writes the full list.</figcaption>
</figure>


1. Start with `cargo run` in the directory that contains `release.txt`.
2. Choose **Open only**. Mark **Smoke-test installer** complete and watch the progress bar advance while the row disappears.
3. Press **F2** or choose **Preferences**. Select **Dark** and **Français**, then press **Escape**. The same editor still has its filter and pending edit, now with the dark palette and French interface.
4. Press **Ctrl+S** or **Command+S**. After the saved status appears, reopen the app and check that the second line still starts with `[x]`.
5. Close the app, add a malformed line, and reopen it. Check the error status and the disabled save control.

[Learn contents][learn-index] · [Reference][guide-reference]

[learn-index]: learn/index.md
[guide-reference]: reference/index.md
[getting-started-guide]: learn/first-step.md
[basics-guide]: learn/basics.md
[style-guide]: learn/style.md
[components-guide]: learn/components.md
[themes-guide]: learn/themes.md
[scenes-guide]: learn/scenes.md
[scene-input-guide]: learn/input.md
[tasks-guide]: learn/tasks.md
[i18n-guide]: learn/i18n.md
[shot-release-dirty]: images/release-dirty.png
[shot-release-workflow]: images/release-workflow.gif
[shot-release-saved]: images/release-saved.png
