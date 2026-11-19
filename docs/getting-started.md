# Getting started

Add `mue` as a Git dependency.

```toml
[dependencies]
mue = { git = "https://github.com/void-cat-0/mue", branch = "main" }
```

For a fixed dependency, replace `branch` with `rev` and specify a commit that contains the `mue` package.

Build a tree, pass it to `App`, and call `frame` once per Macroquad frame.

```rust,no_run
use mue::{App, macroquad, prelude::*};

#[macroquad::main("Hello, mue")]
async fn main() {
    let mut app = App::new(
        center()
            .style(style! { width: 100%; height: 100%; })
            .children(text("A small reactive view.")),
    );
    loop {
        app.frame();
        macroquad::window::next_frame().await;
    }
}
```

`mue::prelude` includes common builders, reactive values, and authoring macros. Browse `mue::components` for the component catalog, `mue::renderer` for node and Scene APIs, and `mue::core` for the reactive runtime and tasks. `mue::macroquad` exposes the same Macroquad fork the renderer uses. If your application only needs one layer, it can depend on that workspace crate directly.

Applications combining gameplay or multiple UI roots can use `App::with_scene(scene)`. The four Scene entry points are `event`, `update`, `layout`, and `render`. Handle lifecycle events in `event`. See the [Scene guide][scenes-guide] for scheduling, input, and error handling.

Enable `features = ["i18n"]` on the `mue` dependency to use `mue::i18n`. An application-owned `I18n` selects the language, namespace translators provide formatting and reactive text, and missing messages fall back to English. See the [Fluent guide][i18n-guide] for binding and scope requirements.

`App` receives input through [mue’s shared input source][input-api], using macroquad’s existing raw-event interface. For Scene applications, App preplans and dispatches Node UI first, then passes the complete original batch to `Scene::event` as `SceneEvent::Input(&batch)`; UI/modal hits and continuing Move/Up are not filtered. Node capture is separate from gameplay policy, which belongs to the Scene; see [the input contract][scene-input-guide]. Host integrations such as [portal](https://github.com/void-cat-0/mue/blob/main/examples/portal/src/lib.rs) can create a `mue::renderer::input::InputSubscriber`, call `read()`, cancel their local pressed state when `InputBatch::is_interrupted()` is true, then `repeat(&mut handler)`. Create/read from the main application future or synchronous callbacks, before `next_frame`, not automatic macroquad coroutines. Unread events expire each frame; dropping a consumer leaves no queue. `App::frame_paused()` discards UI input and cancels active pointers before ticking tasks, so old clicks do not replay on resume.

Both `#[component]` and `#[node]` declarations produce builders. Creating or configuring one does not execute its body; `into_node` consumes it and constructs the node synchronously, once. `App::new` performs that conversion for its root builder. Component bodies run in the node scope and return another builder, so local `provide`, `map`, and `computed` are valid there. The component return chain keeps the same owner and accumulated style. A terminal `#[node]` body receives `NodeContext` positionally as its first parameter and returns `Node` through `ctx.finish(Style::new()...defaults..., move |style| { ... })`; the parameter can use any simple name and its original declared type is checked by Rust. The injected parameter is not a builder input or setter, cannot use `#[default]` / `#[model]`, and defaults cannot refer to its name. `finish` merges defaults, binds transition, opacity, font, and transform, materializes declared children in the parent scope, then runs `body`. Set up shared `provide` / context in the node author body before `ctx.finish(...)`, or in the component body. Callers never pass the injected context to the generated builder constructor. Ordinary parameters named `build` or `style` remain reserved for now, while the injected context may use either name or another simple identifier. See [component and node authoring][components-guide] for a complete example.

`.children(...)` retains a `ChildrenPlan`. The parent installs its context and style bindings during `finish`, then materializes children before running `body`. `body` can work with the materialized children and register hooks through public fields such as `style.on_render.append(...)`; fluent `.on_render(...)` setters remain available. `map_keyed` keeps nodes and local state for retained keys; `show_if` retains its node while hidden. Scope ownership and asynchronous work have separate lifetime boundaries, described in [application tokens and context][themes-guide].

[scenes-guide]: scenes.md
[scene-input-guide]: scenes.md#input
[i18n-guide]: i18n.md
[components-guide]: components.md
[themes-guide]: themes.md
[input-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/input.rs
