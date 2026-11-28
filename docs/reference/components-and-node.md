# View and node authoring

[Reference contents][guide-reference] · [Related lesson][components-guide]

- [Component catalog](#component-catalog)
- [Builder parameters and construction](#builder-parameters-and-construction)
- [Author a node](#author-a-node)
- [Frame phases](#frame-phases)
- [Modal controls](#modal-controls)

## Component catalog

The component crate owns reusable behavior.

- layout: `stack`, `wrap`, `center`, `container`, `grid`, `simple_grid`, `safe_area`, `scroll_area`, and `pages`;
- application-level push/pop flows use [Scenes][scenes-guide]; the former navigation stack has been removed.
- controls: `button`;
- overlay: `overlay_host`;
- graphics: `image`, `icon`/`icon_named`, and the renderer's `path`, `svg`, `shader_box`, and `shape` nodes;
- layers and effects (re-exported from the renderer): `teleport`, `anchored`, `presence`, and `offscreen`;
- state and behavior: `Controllable`, `provide_part`/`use_part`, composables such as `use_disclosure`, and directives such as `Hotkey`;
- typography: `text`, `document`, `markdown`, headings, links, lists, code, and quotations.

`flexbox`, `text`, `path`, `svg`, `shape`, and the atlas-backed text implementation are renderer primitives under `mue_macroquad::node` and are also exposed by the component catalog. `mue-macroquad` does not import `mue-components`.

## Builder parameters and construction

Both `#[component]` and `#[node]` generate styleable builders. Calling a generated constructor or a setter configures the builder without running the declaration body.

Consuming the builder with `IntoNode::into_node` constructs its node synchronously, once; `App::new` performs that conversion for its root. Parameters written as ordinary value types receive `Prop<T>` values inside the body.

Required parameters form the public constructor, optional `#[default]` parameters have setters, and `#[model]` parameters receive writable `Signal<T>` values. `#[slot]` parameters (`Slot`, `ScopedSlot<T>`) take views or templates, and `#[event]` parameters (`Callback<T>`) take handler closures; see [component conventions][conventions-reference].

```rust
use std::rc::Rc;
use mue::prelude::*;

#[component]
fn primary_button(caption: Rc<str>, #[default] primary: bool) -> impl Component {
    let opacity = primary.map(|active| if active { 1. } else { 0.7 });
    center().opacity(opacity).children(text(caption))
}

let view = primary_button("Save").primary(true);
```

A `#[component]` body returns another builder through `impl Component` or a concrete builder type. During `into_node`, the macro runs that body in the resulting node's scope and continues synchronously through its returned builder.

Each body runs once, and each omitted parameter's default expression is evaluated once. Keep the return value as a builder so that the caller's styles and ownership reach the same final node.

Ordinary child inputs can still include prebuilt nodes.

A parameter affects appearance when component code explicitly uses it in a style binding. Reactive parameter changes update those bindings without rerunning the body.

A plain `if primary.get()` in the body samples the current value once. Local `provide`, `map`, `computed`, signals, and effects are valid in component bodies; they use the node scope and are released with that node.

### Choosing a declaration

A regular Rust function returning `impl Component` works for wrapping a configured builder. Use `#[component]` to expose generated reactive setters. A parameter written as a value type becomes `Prop<T>` in the body. Use `map` or `computed` to derive a style value, and `#[model]` for a writable signal.

## Author a node

Terminal `#[node]` bodies return a real `Node`; the macro injects `NodeContext` positionally as the first parameter. Its type annotation is preserved and checked by Rust, so `NodeContext`, qualified paths, and type aliases are accepted.

The parameter may use any simple name, including `mut`, such as `ctx: NodeContext`; call `ctx.finish(defaults, body)` to combine the configured style with node defaults and initialize its consumers. This injected parameter is excluded from the builder constructor and setters, cannot use `#[default]` or `#[model]`, and its name cannot be referenced by defaults because they are evaluated before injection.

The public constructor still returns a configurable builder:

```rust
use mue::{prelude::*, renderer::layout::use_layout};

#[node]
fn spacer(ctx: NodeContext, #[default(8.)] height: f32) -> Node {
    ctx.finish(Style::new().height(height), move |style| {
        use_layout(style);
    })
}

let view = spacer().height(12.); // Configure the generated builder.
let node = view.into_node();    // Run the body and construct synchronously, once.
```

The macro supplies the first parameter during construction; callers pass only the declaration's ordinary inputs to `spacer()` and its generated setters. A declaration that returns another builder belongs under `#[component]`.

Its returned builder continues with the same final node owner and accumulated style. `NodeContext` is the context type name; its methods are `scope()`, `push_style()`, and `finish()`.

Ordinary parameters named `build` or `style` remain reserved for now, but the injected context parameter may use either name or another simple identifier.

`finish` accepts an ordinary `Style` for defaults, including defaults derived from reactive parameters. Use `Style::new()` when there are no defaults.

Caller and outer component styles override styles on inner returned builders; node defaults follow those explicit contributions, then framework defaults or inheritance. On the same builder, a later setter replaces an earlier declaration for that property.

Children and event hooks follow their construction and ownership behavior rather than this property precedence.

Construction has an explicit order: `finish` merges defaults, binds the parent's transition, opacity, font, and transform, materializes its declared `ChildrenPlan` in the parent scope, calls `body`, and then attaches hooks. `body` initializes layout, paint, and event consumers from the resulting `&mut Style` and operates on already materialized children.

Style accessors read or take ordinary fields.

Put shared `provide` / context setup in the node author body before `ctx.finish(...)`, or in the parent component body before returning its builder. This makes the context available when `finish` constructs children.

A node that wraps or rearranges children must complete `take_children()` / `set_children(...)` in `body` before calling `use_layout` or `use_container`; replacing the child list after layout has captured it is unsupported.

`Style` stores ordinary fields and exposes its hook fields publicly. Register callbacks with `style.on_render.append(callback)`, `style.on_wheel_event.append(callback)`, and the corresponding public fields.

Fluent builder methods such as `.on_render(...)` remain available. The synchronous author body and init closure may use local borrowed data; hooks, effects, event callbacks, and retained child plans keep their own existing `'static` requirements.

Local `provide`, signals, effects, and `map` / `computed` projections created in a node body share its node scope, just as component-local resources do. External inputs still require their original owner to remain alive.

Handwritten `BuildNode` implementations use the same `NodeContext::finish(defaults, body)` protocol and receive `IntoNode` from a blanket impl, so only `build` is written by hand. See the [component API reference][components-api] for the generated builder and node contracts.

## Frame phases

`App::frame` preplans Node input against the displayed frame before lifecycle/input callbacks, dispatches Node pointer/hover/wheel, and notifies the current Scene through `event(SceneEvent::Input(&batch), ctx)` with the complete original raw batch. Node UI handling does not consume or filter that batch.

Transitions, timeouts/tasks, the `frame_with_update` callback and `Scene::update` follow. App prepares all submitted roots, invokes each snapshotted node's `Style::on_update(&dt)` once, stabilizes geometry again, and lets the Scene organize drawing.

Suspended frames do not run node updates or layout; see [Scene input and lifecycle][scene-input-guide].

A measurement installed with `Layout::set_measure_fn` and its reactive subscriptions belong to the measuring node, even when installed from another scope. Replacing it or disposing the node releases the old callback and subscriptions.

It runs in the Taffy solver: read sizing inputs (or `Layout::rect.get_untracked()` for previously published geometry), but do not mutate reactive/layout state or call `with_taffy` from it.

Use `style.on_layout.append(...)` for geometry correction after the node's rect is published and before its children are visited. It may run repeatedly while layout stabilizes; do not advance time there.

Use `style.on_update.append(...)` for frame state such as momentum or elapsed time, and keep `on_render` / `on_after_children` limited to drawing and render caches. App snapshots the active node list for updates: deleted nodes are skipped, and nodes mounted during an update start updating next frame.

A retained `Node` exposes `prepare_layout(available_size: Vector)` and `render()` through its `NodeRef`. Preparation uses logical pixels and a zero root origin, publishes rects/scroll clips without drawing, and may be repeated without advancing time.

Call it outside an enclosing reactive `batch` so geometry observers can settle, and after input routing for the displayed frame. `render()` consumes the prepared geometry, brackets the UI text-atlas pass, and may be repeated without running node updates.

It does not drive tasks, advance transitions, set the camera, or flush texture deletions. `prepare_layout_in(bounds: Rect)` also accepts an explicit logical origin and available size.

App owns frame scheduling; the [Scene API][scenes-guide] can submit multiple roots and interleave their controlled drawing with gameplay.


## Modal controls

`overlay_host(open, content)` renders a full-screen modal layer, teleported
into the `Modal` layer of the nearest layer host, so it covers the window
wherever it is declared. The backdrop fades in and out while the content
also rises and scales up from 0.95 (`content_animation` changes the motion,
`transition` the curve and duration). It captures pointer input while open
and keeps the content mounted while hidden. Focus moves into the content
when it opens, Tab stays inside it, and focus returns when it closes. Content
taps are swallowed; a backdrop tap or Escape only dismisses when
`dismissible(true)` is set. See [layers, overlays, and anchoring][layers-reference]
for the building blocks.

```rust,no_run
use mue::{components::overlay_host, prelude::*};

let open = signal(false);
overlay_host(
    open,
    flexbox()
        .style(style! {
            width: 320px;
            padding: 24px;
            border-radius: 16px;
            flex-direction: column;
            gap: 12px;
            align-items: center;
        })
        .style(style! { box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35); })
        .children((
            text("Paused"),
            button("Resume")
                .primary(true)
                .on_activate(move || open.set(false)),
        )),
)
.dismissible(true)
.on_dismiss(move || open.set(false));
```


[components-api]: https://github.com/void-cat-0/mue/blob/main/mue-components/src/lib.rs

[components-guide]: ../learn/components.md
[scenes-guide]: ../learn/scenes.md
[scene-input-guide]: ../learn/input.md

[guide-reference]: index.md
[layers-reference]: layers-and-overlays.md
[conventions-reference]: component-conventions.md
