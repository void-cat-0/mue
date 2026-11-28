# Text input and IME

[Reference contents][guide-reference] · [Related reference][focus-reference]

- [Controls](#controls)
- [Model modifiers](#model-modifiers)
- [The text_edit node](#the-text_edit-node)
- [Editing behavior](#editing-behavior)
- [Formatters and undo](#formatters-and-undo)
- [Input methods and soft keyboards](#input-methods-and-soft-keyboards)
- [Platform support](#platform-support)

## Controls

`mue-components` provides three text controls built on the `text_edit` node:

| Builder | Purpose |
| --- | --- |
| `input()` | A single-line field |
| `textarea()` | A multiline field; `.rows(n)` sets the minimum height in lines |
| `input_group(input)` | A field with `.left_addon` / `.right_addon` (attached boxes) or `.left_element` / `.right_element` (inside the border) |

Fields take `.placeholder`, `.size(InputSize::Sm | Md | Lg)`,
`.variant(InputVariant::Outline | Filled | Flushed)`, `.invalid(prop)`,
`.disabled(prop)`, `.read_only(prop)`, `.max_length(n)`,
`.select_all_on_focus(bool)`, and `.formatter(..)`. The keyboard hints
`.kind(TextInputKind)`, `.action(TextInputAction)`, `.capitalization(..)`,
and `.autocorrect(bool)` choose the soft keyboard layout and its action
key. `.kind(TextInputKind::Password)` masks the text and disables
suggestions and composition. `.on_change(|text| ..)` runs with the new text
after each edit, and `.on_submit(|action| ..)` runs on Enter or the soft
keyboard's action key; it receives the `TextInputAction` (Done, Go, Search,
Send, ...), and the text is in your model.

```rust
use mue::components::{input, input_group};
use mue::prelude::*;

let query = signal(String::new());
let search = input_group(input().model(query).placeholder("Search"))
    .left_addon(text("https://"))
    .right_element(button("Go"));
```

## Model modifiers

`.model(signal)` binds a `Signal<String>` in both directions, like Vue's
`v-model`. The Vue modifiers have builder equivalents:

| Vue | mue |
| --- | --- |
| `v-model="name"` | `.model(name)` |
| `v-model.lazy` | `.model(name).lazy()`: write on blur or Enter instead of each edit |
| `v-model.trim` | `.model(name).trim()`: write the text without surrounding whitespace |
| `v-model.number` | `.model_parsed(age)`: any `T: FromStr + Display`; text that does not parse leaves the signal unchanged |

## The text_edit node

`mue_macroquad::node::text_edit()` is the editable text node underneath the
controls. Use it directly for fields with custom chrome. It takes
`.value(prop)` (a controlled value), `.multiline(bool)`, `.min_lines(n)`,
`.mask('•')`, the colors `.caret_color`, `.selection_color`, and
`.placeholder_color`, and the same hints and events as the controls, plus
`.on_commit(|text| ..)`, which runs with the text when the field loses focus.

`TextEditHandle` gives imperative access, like a Vue component ref exposed
with `defineExpose`: bind it with `.handle(handle)`, then call `focus`,
`blur`, `select_all`, `set_selection`, `insert`, `set_text`, `undo`, `redo`,
`value`, or `editing_value` (text, selection, and composing range).

## Editing behavior

- **Keyboard:** arrows, Home/End, Page Up/Down, and word movement
  (Ctrl, or Option on macOS), with Shift to extend the selection;
  Backspace/Delete by grapheme or word; Ctrl/Cmd+A, C, X, V, Z, Y and
  Shift+Ctrl/Cmd+Z. Up and Down keep the caret's horizontal position across
  lines of different lengths.
- **Pointer:** click places the caret, drag selects, double-click selects a
  word, triple-click a paragraph. Shift+click extends the selection.
- **Touch:** a tap places the caret; a long press or a double tap selects a
  word and, on Android, opens the system's floating text toolbar (Cut, Copy,
  Paste, Select all) over the selection. The next touch or edit closes it. A
  drag that starts as a scroll does not move the caret.
- **Display:** the selection, the composition underline, and a blinking
  caret are drawn with the text. A single-line field scrolls horizontally to
  keep the caret visible; a multiline field grows with its content and
  scroll areas keep the caret visible.

Offsets in `TextEditingValue` are UTF-8 byte offsets on character
boundaries. Movement and deletion use grapheme clusters, so emoji sequences
and combining marks are edited as one character.

## Formatters and undo

A `TextFormatter` rewrites each proposed edit before it is applied, like
Flutter's input formatters. Built-in formatters: `MaxLength(n)` (in
graphemes, never splitting a composition), `SingleLine`, `digits_only()`,
`AllowChars(|c| ..)`, and `DecimalNumber { signed }`.
Formatters run in order; implement `TextFormatter` for custom ones.

`UndoHistory` merges edits of the same kind (typing, deleting) made within
`coalesce_seconds` (one second by default) into one step; pastes and cuts
are separate steps. A finished IME composition is undone as one
step.

## Input methods and soft keyboards

The design follows Flutter's text input protocol. The focused field
attaches to the platform with `text_input::attach(config)` and receives a
`TextInputConnection`:

- **Desktop** input methods send composition (preedit) text and commits;
  the field draws the composition inline with an underline.
- **Android, iOS, and the web** keep a mirror of the field (an `Editable`,
  a hidden text view, a hidden `<textarea>`). The platform sends the whole
  new state after each input-method edit, and the field sends its value
  back after local edits. Offsets cross that boundary in UTF-16.
- The field reports its caret rectangle (`text_input::caret_rect()`) so
  candidate windows appear next to the caret.
- `TextInputConnection::show_text_menu(rect)` opens the floating text
  toolbar over a selection; its actions edit the mirror and arrive as
  ordinary state changes. Custom editors call it after a touch selection.
- Each session has an id, so edits the platform reports late for a field
  that lost focus never reach the next field.
- The platform reports the soft keyboard's frame. The window keeps its size
  while the keyboard is open; the keyboard covers its bottom instead:
  `keyboard_frame()`, `keyboard_inset()`, and `keyboard_visible()` are
  signals, and `use_keyboard()` in `mue-components` bundles them.
  `scroll_area` scrolls the focused field above the keyboard, and Tab focus
  reveals fields below the fold.
- `on_text_input_action(|action| ..)` observes action keys (Done, Next,
  Search, Send) that the soft keyboard sends.

## Platform support

The input bridge lives in the `text-input` branches of the miniquad and
macroquad forks, which the workspace patches in by path.

| Platform | Mechanism | Status |
| --- | --- | --- |
| Windows | IMM32: composition string, candidate window placed with `CFS_EXCLUDE` at the caret, surrogate pairs joined | Tested with Microsoft Pinyin |
| macOS | `NSTextInputClient` (marked text, `firstRectForCharacterRange`) | Compiles; not run |
| Linux X11 | XIM with preedit callbacks and spot location | Compiles; not run |
| Linux Wayland | `text-input-unstable-v3` (preedit, commit, cursor rectangle) | Compiles; not run |
| Android | Hidden `EditText` with an `InputConnection`; keyboard insets from `WindowInsets` | Java and Rust compile; not run on a device |
| iOS | Hidden `UITextView` mirror; keyboard frame notifications | Compiles; not run |
| Web | Hidden `<input>` / `<textarea>` mirror with composition events | Compiles; not run |

OpenHarmony is not supported. Platforms without a bridge still receive
typed characters through the regular key and character events.

[guide-reference]: index.md
[focus-reference]: focus-and-keyboard.md
