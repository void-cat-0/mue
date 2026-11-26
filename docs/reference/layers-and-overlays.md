# Layers, overlays, and anchoring

[Reference contents][guide-reference] · [Related reference][components-reference]

- [Paint order and z-index](#paint-order-and-z-index)
- [Layer hosts and teleports](#layer-hosts-and-teleports)
- [Anchored content](#anchored-content)
- [Node handles](#node-handles)

## Paint order and z-index

Children paint in tree order and are hit-tested in reverse, so a later sibling
covers an earlier one. `z_index(i32)` (`z-index: 2;` in `style!`) reorders
siblings for both painting and hit testing: higher values draw on top, and
equal values keep tree order. It does not lift a node above its parent's
siblings; use a teleport for that.

## Layer hosts and teleports

`mue_macroquad::layer::layer_host(content)` is a full-size block container that
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
space. Removing the placeholder (for example through `keyed`)
removes the teleported subtree. `show_if` keeps nodes mounted, so a teleport
inside a hidden `show_if` stays visible; mount it conditionally with `keyed`
instead. Without a host, the teleport draws in place.

Each teleported view sits in a full-size surface over the host. The surface is
transparent to the pointer: only its children can be hit, so an empty layer
never blocks the page. Position content inside the surface with `center`,
absolute insets, or `anchored`. Styles on the teleport builder apply to the
surface; `layer_z_index(n)` orders surfaces within one layer, after which
insertion order applies. Keyboard events bubble through the surface and host,
not through the placeholder's ancestors.

```rust
use mue::{prelude::*, renderer::layer::{Layer, teleport}};

let toast = teleport(Layer::Toast, || {
    flexbox()
        .style(style! { position: absolute; right: 16px; bottom: 16px; padding: 12px; background: #222a3b; })
        .children(text("Saved"))
});
```

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
use mue::{prelude::*, renderer::{anchor::{Placement, anchored}, layer::{Layer, teleport}}};

let trigger = NodeHandle::new();
let button = button("More").node_handle(trigger);
let menu = teleport(Layer::Popover, move || {
    anchored(trigger)
        .placement(Placement::BottomEnd)
        .offset(6.)
        .children(text("Menu"))
});
```

## Node handles

`NodeHandle::new()` creates a slot that receives a node once it is built, like
a Vue template ref. Bind it with `.node_handle(handle)` on any builder. The
handle reads the node (`get`, `get_untracked`, `signal`), its bounds (`rect`),
and focuses it (`focus`, `focus_within`, `blur`). It is `Copy`, so it can be
moved into several closures.

[guide-reference]: index.md
[components-reference]: components-and-node.md
