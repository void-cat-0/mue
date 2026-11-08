# Components

The component crate owns reusable behavior:

- layout: `stack`, `wrap`, `center`, `container`, `scroll_area`, `pages`, and navigation;
- graphics: `image`, `shape`, and `circle`;
- typography: `text`, `document`, `markdown`, headings, links, lists, code, and quotations.

`flexbox`, `text`, `shape`, and the atlas-backed text implementation are renderer primitives under `mue_macroquad::node` and are also exposed by the component catalog. `mue-macroquad` does not import `mue-components`.

Both `#[component]` and `#[node]` generate styleable builders. Calling a generated constructor or a setter configures the builder without running the declaration body. Consuming the builder with `IntoNode::into_node` constructs its node synchronously, once; `App::new` performs that conversion for its root. Parameters written as ordinary value types receive `Prop<T>` values inside the body. Required parameters form the public constructor, optional `#[default]` parameters have setters, and `#[model]` parameters receive writable `Signal<T>` values.

```rust
use std::rc::Rc;
use mue_components::prelude::*;

#[component]
fn button(caption: Rc<str>, #[default] primary: bool) -> impl Component {
    let opacity = primary.map(|active| if active { 1. } else { 0.7 });
    center().opacity(opacity).children(text(caption))
}

let view = button("Save").primary(true);
```

A `#[component]` body returns another builder through `impl Component` or a concrete builder type. During `into_node`, the macro runs that body in the resulting node's scope and continues synchronously through its returned builder. Each body runs once, and each omitted parameter's default expression is evaluated once. Keep the return value as a builder so that the caller's styles and ownership reach the same final node. Ordinary child inputs can still include prebuilt nodes.

A parameter affects appearance when component code explicitly uses it in a style binding. Reactive parameter changes update those bindings without rerunning the body. A plain `if primary.get()` in the body samples the current value once. Local `provide`, `map`, `computed`, signals, and effects are valid in component bodies; they use the node scope and are released with that node.

## Author a node

Terminal `#[node]` bodies return a real `Node`; the macro injects `NodeContext` positionally as the first parameter. Its type annotation is preserved and checked by Rust, so `NodeContext`, qualified paths, and type aliases are accepted. The parameter may use any simple name, including `mut`, such as `ctx: NodeContext`; call `ctx.finish(defaults, body)` to combine the configured style with node defaults and initialize its consumers. This injected parameter is excluded from the builder constructor and setters, cannot use `#[default]` or `#[model]`, and its name cannot be referenced by defaults because they are evaluated before injection. The public constructor still returns a configurable builder:

```rust
use mue_components::node;
use mue_macroquad::{
    layout::use_layout,
    node::{IntoNode, Node, NodeContext},
    style::{Style, Styleable},
};

#[node]
fn spacer(ctx: NodeContext, #[default(8.)] height: f32) -> Node {
    ctx.finish(Style::new().height(height), move |style| {
        use_layout(style);
    })
}

let view = spacer().height(12.); // Configure the generated builder.
let node = view.into_node();    // Run the body and construct synchronously, once.
```

The macro supplies the first parameter during construction; callers pass only the declaration's ordinary inputs to `spacer()` and its generated setters. A declaration that returns another builder belongs under `#[component]`. Its returned builder continues with the same final node owner and accumulated style. `NodeContext` is the context type name; its methods are `scope()`, `push_style()`, and `finish()`. Ordinary parameters named `build` or `style` remain reserved for now, but the injected context parameter may use either name or another simple identifier.

`finish` accepts an ordinary `Style` for defaults, including defaults derived from reactive parameters. Use `Style::new()` when there are no defaults. Caller and outer component styles override styles on inner returned builders; node defaults follow those explicit contributions, then framework defaults or inheritance. On the same builder, a later setter replaces an earlier declaration for that property. Children and event hooks follow their construction and ownership behavior rather than this property precedence.

Construction has an explicit order: `finish` merges defaults, binds the parent's transition, opacity, font, and transform, materializes its declared `ChildrenPlan` in the parent scope, calls `body`, and then attaches hooks. `body` initializes layout, paint, and event consumers from the resulting `&mut Style` and operates on already materialized children. Style accessors read or take ordinary fields.

Put shared `provide` / context setup in the node author body before `ctx.finish(...)`, or in the parent component body before returning its builder. This makes the context available when `finish` constructs children. A node that wraps or rearranges children must complete `take_children()` / `set_children(...)` in `body` before calling `use_layout` or `use_container`; replacing the child list after layout has captured it is unsupported.

`Style` stores ordinary fields and exposes its hook fields publicly. Register callbacks with `style.on_render.append(callback)`, `style.on_wheel_event.append(callback)`, and the corresponding public fields. Fluent builder methods such as `.on_render(...)` remain available. The synchronous author body and init closure may use local borrowed data; hooks, effects, event callbacks, and retained child plans keep their own existing `'static` requirements.

Local `provide`, signals, effects, and `map` / `computed` projections created in a node body share its node scope, just as component-local resources do. External inputs still require their original owner to remain alive. Handwritten `BuildNode` implementations use the same `NodeContext::finish(defaults, body)` protocol and receive `IntoNode` from a blanket impl, so only `build` is written by hand. See the [project design record](project-status.md) for the implementation and validation status.

## Children and retained state

`.children(...)` saves a `ChildrenPlan`; configuring the parent does not run child component or node bodies. During synchronous `finish`, the parent installs its node context and binds its style, including font and transform state, then materializes the declared child plan before running `body`. `take_children()` and layout use that already built list. Each real child gets its own owner and inherits ordinary scope context lookup. Place shared `provide` calls in the parent component body before returning its builder, or in the node author body before `ctx.finish(...)`, so they are available to child construction.

Tuples can mix builders, prebuilt nodes, plans, and existing child lists. Vectors and arrays preserve order, and existing owning node containers retain their ownership behavior. A prebuilt `Node` keeps the context in which it was constructed; attaching it to a new parent does not rebuild it. This also applies to an independent node manually constructed with `into_node()` inside a component body: it observes the context available at that call and is not retroactively updated when the component's returned builder later establishes the final parent bindings. Return builders and use ordinary `.children(...)` plans when children should observe that final parent context. Deferred construction occurs synchronously during the parent's `finish`, with no need for an application-written lazy closure. `ChildrenPlan` retains a one-shot owned callback until then, so its stored callback and inputs keep their existing `'static` requirements. Setter argument expressions still evaluate at their ordinary Rust call sites, and explicitly calling `into_children` requests immediate conversion.

`map_keyed` waits until the parent context is installed before starting its list computation. Later new keys restore that context and get a separate owner for resources created in their callback. Keys must be unique. Reordering retained keys preserves nodes, original content, and local state without rerunning the callback; changed data under an unchanged key needs its own reactive binding. Removing a key releases its node and callback-owned resources. Adding that key again creates a new node.

`show_if` also returns a `ChildrenPlan`. Materializing the plan during `finish` constructs its node once, even if the initial condition is false. Toggling visibility changes the visible child list while retaining that node and its local state; hiding it does not dispose it or rerun the body when shown again. Dropping the owning child list releases it.

## Scope, failure, and async boundaries

The node owns resources created in its synchronous construction scope. An unconsumed builder or child plan releases what it already holds when dropped, without executing its declaration bodies. A panic during construction is not recovered: it propagates to the caller, the unfinished node and its owner are never released, layout and gesture registrations made so far stay in place, and the node stack is not unwound. Nothing in the library catches panics, so treat a construction panic as fatal for the process instead of a recoverable error.

`Scope`, `Signal` and `NodeRef` handles are thread-local, non-owning handles; copying them does not extend their owner's lifetime or make them transferable to another thread. `Scope::run(|| async { ... })` installs the scope only while constructing the future. `mue_core::spawn` returns a caller-owned task with no `OwnedScope` or inherited context. Each poll and future destruction runs under `Scope::null` and `Runtime::without_effect`, with no ambient tracker or `current_effect`; the previous context is restored on normal return. Captured handles still depend on their original owners. Resources created by a plain task need an explicit owner or manual disposal.

Use `scope_future(future)` to opt into an independent `OwnedScope::new_inherited()` created at wrapping time. It installs its own scope for polling and inner future destruction, and drops the inner future and owner before returning a completed output, even if the wrapper is retained. Its parent link supports weak context lookup, not keepalive or a snapshot. Only resources constructed under that scope are owned by the wrapper; capturing an existing resource does not transfer ownership. Returning a local reactive handle leaves an invalid handle after completion. The wrapper works standalone for scope ownership or in `spawn(scope_future(...))` for task execution isolation as well. Effect reruns retain their existing behavior and do not restore their creation scope. See [foreground tasks](tasks.md) for a concrete example.

Disposing the creator scope does not cancel the task. Dropping the task handle cancels it; `detach()` transfers its lifetime management to the executor. `Task<T>.await` yields `T` directly. Its handle is `Send` when `T: Send` and may cross threads for await/drop, but its future is polled and destroyed on its original UI thread, where any scoped cleanup also runs.

`mue_core::task::tick(limit)` polls at most `limit` ready tasks without blocking. `mue_macroquad::App::frame` drives up to 32 task polls after timeouts and before layout. One poll is not preempted: synchronous I/O or CPU-heavy code still blocks that frame. A background executor is not implemented. For expected failures, use a `Result<T, E>` task output or `AsyncResource<T, E>`, which owns its state owner and task and exposes `Loading` then `Ready(Result<T, E>)` as a reactive signal. Its state persists until resource drop; its input operation uses plain `spawn` unless explicitly wrapped in `scope_future`. `is_finished()` becomes true after future destruction and any opt-in scoped cleanup. Polling a completed task again or awaiting one forcibly stopped at executor shutdown panics. There is no fallible `Task` API; panics propagate without recovery.

For application-owned tokens, `provide` / `inject`, snapshots versus bindings, scope ownership, asynchronous boundaries, explicit hover, and inherited foreground, see [application tokens and context](themes.md).
