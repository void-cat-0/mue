# Controls and the design system

[Reference contents][guide-reference] · [Related reference][conventions-reference]

- [Design system](#design-system)
- [Buttons](#buttons)
- [Form controls](#form-controls)
- [Feedback](#feedback)
- [Data display](#data-display)
- [Navigation and overlays](#navigation-and-overlays)
- [Graphics effects](#graphics-effects)

The controls in `mue-components` follow Chakra UI v2: the same variants,
sizes, and colors, with light and dark values. Every value control works
uncontrolled (it keeps its own state) or controlled (bind a `Signal`), and
reports changes through `on_change`. Controls are focusable, show a focus
ring for keyboard focus, keep a 44 × 44 logical-pixel hit area on touch
screens, and animate their state changes.

```rust
use mue::components::{
    prelude::*,
    system::{ControlSize, Palette},
};
use mue::core::prelude::*;

# let owner = mue::core::scope::OwnedScope::new();
# owner.run(|| {
let volume = signal(40_f32);
let notify = signal(true);
let _settings = vstack().gap(12.).children((
    switch().label("Notifications").checked(notify),
    slider().value(volume).min(0.).max(100.).palette(Palette::Teal),
    hstack().gap(8.).children((
        button("Save").palette(Palette::Teal),
        button("Cancel").variant(ButtonVariant::Ghost),
        badge("beta").palette(Palette::Purple),
    )),
    progress().value(volume).size(ControlSize::Sm),
));
# });
```

## Design system

`mue_components::system` holds what the controls share:

| Item | What it is |
| --- | --- |
| `Palette` | Chakra's color families (gray, red, orange, yellow, green, teal, blue, cyan, purple, pink) with `shade(50..900)` and `alpha(shade, a)`; the accent of a control (`colorScheme`) |
| `fg`, `fg_muted`, `bg`, `bg_panel`, `bg_subtle`, `bg_muted`, `border`, `border_emphasized`, `focus_ring`, `error`, `accent(p)`, `on_accent(p)` | Semantic brushes that switch between light and dark values with `use_color_mode` |
| `is_dark`, `mode(light, dark)`, `pick(light, dark)` | Build your own mode-dependent colors |
| `ControlSize` | `Xs`, `Sm`, `Md`, `Lg`: heights of 1.5, 2, 2.5, and 3 rem, with matching text, padding, and radius |
| `radius::*`, `space(n)`, `font_size::*` | Chakra's radii, spacing (`space(4.)` is 1 rem), and font sizes |

Sizes are in `rem`, so every control follows the root font size: set a
larger `font-size` on the App root (for a zoom setting, or on large screens)
and the controls grow with the text.

## Buttons

| Builder | Notes |
| --- | --- |
| `button(label)` | `variant(ButtonVariant::Solid \| Subtle \| Outline \| Ghost \| Link)`, `palette`, `size`, `left_icon`, `right_icon`, `loading` (a spinner) with optional `loading_text`, `disabled`, `full_width`, `on_activate`. `primary(true)` is solid blue |
| `icon_button(icon)` | A square ghost button showing only the icon |
| `button_group(buttons)` | Applies `variant`, `palette`, and `size` to every button; `attached(true)` joins them |

## Form controls

| Builder | Notes |
| --- | --- |
| `input()`, `textarea()`, `input_group(input)` | See [text input][text-input-reference]; variants `Outline`, `Filled`, `Flushed`, `Unstyled` |
| `checkbox()` | `checked(Signal<bool>)` or `default_checked`, `indeterminate`, `invalid`, `label`, `palette`, `size` |
| `radio_group()` with `radio(value)` children | `value(Signal<Rc<str>>)` (empty means none) or `default_value`, `horizontal`; arrow keys move the selection |
| `switch()` | `checked` or `default_checked`, `label`, `palette`, `size`; the thumb springs across |
| `slider()` | `value(Signal<f32>)` or `default_value`, `min`, `max`, `step`, `on_change_end`; drag, tap the track, or use the arrow keys, Page Up/Down, Home, and End |

## Feedback

| Builder | Notes |
| --- | --- |
| `spinner()` | `size`, `color`, `track`, `thickness`, `speed` |
| `progress()` | `value`, `min`, `max`, `indeterminate`, `has_stripe`, `is_animated`, `palette`, `size` |
| `circular_progress()` | `value` or `indeterminate`, `diameter`, `thickness`, `color`, `track`, `label(node)` |
| `alert(AlertStatus::Info \| Warning \| Success \| Error)` | `variant(AlertVariant::Subtle \| Solid \| LeftAccent \| TopAccent)`, `show_icon`, `on_close`; children `alert_title(..)` and `alert_description(..)` |
| `skeleton()`, `skeleton_circle(rem)`, `skeleton_text()` | Shimmering placeholders; `loaded(prop)` fades the real content in |

## Data display

| Builder | Notes |
| --- | --- |
| `badge(label)` | `variant(BadgeVariant::Solid \| Subtle \| Outline)`, `palette`; uppercase text |
| `tag(label)` | `variant`, `palette`, `size`, `left_icon`; `on_close` adds a close button |
| `avatar()` | `name` (initials on a color derived from the name), `src(texture)`, `size(AvatarSize::Xs..Xxl)`, `badge(Palette)` |

## Navigation and overlays

| Builder | Notes |
| --- | --- |
| `tabs()` with `tab_list()`/`tab(label)` and `tab_panels()`/`tab_panel()` | `index(Signal<usize>)` or `default_index`, `variant(TabsVariant::Line \| Enclosed \| SoftRounded)`, `fitted`; the indicator slides between tabs; arrow keys move between tabs |
| `tooltip(label, child)` | Opens on hover after `open_delay` (0.4 s) and on long press; `placement`, `has_arrow`; teleported so it is never clipped |
| `menu()` with `menu_button(label)` and `menu_list()` | Items `menu_item(label)` (`icon`, `command`, `on_select`), `menu_divider()`, `menu_group(title)`; flips to stay in the window and closes on selection, an outside press, Escape, or back |
| `overlay_host(open, content)` | The modal layer for dialogs; see [layers][layers-reference] |

## Graphics effects

`stripes()` is the moving diagonal "blinds" background of Phira's home
screen: antialiased stripes at 0.66 rad, 48 px apart, half filled, drifting
18 px/s, lifting what is behind them by 7 %. `angle`, `period`, `duty`,
`speed`, and `color` (or `intensity`) take reactive values. Put it behind
content, filling its parent.

[guide-reference]: index.md
[conventions-reference]: component-conventions.md
[text-input-reference]: text-input.md
[layers-reference]: layers-and-overlays.md
