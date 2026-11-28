# Touch and mobile

[Reference contents][guide-reference] · [Related reference][text-input-reference]

- [Touch targets](#touch-targets)
- [Hover on touch screens](#hover-on-touch-screens)
- [Back navigation](#back-navigation)
- [Scrolling patterns](#scrolling-patterns)
- [Swipe actions](#swipe-actions)
- [Soft keyboard](#soft-keyboard)
- [Haptics and sounds](#haptics-and-sounds)
- [Limitations](#limitations)

## Touch targets

`hit-slop` and `min-hit-size` grow a node's pointer hit area without changing
its layout or paint ([style reference][style-reference]). `button` uses
`min-hit-size: 44px`, so compact buttons stay easy to touch. Nodes hidden with
`display: none` are neither drawn nor hit-tested.

## Hover on touch screens

A finger has no hover. After a touch, mouse events that the platform
emulates from it are ignored for a short grace period, so a tapped control
does not stay in its hover style. `mue_macroquad::input::hover::hover_capable()`
is a signal that is true while the most recent pointer can hover (a mouse or
pen); `use_media_query("(hover: none)")` reads the same value.

## Back navigation

`mue_macroquad::input::back` routes back presses to a stack of handlers, like
Android's `OnBackPressedDispatcher`. A back press comes from the Android back
button or back gesture, from Escape or the Back key when no focused control
handles it, or from `dispatch_back()`.

```rust
use mue::prelude::*;
use mue::renderer::input::back::use_back_handler;

let owner = OwnedScope::new();
owner.run(|| {
    let editing = signal(true);
    // While editing, back leaves edit mode instead of the page.
    use_back_handler(editing, move || editing.set(false));
});
```

`use_back_handler(enabled, handler)` registers a handler for the lifetime
of the current scope. The handler enabled most recently wins, so an overlay
that opened last closes first; `overlay_host` registers one while it is
open. With no enabled handler, the fallback runs: it closes the app on
Android and does nothing elsewhere. Replace it with `set_back_fallback`, and
check `can_go_back()` to show a back button only when one would act.

`back_swipe()` turns a rightward swipe from the left edge into a back press,
as on iOS. Wrap a page in it; `.edge(px)` (default 24) and `.distance(px)`
(default 80) tune the gesture. It claims the touch only once the motion is
clearly a back swipe, so taps and vertical scrolling near the edge still
work.

## Scrolling patterns

`scroll_area` supports:

- `.on_refresh(refreshing, action)`: pull to refresh. Pulling past the
  threshold runs `action` and sets `refreshing`; the indicator stays until the
  application sets `refreshing` back to false.
- `.on_reach_end(threshold, action)`: runs `action` once when the end comes
  within `threshold` pixels, then again after the content grows (load more).
- `.handle(ScrollAreaHandle)`: scroll from code with `scroll_to`,
  `scroll_by`, `scroll_to_top`, `scroll_to_end`, and `scroll_into_view`,
  each optionally animated.
- Focus reveal: when focus moves to a node inside the area (by Tab or a
  tapped field), the innermost scroll area scrolls it into view, above the
  soft keyboard.

`transition_group(list, key, view)` (Vue's `<TransitionGroup>`) animates
keyed list changes: added items enter and removed items leave with a
`PresenceAnimation`, and the remaining items slide to their new positions.

## Swipe actions

`swipe_actions(content)` reveals actions behind a list row, as in mail apps:

```rust
use mue::components::{swipe_actions, text};
use mue::prelude::*;

let row = swipe_actions(text("Message from Ada"))
    .leading(button("Archive"))
    .trailing(button("Delete"))
    .on_full_swipe_trailing(|| println!("deleted"));
```

Releasing past half the actions' width keeps them open; swiping past 60%
of the row runs the full-swipe action. Tapping the row or opening another
row closes it.

## Soft keyboard

On Android, iOS, and the web the platform reports the soft keyboard's
frame. The window does not shrink when the keyboard opens, so layouts and
anything sized from the window stay put; the keyboard covers the bottom of
the window. `use_keyboard()` returns its `visible`, `inset`, and `frame`
signals, and scroll areas keep the focused field visible above it. `use_safe_area()`
returns the insets for notches and system bars. See [text input and
IME][text-input-reference] for input methods.

## Haptics and sounds

`mue_macroquad::input::feedback` plays interaction feedback:

- `haptic(HapticStyle)` uses the platform by default: Android
  `performHapticFeedback`, iOS feedback generators, and `navigator.vibrate`
  on the web. It does nothing on desktop.
- `sound(SoundCue)` plays a short built-in sample with the `sound` feature
  (of `mue` or `mue-macroquad`), through [sasa](https://github.com/Mivik/sasa):
  cpal on Windows, macOS, Linux, and iOS, oboe on Android. The output opens
  on the first sound. `set_sound_sample(cue, bytes)` replaces a sample with
  the application's own file (WAV, OGG, FLAC, MP3, AAC) and
  `set_sound_volume` scales them all. On Android, oboe needs the shared C++
  runtime: package `libc++_shared.so` from the NDK with the app. The web
  and builds without the feature stay silent unless the application
  installs a handler with `set_sound_handler`; a handler can still call
  `play_sound` for the built-in sample.
- `set_haptic_handler` replaces haptics, for example to honor a settings
  switch.

The `Haptic` and `Sound` directives play feedback when a node is pressed,
tapped, or long-pressed ([directives][conventions-reference]).

## Limitations

- Only Windows has been tested interactively. The other platform bridges
  compile for their targets but have not run on devices.
- An edge swipe that starts over an open overlay goes to the overlay, not to
  the `back_swipe` beneath it, because overlays are teleported above the
  page. Back presses from the system or Escape still reach the overlay's
  handler.
- OpenHarmony is not supported.

[guide-reference]: index.md
[style-reference]: style.md
[text-input-reference]: text-input.md
[conventions-reference]: component-conventions.md#composables-and-directives
