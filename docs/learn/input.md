# Input routing

[Learn contents][learn-index]

Use a control callback for a pointer action such as **Save**. Use the Scene's raw input batch for a shortcut or for input outside the UI tree. Both can send the same request to the application.

## Connect a control and a shortcut

The button writes one flag. In a Scene, replay the raw batch through an `EventHandler` and set the same flag if the user presses Ctrl+S or Command+S. Ignore key repeat so holding the shortcut does not schedule more writes.

```rust
use mue::{macroquad::miniquad::{EventHandler, KeyCode, KeyMods}, prelude::*};
use mue::renderer::input::InputBatch;

struct SaveShortcut(bool);

impl EventHandler for SaveShortcut {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, mods: KeyMods, repeat: bool) {
        self.0 |= !repeat && key == KeyCode::S && (mods.ctrl || mods.logo);
    }
}

fn request_save_from_keys(batch: &InputBatch, requested: Signal<bool>) {
    let mut shortcut = SaveShortcut(false);
    batch.repeat(&mut shortcut);
    if shortcut.0 {
        requested.set(true);
    }
}

fn save_button(requested: Signal<bool>) -> impl Component {
    button("Save").on_activate(move || requested.set(true))
}
```

Call `request_save_from_keys` from the active editor's `SceneEvent::Input` arm. Consume `requested` in `Scene::update`. The update should check whether the editor can save, since the keyboard path does not consult button state. A boolean request also merges a tap and a shortcut in one frame.
## Handle keys on a focused control

Raw Scene input sees every key. When a key belongs to one control, make the control focusable and handle `on_key_down`: the event goes to the focused node and bubbles to its ancestors, and Tab moves focus between focusable nodes. `button` already activates on Enter and Space and shows a focus ring when reached by keyboard.

```rust
use mue::{prelude::*, renderer::input::keyboard::KeyCode};

let volume = signal(5_i32);
let slider = flexbox()
    .focusable(true)
    .focus_ring(2., mue::renderer::paint::color::Color::WHITE, 2.)
    .on_key_down(move |event| match event.key {
        KeyCode::Left => { volume.set(volume.get() - 1); event.stop_propagation(); }
        KeyCode::Right => { volume.set(volume.get() + 1); event.stop_propagation(); }
        _ => {}
    });
```

See [focus, keyboard, and cursors][focus-reference] for Tab order, focus scopes for dialogs, and programmatic focus.

## Track a drag in one node

Use a gesture hook when a drag needs to compete with a tap. This pad shows horizontal displacement from the press. Release it or drag outside the pad to see how a captured pointer differs from a fresh hit test. Place the pointer down on the pad first.

```rust
use mue::{prelude::*, renderer::input::gesture::GesturePhase};

#[component]
fn drag_pad() -> impl Component {
    let distance = signal(0_f32);
    let label = distance.map(|dx| format!("Horizontal drag: {dx:.0} px"));
    center()
        .style(style! { width: 280px; height: 100px; background: #dcebf8; })
        .on_pan(move |event| match event.phase {
            GesturePhase::Start | GesturePhase::Update => distance.set(event.translation.x),
            GesturePhase::End | GesturePhase::Cancel => distance.set(0.),
        })
        .children(text(label).style(style! { font-size: 17px; color: #1a344e; }))
}
```

The recognizer claims after the pointer moves past its threshold. A tap can still win if the user releases without dragging. Once a continuous gesture starts, clear its temporary state on both `End` and `Cancel`. A Scene switch, hidden path, or host interruption can end the interaction without a normal release.

## Follow the two input paths

1. App plans and dispatches Node input against the roots from the **last drawn frame**.
2. The active Scene receives the complete original `SceneEvent::Input(&batch)`, including events that hit Node controls.
3. App runs foreground work, commits pending switches at the safe boundary, and lays out and renders the next frame.

Use `RootInput::None` when a root should draw without accepting Node input. Decide inside the Scene whether raw input should reach gameplay. For a modal, block gameplay separately while letting UI handle the dismiss control. Keep ownership of continuing contacts explicit. When you enter a Scene during a held press, wait for a fresh Down before treating it as a new gameplay press.

When a batch reports `is_interrupted()`, clear your local held state. `frame_paused` drains input and cancels UI gestures while ticking host work. Read the [input module][input-api] if your host needs its own `InputSubscriber`.

### Gate raw input while a HUD panel is open

Use the same state in the panel and in the Scene. The button changes it during Node dispatch. When the Scene receives the raw batch, consult that flag before replaying events. Keep any held-input bookkeeping in your game input handler.

```rust
use mue::{core::prelude::Access, prelude::Signal};
use mue::renderer::{SceneEvent, input::InputBatch};

fn route_game_input(
    event: SceneEvent<'_>,
    panel_open: Signal<bool>,
    mut cancel_held: impl FnMut(),
    mut play: impl FnMut(&InputBatch),
) {
    match event {
        SceneEvent::Pause => cancel_held(),
        SceneEvent::Input(batch) if panel_open.get() || batch.is_interrupted() => {
            cancel_held();
        }
        SceneEvent::Input(batch) => play(batch),
        _ => {}
    }
}
```

Call this from `event` and return `Ok(())`. If the input that closes the panel must also stay out of gameplay, record that the panel was open at the start of dispatch and gate the entire batch. Continue accepting releases only for gameplay contacts you admitted on a fresh down.

See the [Scene input reference][scene-input-reference] for capture, resize, and interruption rules.

**Previous** [Screen navigation][scenes-guide] · **Next** [Asynchronous work][tasks-guide]

[learn-index]: index.md
[scenes-guide]: scenes.md
[tasks-guide]: tasks.md
[scene-input-reference]: ../reference/scene.md#input
[focus-reference]: ../reference/focus-and-keyboard.md
