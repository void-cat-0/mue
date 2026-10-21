# Getting started

`mue-macroquad` supplies the renderer and runtime. `mue-components` supplies reusable layout, graphics, and typography components. The renderer crate does not depend on the component crate, so applications can use the node primitives without pulling in the higher-level catalog.

```toml
[dependencies]
mue-macroquad = { path = "../mue-macroquad" }
mue-components = { path = "../mue-components" }
```

Build a tree, pass it to `App`, and call `frame` once per Macroquad frame:

```rust
let mut app = App::new(
    stack().size_full().children((
        heading("Hello").style(style! { font-size: 30px; }),
        text("A small reactive view."),
    )),
);
app.frame();
```

Use `mue_components::prelude::*` for the component builders and `mue_core::prelude::*` for signals.
