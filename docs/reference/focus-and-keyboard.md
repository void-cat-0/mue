# Focus, keyboard, and cursors

[Reference contents][guide-reference] · [Related lesson][input-guide]

- [Focusable nodes](#focusable-nodes)
- [Tab order and focus scopes](#tab-order-and-focus-scopes)
- [Keyboard events](#keyboard-events)
- [Showing focus](#showing-focus)
- [Programmatic focus](#programmatic-focus)
- [Cursors](#cursors)

## Focusable nodes

One node per UI thread has keyboard focus. A node takes part when it declares
`focusable(true)`. Pressing a focusable node focuses the deepest focusable
node under the pointer; pressing anywhere else removes focus. A focusable node
counts as interactive for hit testing even without pointer handlers.

| Style setter | Meaning |
| --- | --- |
| `focusable(bool)` | Accept focus from pointer, Tab, and code. Reactive. |
| `tab_index(i32)` | Tab order. Positive values come first in ascending order, then `0` in tree order. Negative values are skipped by Tab but can still be focused by pointer or code. |
| `focus_scope(FocusScope::Contain)` | Keep Tab inside this subtree while focus is there; restore the previous focus when the subtree is removed or hidden. |
| `auto_focus(bool)` | Focus the node, or its first focusable descendant, each time it becomes displayed with a non-empty area. |
| `on_focus`, `on_blur` | Receive a `FocusEvent` with the `FocusReason` and whether focus is visible. |

`button` from `mue-components` is focusable, shows a focus ring for keyboard
focus, and activates on Enter or Space.

## Tab order and focus scopes

Tab moves focus forward and Shift+Tab backward, wrapping around, after the key
event has bubbled without `stop_propagation`. Only nodes in the displayed roots
with a non-empty layout rect are tabbable, so `display: none` and hidden
`show_if` children are skipped. Roots are visited in drawing order.

When focus is inside a `Contain` scope, Tab stays inside the nearest one. With
nothing focused, Tab enters the last `Contain` scope in paint order, so an open
dialog receives focus first. When focus enters a scope from outside, the scope
remembers the previous node. When the scope is removed or hidden (`display:
none`, a hidden `show_if`, a closed `presence`), that node is focused again at
the next frame boundary; with no such node, focus is cleared. Keys therefore
never reach a hidden subtree.

```rust
use mue::prelude::*;

let open = signal(true);
let dialog = flexbox()
    .focus_scope(FocusScope::Contain)
    .auto_focus(true)
    .children((button("Cancel"), button("Delete")));
let page = flexbox().children((button("Open"), dialog.show_if(open)));
```

## Keyboard events

Key and character events go to the focused node first, then bubble through its
ancestors until a handler calls `stop_propagation()`. With no focused node they
go to the displayed roots, topmost first, so a handler on a root acts as a
global shortcut.

| Hook | Event |
| --- | --- |
| `on_key_down` | `KeyEvent { key, modifiers, repeat }`; `command()` is Ctrl, or Cmd on macOS |
| `on_key_up` | `KeyEvent` with `repeat == false` |
| `on_char` | `CharEvent { character, modifiers, repeat }` for text input |

`KeyCode` and `KeyMods` are re-exported from `mue_macroquad::input::keyboard`.
Node keyboard routing is independent of the Scene's raw input: the active Scene
still receives every key in `SceneEvent::Input`.

```rust
use mue::{prelude::*, renderer::input::keyboard::KeyCode};

let saved = signal(false);
let editor = flexbox().focusable(true).on_key_down(move |event| {
    if event.command() && event.key == KeyCode::S && !event.repeat {
        event.stop_propagation();
        saved.set(true);
    }
});
```

## Showing focus

Focus is *visible* when it last moved by keyboard (Tab, or code while focus was
already visible), like CSS `:focus-visible`. Pointer focus is not visible.

- `StyleableExt::with_focus(|style, focus| ...)` gives live `focus.focused` and
  `focus.visible` signals for styling, in the same way `with_hovered` does.
- `StyleableExt::focus_ring(width, brush, offset)` shows an outline only while
  focus is visible.
- `mue_macroquad::input::focus::use_focus()` returns the same signals inside a
  `#[node]` body.

```rust
use mue::{prelude::*, renderer::paint::color::Color};

let field = flexbox()
    .focusable(true)
    .focus_ring(2., Color::from_rgba(120, 170, 255, 255), 2.);
let tile = flexbox().focusable(true).with_focus(|style, focus| {
    style.opacity(focus.focused.map(|focused| if focused { 1. } else { 0.7 }))
});
```

## Programmatic focus

| Call | Effect |
| --- | --- |
| `node_ref.focus()` | Focus a focusable node; returns false otherwise. |
| `node_ref.blur()` | Remove focus if the node has it. |
| `node_ref.is_focused()` | Tracked read. |
| `focus::focused()` | The focused `NodeRef`, if alive. Tracked. |
| `focus::focus_visible()` | `ReadSignal<bool>`. |
| `focus::move_focus(forward)` | Tab or Shift+Tab without a key event. |
| `focus::focus_within(node)` | Focus a node or its first tabbable descendant. |
| `focus::blur()` | Remove focus. |

## Cursors

`cursor(Cursor::Pointer)` (or `cursor: pointer;` in `style!`) sets the mouse
cursor while the node is hovered. The deepest hovered node with a cursor other
than `Cursor::Auto` wins; otherwise the default arrow is shown. Available
shapes are `default`, `pointer`, `text`, `crosshair`, `move`, `not-allowed`,
`wait`, `help`, `ew-resize`, `ns-resize`, `nesw-resize`, and `nwse-resize`.
A node with a cursor takes part in hover hit testing. Touch input ignores
cursors.

[guide-reference]: index.md
[input-guide]: ../learn/input.md
