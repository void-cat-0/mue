# Components

The component crate owns reusable behavior:

- layout: `stack`, `wrap`, `center`, `container`, `scroll_area`, `pages`, and navigation;
- graphics: `image`, `shape`, and `circle`;
- typography: `text`, `document`, `markdown`, headings, links, lists, code, and quotations.

`flexbox`, `text`, `shape`, and the atlas-backed text implementation are renderer primitives under `mue-macroquad::node`. `mue-macroquad` does not import `mue-components`.

Every builder is styleable. A `#[component]` function prepares and returns another builder without materializing it. Its parameters are written as plain types and receive reactive `Prop<T>` values inside the expanded function. Required parameters form the constructor, optional `#[default]` parameters have setters, and `#[model]` parameters receive writable `Signal<T>` values.

```rust
use std::rc::Rc;
use mue_components::prelude::*;

#[component]
#[theme(primary)]
fn button(caption: Rc<str>, #[default] primary: bool) -> impl Component {
    let _ = primary;
    center().children(text(caption))
}

let view = button("Save").primary(true);
```

`#[theme]` opts into type-based theme identity without options. `#[theme(primary)]` additionally exposes a tracked accessor for that parameter. The generated `Button` type alias shares the function's visibility. The return chain can contribute several targets to one real node; each retains its own options and class set. A component must return a preparable builder or plan, not a prebuilt `Node`. Ordinary child inputs may include prebuilt nodes.

A `#[node]` declaration separates preparation from mounting with `NodePlan`. Preparation can allocate owned resources and returns fallback defaults. Mount consumes the sealed style once, after caller styles and theme rules have been resolved:

```rust
use mue_components::node;
use mue_macroquad::{layout::use_layout, node::NodePlan, style::{Style, Styleable}};

#[node]
fn spacer(#[default(8.)] height: f32) -> NodePlan<impl FnOnce(&mut Style)> {
    let defaults = Style::new().height(height);
    NodePlan::new(defaults, move |style| {
        let _layout = use_layout(style);
    })
}

let view = spacer().height(12.);
```

The declaration has no special style argument. `NodePlan::new(defaults, mount)` is the fallback channel; styling the returned builder inside a component is an explicit contribution and takes priority over themes. Do not append fallback declarations after mounting or call `into_node` on a component's returned builder. Owned data required by mount moves into its closure.

Children remain plans until the parent's mount context exists. Passing builders to `.children(...)` preserves the shared application theme context while giving every child a separate node build. Keyed dynamic children restore the same context when creating later items. Theme changes only update style bindings; they do not rerun preparation or mounting.

For classes, selectors, root theme composition, and explicit hover, see [themes and selectors](themes.md). Hover is also available without a theme through `.with_hovered(|view, hovered| ...)`; configure reactive styles directly from that read-only handle.
