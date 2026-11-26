# Keyboard and text input

[Learn contents][learn-index]

Focus works with a mouse or a finger. In this chapter you make it work from the
keyboard too: Tab moves between controls, Space starts and pauses the timer,
and a text field lets you name each session. You also see where raw input goes
when a scene needs it, as games do.

## Focus and Tab

Buttons and text fields are **focusable**. Press Tab and focus moves to the
next one; Shift+Tab moves back. The focused control shows a ring and responds
to the keyboard: a button activates with Enter or Space. Make any box focusable
with `.focusable(true)`, and draw its ring with `.focus_ring(width, color,
offset)`. `.tab_index(-1)` keeps a control out of the Tab order while it can
still be clicked.

## Keyboard shortcuts

A shortcut should work wherever focus is inside the timer. Attach a `Hotkey`
directive to the timer's root box; it runs for a key chord pressed while the
box or anything inside it has focus:

```rust
use mue::{components::{Hotkey, WithDirective}, prelude::*, renderer::input::keyboard::KeyCode};

#[component]
fn timer_shortcuts(#[model] running: bool, #[model] remaining: u32) -> impl Component {
    vstack()
        .focusable(true)
        .directive(Hotkey::new(KeyCode::Space, move || running.update(|on| *on = !*on)))
        .directive(Hotkey::command(KeyCode::R, move || remaining.set(25 * 60)))
}
```

`Hotkey::command` means Ctrl on Windows and Linux and Cmd on macOS. For keys
that need more logic, handle them yourself with `.on_key_down(|event| ...)` on a
focusable node; call `event.stop_propagation()` once you have handled a key so
outer nodes do not see it.

## Name the session

Add a text field above the timer. `input()` is a styled single-line field;
`.model(signal)` keeps its text in your signal as you type:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{components::input, prelude::*};
use std::rc::Rc;

#[component]
fn session_name() -> impl Component {
    let name = signal(String::from("Write chapter 7"));
    let preview = name.map(|text| {
        Rc::<str>::from(if text.trim().is_empty() {
            "Untitled session".to_owned()
        } else {
            format!("Now: {}", text.trim())
        })
    });
    vstack().gap(8.).style(style! { width: 300px; }).children((
        input().model(name).placeholder("What are you working on?"),
        text(preview).style(style! { font-size: 14px; color: #8a8178; }),
    ))
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![A focused text field containing "Write chapter 7" with a caption below][shot-guide-input]

</figure>
</div>

The field handles everything you expect from a text box: selection with the
mouse and Shift+arrows, word jumps, copy and paste, undo and redo, and input
methods for Chinese, Japanese, and Korean, with the candidate window next to the
caret. On phones it opens the soft keyboard, and the scroll area around it moves
the field above the keyboard.

Two modifiers change when your signal updates: `.lazy()` writes only when the
field loses focus or you press Enter, and `.trim()` drops surrounding spaces.
For numbers, `.model_parsed(signal)` parses the text into any `FromStr` type.
`.kind(TextInputKind::Number)` and friends ask phones for a matching keyboard,
and `.on_submit(|action| ...)` runs when you press Enter or the keyboard's
action key. The [text input reference][text-input-reference] lists every
option.

## Raw input in a scene

Controls handle pointer and keyboard input through the view tree. A game needs
the raw stream: every key and every touch, in order, whether or not a control
has focus. A scene receives it each frame as `SceneEvent::Input(batch)`;
replay the batch into a handler that implements Miniquad's `EventHandler`:

```rust
use mue::macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use mue::renderer::input::InputBatch;

#[derive(Default)]
struct Keys {
    space: bool,
}

impl EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        self.space |= key == KeyCode::Space && !repeat;
    }
}

fn space_pressed(batch: &InputBatch) -> bool {
    let mut keys = Keys::default();
    batch.repeat(&mut keys);
    keys.space
}
```

The view tree still gets the same input. If a menu is open over the game,
decide in the scene whether the game should also react; the
[scene input reference][scene-input-reference] describes how the two paths
interact.

## What you learned

- Tab moves focus between focusable controls; focused buttons activate with
  Enter and Space.
- `Hotkey` adds shortcuts to a part of the view.
- `input()` is a complete text field with selection, clipboard, undo, and input
  methods; `.model` binds its text.
- Scenes receive the raw input stream for game-style controls.

**Previous** [Screens][scenes-guide] · **Next** [Background work][tasks-guide]

[shot-guide-input]: ../images/guide-input.png

[learn-index]: index.md
[scenes-guide]: scenes.md
[tasks-guide]: tasks.md
[text-input-reference]: ../reference/text-input.md
[scene-input-reference]: ../reference/scene.md#input
