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

Both `#[component]` and `#[node]` declarations produce builders. Creating or configuring one does not execute its body; `into_node` consumes it and constructs the node synchronously, once. `App::new` performs that conversion for its root builder. Component bodies run in the node scope and return another builder, so local `provide`, `map`, and `computed` are valid there. The component return chain keeps the same owner and accumulated style. A terminal `#[node]` body receives `NodeContext` positionally as its first parameter and returns `Node` through `ctx.finish(Style::new()...defaults..., move |style| { ... })`; the parameter can use any simple name and its original declared type is checked by Rust. The injected parameter is not a builder input or setter, cannot use `#[default]` / `#[model]`, and defaults cannot refer to its name. `finish` merges defaults, binds transition, opacity, font, and transform, materializes declared children in the parent scope, then runs `body`. Set up shared `provide` / context in the node author body before `ctx.finish(...)`, or in the component body. Callers never pass the injected context to the generated builder constructor. Ordinary parameters named `build` or `style` remain reserved for now, while the injected context may use either name or another simple identifier. See [component and node authoring](components.md) for a complete example.

`.children(...)` retains a `ChildrenPlan`. The parent installs its context and style bindings during `finish`, then materializes children before running `body`. `body` can work with the materialized children and register hooks through public fields such as `style.on_render.append(...)`; fluent `.on_render(...)` setters remain available. `map_keyed` keeps nodes and local state for retained keys; `show_if` retains its node while hidden. Scope ownership and asynchronous work have separate lifetime boundaries, described in [application tokens and context](themes.md).
