# Layers, overlays, and anchoring

[Reference contents][guide-reference] · [Related reference][components-reference]

- [Paint order and z-index](#paint-order-and-z-index)
- [Layer hosts and teleports](#layer-hosts-and-teleports)
- [Anchored content](#anchored-content)
- [Presence animations](#presence-animations)
- [List transitions](#list-transitions)
- [Back handling](#back-handling)
- [Node handles](#node-handles)

## Paint order and z-index

Children paint in tree order and are hit-tested in reverse, so a later sibling
covers an earlier one. `z_index(i32)` (`z-index: 2;` in `style!`) reorders
siblings for both painting and hit testing: higher values draw on top, and
equal values keep tree order. It does not lift a node above its parent's
siblings; use a teleport for that.

## Layer hosts and teleports

`mue_macroquad::overlay::layer::layer_host(content)` is a full-size block container that
draws `content` and then a fixed stack of overlay layers:

| `Layer` | Typical use |
| --- | --- |
| `Sticky` | Pinned headers |
| `Dropdown` | Selects attached to a trigger |
| `Overlay` | Backdrops and drawers |
| `Modal` | Dialogs |
| `Popover` | Menus and popovers, including ones opened from a dialog |
| `Toast` | Notifications |
| `Tooltip` | Tooltips |

`App::new` wraps its root in a layer host. A custom `Scene` wraps each root
that should accept teleports.

`teleport(layer, || view)` builds `view` where it is declared, so the subtree
keeps that position's `provide`/`inject` context, themes, inherited paint, and
lifetime, but draws it in the nearest host's layer. The placeholder takes no
space. Removing the placeholder (for example through `keyed` or `presence`)
removes the teleported subtree. `show_if` keeps nodes mounted, so a teleport
inside a hidden `show_if` stays visible; wrap the teleported view in
`presence` instead. Without a host, the teleport draws in place.

Each teleported view sits in a full-size surface over the host. The surface is
transparent to the pointer: only its children can be hit, so an empty layer
never blocks the page. Position content inside the surface with `center`,
absolute insets, or `anchored`. Styles on the teleport builder apply to the
surface; `layer_z_index(n)` orders surfaces within one layer, after which
insertion order applies. Keyboard events bubble through the surface and host,
not through the placeholder's ancestors.

```rust
use mue::{prelude::*, renderer::overlay::layer::{Layer, teleport}};

let toast = teleport(Layer::Toast, || {
    flexbox()
        .style(style! { position: absolute; right: 16px; bottom: 16px; padding: 12px; background: #222a3b; })
        .children(text("Saved"))
});
```

`overlay_host(open, content)` in `mue-components` combines these pieces: a
teleport into `Layer::Modal`, a backdrop, a fade presence (the content also
scales and rises with its progress, read through `use_presence`), a `Contain` focus
scope with auto focus, and Escape or backdrop dismissal when `dismissible`.

## Anchored content

`anchored(target)` is an absolutely positioned container that places itself
next to `target`:

| Setter | Default | Meaning |
| --- | --- | --- |
| `placement(Placement)` | `Bottom` | `Top`, `Bottom`, `Left`, `Right`, each with `Start` or `End` alignment variants |
| `offset(px)` | 0 | Gap from the anchor |
| `flip(bool)` | true | Use the opposite side when the preferred side overflows and the opposite overflows less |
| `shift(bool)` | true | Slide along the side to stay inside the boundary |
| `boundary(Rect)` | host rect | Bounds to stay inside; defaults to the nearest layer host, then the window |
| `boundary_padding(px)` | 8 | Minimum distance from the boundary |
| `match_width(bool)` | false | At least as wide as the anchor |

The target is a `NodeHandle`, a `NodeRef`, a `Rect`, or a point (`Vector`), in
window logical pixels. Placement runs in a layout hook and converges within
the same layout preparation, so the content never draws at a stale position.
Children read the result with `use_anchor()`: the final `placement` after
flipping, the `arrow` offset of the anchor's center along the content's edge,
and the `anchor` rectangle.

```rust
use mue::{prelude::*, components::{overlay::anchor::{Placement, anchored}, overlay::layer::{Layer, teleport}}};

let open = signal(false);
let trigger = NodeHandle::new();
let button = button("More").node_handle(trigger).on_activate(move || open.set(!open.get()));
let menu = teleport(Layer::Popover, move || {
    presence(open, move || {
        anchored(trigger)
            .placement(Placement::BottomEnd)
            .offset(6.)
            .children(text("Menu"))
    })
});
```

## Presence animations

`presence(present, || view)` (Vue's `<Transition>`) mounts `view` when `present`
becomes true and animates a progress value from 0 to 1. When `present` becomes
false, progress animates back to 0 and the view is unmounted afterwards, so
exit animations play. `keep_mounted(true)` builds the view once and keeps it
(hidden with `display: none`) to preserve its state.

`PresenceAnimation` applies wrapper effects from progress: `fade()`,
`.scale(from)`, `.slide(dx, dy)`, or `none()`. `transition(Transition)` sets
the curve and duration for both directions (default `ease_out(0.2)`). Content
reads `use_presence()` for `present` and `progress` to drive custom effects.
Animations advance once per App frame; `Transition::Instant` switches
immediately.

`mue_macroquad::motion::transition::animated(target, curve)` is the same primitive for
any interpolatable value: the returned property follows `target` along
`curve`.

## List transitions

`transition_group(list, key, view)` (Vue's `<TransitionGroup>`) renders a
keyed list and animates its changes. New items enter and removed items leave
with the group's `PresenceAnimation` (`.animation(..)`, `.transition(..)`);
a leaving item stays in place until its exit finishes. Items that remain
slide from their old positions to their new ones (FLIP), timed by
`.move_transition(..)`.

```rust
use mue::prelude::*;
use mue::components::{motion::presence::PresenceAnimation, motion::transition_group::transition_group};
use std::rc::Rc;

let owner = OwnedScope::new();
owner.run(|| {
    let items: Signal<Rc<[u32]>> = signal(Rc::from(vec![1, 2, 3]));
    let list = transition_group(items, |id: &u32| *id, |id: &u32| text(Rc::<str>::from(id.to_string())))
        .animation(PresenceAnimation::fade().slide(0., -12.));
});
```

## Back handling

An open `overlay_host` registers a back handler, so the Android back button,
the back gesture, and Escape close the topmost overlay before anything else.
See [touch and mobile][mobile-reference] for the handler stack and edge
swipes.

## Node handles

`NodeHandle::new()` creates a slot that receives a node once it is built, like
a Vue template ref. Bind it with `.node_handle(handle)` on any builder. The
handle reads the node (`get`, `get_untracked`, `signal`), its bounds (`rect`),
and focuses it (`focus`, `focus_within`, `blur`). It is `Copy`, so it can be
moved into several closures.

[guide-reference]: index.md
[components-reference]: components-and-node.md
[mobile-reference]: mobile.md
