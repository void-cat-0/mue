# Component conventions

[Reference contents][guide-reference] · [Related reference][components-reference]

The rules every `mue-components` control follows, so controls compose the same
way and application components can follow them too.

- [Naming](#naming)
- [Styles](#styles)
- [Controlled and uncontrolled values](#controlled-and-uncontrolled-values)
- [Compound components](#compound-components)
- [Slots and events](#slots-and-events)
- [Composables and directives](#composables-and-directives)
- [Icons](#icons)
- [Viewport and safe area](#viewport-and-safe-area)

## Naming

- Constructors are `snake_case` functions named after the control and take
  only what the control cannot work without: `button("Save")`, `icon_named("check")`.
- Everything else is a builder method with the same name as the property:
  `.disabled(true)`, `.placement(Placement::Bottom)`.
- Events are `on_<event>` and take a closure: `on_change`, `on_open_change`,
  `on_activate`. Value events receive the new value by reference.
- Boolean setters read as adjectives (`disabled`, `loading`, `dismissible`).
- Compound parts live in a module named after the root, with `root` and part
  constructors (`tabs::root`, `tabs::tab`).

## Styles

Every builder implements `Styleable`, so `.style(style! { .. })`, layout
setters, and hooks work on any control; a test in `mue-components` checks this.
Caller styles take precedence over a control's defaults. Controls keep their
visual defaults in `provide_defaults` (or a `#[node]` default style) instead of
hard-coding them in the body.

## Controlled and uncontrolled values

A control that holds a value declares a `#[model]` parameter with a
`#[default]` and an `#[event] on_change: Callback<T>`:

| Caller writes | Behavior |
| --- | --- |
| nothing | Uncontrolled: the control keeps its own signal, starting from the default |
| `.value(signal)` | Controlled: the caller's signal is the state |
| `.on_change(|value| ..)` | Notified after each change the control makes, either way |

In the body, `Controllable::new(model, on_change)` combines the two; change the
value only through `Controllable::set`, which updates the signal and emits
`on_change` when the value differs. See the `state` module of `mue-components`
for a complete example.

## Compound components

A multi-part control shares state from its root to its parts through context.
The root calls `provide_part(context)` in its body before returning its builder;
each part calls `use_part::<Context>("part", "root")`, which panics with a clear
message when a part is used outside its root. Parts are ordinary components, so
callers can rearrange and style them; a convenience constructor can build the
usual arrangement.

## Slots and events

`#[component]` accepts two more parameter attributes:

| Declaration | Setter | Body receives |
| --- | --- | --- |
| `#[slot] header: Slot` | `.header(view)` | `Slot`: `render()`, `or_else(fallback)`, or pass it to `children` |
| `#[slot] row: ScopedSlot<Item>` | `.row(|item| view)` | `ScopedSlot`: `render(item)`, `render_or(item, fallback)` |
| `#[event] on_pick: Callback<usize>` | `.on_pick(|index| ..)`, callable several times | `Callback`: `emit(&value)` |

Slots and events are always optional. A scoped slot is Vue's scoped slot: the
component decides when and with which data the caller's template is built.

## Composables and directives

Composables are functions called in a component body that return signals and
methods:

| Function | Returns |
| --- | --- |
| `use_disclosure(open)` / `use_toggle(on)` | `Disclosure` with `is_open`, `open`, `close`, `toggle` |
| `use_debounced(signal, seconds)` | A signal that follows the source after it stops changing |
| `use_interval(seconds, tick)` | Runs `tick` periodically while the scope lives |
| `use_clipboard()` | `Clipboard` with `copy` and `paste` |
| `use_breakpoint(&[640., 1024.])` | How many width thresholds the window has reached |
| `use_presence()`, `use_anchor()`, `focus::use_focus()` | State from the nearest presence, anchored, or focusable node |
| `use_media_query("(min-width: 768px)")` | Whether a CSS-style media query matches: widths, heights, orientation, `hover`, `pointer`, `prefers-reduced-motion`, `prefers-color-scheme` |
| `use_color_mode()` | `ColorModeState` with the `mode` preference (light, dark, system), the resolved `scheme`, and `toggle` |
| `use_safe_area()` | Insets for notches and system bars |
| `use_keyboard()` | The soft keyboard's `visible`, `inset`, and `frame` |

Hosts report system preferences with `set_system_color_scheme` and
`set_prefers_reduced_motion`.

A `Directive` adds reusable hooks or styles to any builder with
`.directive(..)` (Vue's custom directives):

| Directive | Behavior |
| --- | --- |
| `Hotkey::command(KeyCode::S, save)` | Runs an action for a key chord on the focus path |
| `ClickOutside::new(close).enabled(open)` | Runs an action when a press lands outside the node |
| `LongPressRepeat::new(step)` | Runs an action on press, then repeatedly while held, speeding up |
| `Haptic::new(HapticStyle::Light)` | Plays a haptic on tap (or `Haptic::on(FeedbackTrigger::Press, ..)`) |
| `Sound::new(SoundCue::Click)` | Plays the application's UI sound on tap, press, or long press |

`mue_macroquad::event::pointer::use_pointer_down_observer` observes every
press in the window; `ClickOutside` is built on it.

## Handles

A handle gives code imperative access to a mounted control, like a Vue
template ref with `defineExpose`. Create it in the component body, pass it to
the builder, and call it from event handlers:

| Handle | Builder | Methods |
| --- | --- | --- |
| `NodeHandle` | `.node_handle(handle)` on any builder | `get`, `rect`, `focus`, `focus_within`, `blur` |
| `TextEditHandle` | `.handle(handle)` on `input`, `textarea`, `text_edit` | `focus`, `select_all`, `set_selection`, `insert`, `set_text`, `undo`, `redo`, `value` |
| `ScrollAreaHandle` | `.handle(handle)` on `scroll_area` | `offset`, `scroll_to`, `scroll_by`, `scroll_to_top`, `scroll_to_end`, `scroll_into_view` |

Text inputs bind a model with Vue's modifiers: `.model(text)`, `.lazy()`,
`.trim()`, and `.model_parsed(number)` ([text input][text-input-reference]).

## Icons

`icon(source)` draws an `SvgDocument`, a `PathData` in a view box (filled or
stroked), or a texture region. Icons are `1em` squares by default, so they
follow the font size, and they paint with the inherited foreground `color`
(`currentColor` in SVG). mue ships no icon artwork: `register_icon(name, source)`
adds your own or an icon library's SVGs (check the library's license) for
`icon_named(name)`; an unregistered name draws nothing.

```rust
use mue::components::{IconSource, hstack, icon_named, register_icon, text};
use mue::prelude::*;

register_icon(
    "check",
    IconSource::stroked(PathData::parse("M5 12.5l4.5 4.5L19 7.5").unwrap(), 24., 2.),
);
let saved = hstack().children((
    icon_named("check").style(style! { color: #7fd1a8; }),
    text("Saved"),
));
```

## Viewport and safe area

`mue_macroquad::viewport` exposes the window as signals: `use_viewport()`
(logical size, DPI scale, orientation), `breakpoint(thresholds)`, and
`min_width(px)`. `safe_area()` returns insets for notches and system bars; the
platform layer publishes them with `set_safe_area(edges)`, and they are zero on
desktop. The `safe_area` component pads its content by those insets.

[guide-reference]: index.md
[components-reference]: components-and-node.md
[text-input-reference]: text-input.md
