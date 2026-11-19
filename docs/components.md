# Components

The component crate owns reusable behavior:

- layout: `stack`, `wrap`, `center`, `container`, `scroll_area`, and `pages`;
- application-level push/pop flows use [Scenes][scenes-guide]; the former navigation stack has been removed.
- controls: `button`;
- overlay: `overlay_host`;
- graphics: `image`, `shape`, and `circle`;
- typography: `text`, `document`, `markdown`, headings, links, lists, code, and quotations.

`flexbox`, `text`, `shape`, and the atlas-backed text implementation are renderer primitives under `mue_macroquad::node` and are also exposed by the component catalog. `mue-macroquad` does not import `mue-components`.

Both `#[component]` and `#[node]` generate styleable builders. Calling a generated constructor or a setter configures the builder without running the declaration body. Consuming the builder with `IntoNode::into_node` constructs its node synchronously, once; `App::new` performs that conversion for its root. Parameters written as ordinary value types receive `Prop<T>` values inside the body. Required parameters form the public constructor, optional `#[default]` parameters have setters, and `#[model]` parameters receive writable `Signal<T>` values.

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

A `#[component]` body returns another builder through `impl Component` or a concrete builder type. During `into_node`, the macro runs that body in the resulting node's scope and continues synchronously through its returned builder. Each body runs once, and each omitted parameter's default expression is evaluated once. Keep the return value as a builder so that the caller's styles and ownership reach the same final node. Ordinary child inputs can still include prebuilt nodes.

A parameter affects appearance when component code explicitly uses it in a style binding. Reactive parameter changes update those bindings without rerunning the body. A plain `if primary.get()` in the body samples the current value once. Local `provide`, `map`, `computed`, signals, and effects are valid in component bodies; they use the node scope and are released with that node.

## Author a node

Terminal `#[node]` bodies return a real `Node`; the macro injects `NodeContext` positionally as the first parameter. Its type annotation is preserved and checked by Rust, so `NodeContext`, qualified paths, and type aliases are accepted. The parameter may use any simple name, including `mut`, such as `ctx: NodeContext`; call `ctx.finish(defaults, body)` to combine the configured style with node defaults and initialize its consumers. This injected parameter is excluded from the builder constructor and setters, cannot use `#[default]` or `#[model]`, and its name cannot be referenced by defaults because they are evaluated before injection. The public constructor still returns a configurable builder:

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

The macro supplies the first parameter during construction; callers pass only the declaration's ordinary inputs to `spacer()` and its generated setters. A declaration that returns another builder belongs under `#[component]`. Its returned builder continues with the same final node owner and accumulated style. `NodeContext` is the context type name; its methods are `scope()`, `push_style()`, and `finish()`. Ordinary parameters named `build` or `style` remain reserved for now, but the injected context parameter may use either name or another simple identifier.

`finish` accepts an ordinary `Style` for defaults, including defaults derived from reactive parameters. Use `Style::new()` when there are no defaults. Caller and outer component styles override styles on inner returned builders; node defaults follow those explicit contributions, then framework defaults or inheritance. On the same builder, a later setter replaces an earlier declaration for that property. Children and event hooks follow their construction and ownership behavior rather than this property precedence.

Construction has an explicit order: `finish` merges defaults, binds the parent's transition, opacity, font, and transform, materializes its declared `ChildrenPlan` in the parent scope, calls `body`, and then attaches hooks. `body` initializes layout, paint, and event consumers from the resulting `&mut Style` and operates on already materialized children. Style accessors read or take ordinary fields.

Put shared `provide` / context setup in the node author body before `ctx.finish(...)`, or in the parent component body before returning its builder. This makes the context available when `finish` constructs children. A node that wraps or rearranges children must complete `take_children()` / `set_children(...)` in `body` before calling `use_layout` or `use_container`; replacing the child list after layout has captured it is unsupported.

`Style` stores ordinary fields and exposes its hook fields publicly. Register callbacks with `style.on_render.append(callback)`, `style.on_wheel_event.append(callback)`, and the corresponding public fields. Fluent builder methods such as `.on_render(...)` remain available. The synchronous author body and init closure may use local borrowed data; hooks, effects, event callbacks, and retained child plans keep their own existing `'static` requirements.

Local `provide`, signals, effects, and `map` / `computed` projections created in a node body share its node scope, just as component-local resources do. External inputs still require their original owner to remain alive. Handwritten `BuildNode` implementations use the same `NodeContext::finish(defaults, body)` protocol and receive `IntoNode` from a blanket impl, so only `build` is written by hand. See the [component API reference][components-api] for the generated builder and node contracts.

## Frame phases

`App::frame` preplans Node input against the displayed frame before lifecycle/input callbacks, dispatches Node pointer/hover/wheel, and notifies the current Scene through `event(SceneEvent::Input(&batch), ctx)` with the complete original raw batch. Node UI handling does not consume or filter that batch. Transitions, timeouts/tasks, the `frame_with_update` callback and `Scene::update` follow. App prepares all submitted roots, invokes each snapshotted node's `Style::on_update(&dt)` once, stabilizes geometry again, and lets the Scene organize drawing. Suspended frames do not run node updates or layout; see [Scene input and lifecycle][scene-input-guide].

A measurement installed with `Layout::set_measure_fn` and its reactive subscriptions belong to the measuring node, even when installed from another scope. Replacing it or disposing the node releases the old callback and subscriptions. It runs in the Taffy solver: read sizing inputs (or `Layout::rect.get_untracked()` for previously published geometry), but do not mutate reactive/layout state or call `with_taffy` from it.

Use `style.on_layout.append(...)` for geometry correction after the node's rect is published and before its children are visited. It may run repeatedly while layout stabilizes; do not advance time there. Use `style.on_update.append(...)` for frame state such as momentum or elapsed time, and keep `on_render` / `on_after_children` limited to drawing and render caches. App snapshots the active node list for updates: deleted nodes are skipped, and nodes mounted during an update start updating next frame.

A retained `Node` exposes `prepare_layout(available_size: Vector)` and `render()` through its `NodeRef`. Preparation uses logical pixels and a zero root origin, publishes rects/scroll clips without drawing, and may be repeated without advancing time. Call it outside an enclosing reactive `batch` so geometry observers can settle, and after input routing for the displayed frame. `render()` consumes the prepared geometry, brackets the UI text-atlas pass, and may be repeated without running node updates. It does not drive tasks, advance transitions, set the camera, or flush texture deletions. `prepare_layout_in(bounds: Rect)` also accepts an explicit logical origin and available size. App owns frame scheduling; the [Scene API][scenes-guide] can submit multiple roots and interleave their controlled drawing with gameplay.

## Children and retained state

`.children(...)` saves a `ChildrenPlan`; configuring the parent does not run child component or node bodies. During synchronous `finish`, the parent installs its node context and binds its style, including font and transform state, then materializes the declared child plan before running `body`. `take_children()` and layout use that already built list. Each real child gets its own owner and inherits ordinary scope context lookup. Place shared `provide` calls in the parent component body before returning its builder, or in the node author body before `ctx.finish(...)`, so they are available to child construction.

Tuples can mix builders, prebuilt nodes, plans, and existing child lists. Vectors and arrays preserve order, and existing owning node containers retain their ownership behavior. A prebuilt `Node` keeps the context in which it was constructed; attaching it to a new parent does not rebuild it. This also applies to an independent node manually constructed with `into_node()` inside a component body: it observes the context available at that call and is not retroactively updated when the component's returned builder later establishes the final parent bindings. Return builders and use ordinary `.children(...)` plans when children should observe that final parent context. Deferred construction occurs synchronously during the parent's `finish`, with no need for an application-written lazy closure. `ChildrenPlan` retains a one-shot owned callback until then, so its stored callback and inputs keep their existing `'static` requirements. Setter argument expressions still evaluate at their ordinary Rust call sites, and explicitly calling `into_children` requests immediate conversion.

`map_keyed` waits until the parent context is installed before starting its list computation. Later new keys restore that context and get a separate owner for resources created in their callback. Keys must be unique. Reordering retained keys preserves nodes, original content, and local state without rerunning the callback; changed data under an unchanged key needs its own reactive binding. Removing a key releases its node and callback-owned resources. Adding that key again creates a new node.

Use `map_keyed_prop(list, key_fn, node_fn)` when each retained node needs the latest item or its current position. The construction callback receives `KeyedItem { key, value: Prop<T>, index: Prop<usize> }` and still runs only for new keys. Bind properties through `value.map(...)` or a computed that reads `index`; event callbacks can read `value` when the event occurs:

```rust,ignore
let rows = map_keyed_prop(items, |item| item.id, |row| {
    let value = row.value;
    let label = value.map(|item| item.name);
    button(label).on_activate(move || rename(value.get_clone()))
});
```

This variant requires `T: Clone + PartialEq` and `K: Clone + Hash + Eq`. Same-key updates change the value binding without rebuilding; reordering changes the index binding; equal values do not notify their consumers. Both bindings derive from a shared computed list snapshot, so reading them inside a batch refreshes the latest observable list. The owning children releases all row bindings on drop. A removed row's cleanup may read its last value/index from that snapshot; intermediate list writes never observed inside a batch may be coalesced. Construction callbacks and row cleanup must not read the returned children signal while it is being recomputed: this is a reentrant read of the same computed output. Cleanup should use the row's value/index bindings instead. The extra snapshot and per-row computed bindings are opt-in costs; `map_keyed` remains the simpler API for fixed row inputs. Ordered `list` uses this variant to retain rows while their displayed indices change.

Use `keyed(value, node_fn)` to switch a single subtree from a scalar `Prop<T>` or signal, without wrapping the value in a one-element list. It requires `T: Clone + PartialEq`, with no hashing or separate key function. An equal value retains the current node; a different value creates a new node and releases the old node and its callback-owned resources. Switching back builds a fresh node. Like `map_keyed`, construction is deferred until mount and later builds inherit the mount context. Reads directly inside `node_fn` are snapshots and do not subscribe the selector; nested reactive bindings still track normally. For example:

```rust,ignore
let region = keyed(phase, move |phase| match phase {
    Phase::Idle => text("Press Load to fetch data.").into_node(),
    Phase::Loading => loading_spinner(),
    Phase::Ready => content_view(data.get_clone().unwrap()),
    Phase::Failed => error_view(error.get_clone().unwrap()),
});
```

Updating `data` while `phase` stays `Ready` does not rebuild this region; pass a reactive binding to `content_view` if it should update without a phase change. The scalar helper removes the input list allocation, list projection, and keyed-list bookkeeping. Its output still uses the shared `Children` representation, `Rc<[NodeRef]>`.

`show_if` also returns a `ChildrenPlan`. Materializing the plan during `finish` constructs its node once, even if the initial condition is false. Toggling visibility changes the visible child list while retaining that node and its local state; hiding it does not dispose it or rerun the body when shown again. Dropping the owning child list releases it.

## Scope, failure, and async boundaries

The node owns resources created in its synchronous construction scope. An unconsumed builder or child plan releases what it already holds when dropped, without executing its declaration bodies. A panic during construction is not recovered: it propagates to the caller, the unfinished node and its owner are never released, layout and gesture registrations made so far stay in place, and the node stack is not unwound. Nothing in the library catches panics, so treat a construction panic as fatal for the process instead of a recoverable error.

`Scope`, `Signal` and `NodeRef` handles are thread-local, non-owning handles; copying them does not extend their owner's lifetime or make them transferable to another thread. `Scope::run(|| async { ... })` installs the scope only while constructing the future. `mue_core::spawn` returns a caller-owned task with no `OwnedScope` or inherited context. Each poll and future destruction runs under `Scope::null` and `Runtime::without_effect`, with no ambient tracker or `current_effect`; the previous context is restored on normal return. Captured handles still depend on their original owners. Resources created by a plain task need an explicit owner or manual disposal.

Use `scope_future(future)` to opt into an independent `OwnedScope::new_inherited()` created at wrapping time. It installs its own scope for polling and inner future destruction, and drops the inner future and owner before returning a completed output, even if the wrapper is retained. Its parent link supports weak context lookup, not keepalive or a snapshot. Only resources constructed under that scope are owned by the wrapper; capturing an existing resource does not transfer ownership. Returning a local reactive handle leaves an invalid handle after completion. The wrapper works standalone for scope ownership or in `spawn(scope_future(...))` for task execution isolation as well. Effect reruns retain their existing behavior and do not restore their creation scope. See [foreground tasks][tasks-guide] for a concrete example.

Disposing the creator scope does not cancel the task. Dropping the task handle cancels it; `detach()` transfers its lifetime management to the executor. `Task<T>.await` yields `T` directly. Its handle is `Send` when `T: Send` and may cross threads for await/drop, but its future is polled and destroyed on its original UI thread, where any scoped cleanup also runs.

`mue_core::task::tick(limit)` polls at most `limit` ready tasks without blocking. `sleep` / `timeout` register with the same foreground executor and are driven by `tick`; they are `!Send` and cannot be used from background tasks. `spawn_blocking` runs blocking closures on fresh threads; `JoinSet` joins foreground tasks; `block_on` is for tests and headless hosts. `mue_macroquad::App::frame` drives up to 32 task polls after timeouts and before layout. One poll is not preempted: synchronous I/O or CPU-heavy code still blocks that frame. Blocking work uses `spawn_blocking` or `AsyncResource::new_blocking`; the result is published on the UI thread. Network/IO work runs on the host's runtime and is awaited from `AsyncResource::new`. For expected failures, use a `Result<T, E>` task output or `AsyncResource<T, E>`, which owns its state owner and task and exposes `Loading` then `Ready(Result<T, E>)` as a reactive signal. Its state persists until resource drop; `new` runs the operation on the foreground executor, while `new_blocking` runs a blocking closure on a fresh OS thread. A blocking operation cannot use `scope_future`. `is_finished()` becomes true after future destruction and any opt-in scoped cleanup. Polling a completed task again panics. `Task::fallible` converts the handle into a `FallibleTask<T>` that yields `Option<T>`; `None` reports a task stopped by the executor instead of panicking. `try_spawn` / `try_tick` return `Err(ExecutorClosed)` during shutdown; panics still propagate without recovery.

For application-owned tokens, `provide` / `inject`, snapshots versus bindings, scope ownership, asynchronous boundaries, explicit hover, and inherited foreground, see [application tokens and context][themes-guide].
## Modal controls

`overlay_host(open, content)` renders a full-screen modal layer. It is hidden
when `open` is false, captures pointer input while open, and keeps the content
mounted while hidden. Content taps are swallowed; a backdrop tap only dismisses
when `dismissible(true)` is set.

```rust,no_run
use mue::{components::overlay_host, prelude::*, renderer::Shadow};

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
        .shadow(Shadow::new(18., 16.))
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

`use_background` caches CPU `Shapes` through `Paint::build`, rebuilding when its
tracked rect, brush, radius or paint state changes. Cached geometry still uses
immediate submission and batching; no per-background GPU buffers are created.
Permanently transparent solid backgrounds do not create a cache, but may still
draw shadows. The reactive cache is eager: changing dependencies can rebuild
hidden retained nodes too. Paint-building callbacks and their property producers
should be pure geometry/value computations, not recursively draw or build nodes
while the shared tessellator is borrowed.

`Style::shadow(Shadow::new(elevation, radius))` draws a drop shadow behind the
background; use the same radius as `border-radius`. The shadow follows the
node transform (rotation and scale). Shadows are drawn by `use_background`, so
custom nodes must call `use_container` or `use_background`. Shadow geometry is a
stack-built quad with fixed indices, submitted on the immediate path without
temporary `Shapes` or geometry `Vec`s. Material uniform snapshots and batch-buffer
growth may still allocate. Shadow-mask admission defaults to `ShadowCache::Auto`:

```rust
use mue::{prelude::*, renderer::{Shadow, ShadowCache}};

let auto = flexbox().shadow(Shadow::new(18., 16.)); // Auto when unset

let disabled = flexbox()
    .shadow(Shadow::new(18., 16.))
    .shadow_cache(false); // always analytic (Disabled)

let enabled = flexbox()
    .shadow(Shadow::new(18., 16.))
    .shadow_cache_policy(ShadowCache::Enabled); // bypass the Auto admission heuristic
```

The existing `.shadow_cache(true/false)` API still accepts bool, `Prop<bool>` and
bool signals; true means Enabled, false means Disabled. Use `.shadow_cache_policy`
with `ShadowCache::{Auto, Enabled, Disabled}` or a reactive policy signal. These
setters write the same property; the last setter wins. Declared/take accessors now
return `Prop<ShadowCache>` rather than bool. The fluent API does not add a CSS
`shadow-cache` or `box-shadow` declaration to `style!`.

Exact resident masks reuse immediately, including a new node or a return to an
old geometry key. Only new bakes wait for width, height, effective radius, blur
and sampling resolution to remain unchanged for 100 ms. Color, opacity, translation
and rotation at the same density reuse the mask; size/scale/DPI/camera/viewport
changes may require a new key.

Auto estimates conservative visible halo coverage in physical target pixels: an
affine projected quad clipped by target, viewport and raw scissor, minus the full
body rectangle. A new key needs at least 8192 halo pixels in one observation or
at least four recently drawn distinct same-key nodes whose halo sum reaches 8192.
It also needs eight observations and accumulated halo work at least the bake
texture pixel count within a 500 ms fixed window. The bookkeeping is bounded to
16 pending keys and four contributors/key, using the strongest recent coverage;
repeated calls on one node are not four contributors. These are conservative
heuristics, not a GPU-profit guarantee or a frame snapshot. Enabled bypasses these
economic checks, but not stability/backend/quality/storage limits. Small shadows
and overloaded pending sets may stay analytic indefinitely.

Zero physical coverage or a clip strictly within the knockout core does not
count demand, sample or bake. Uncertain rounded edges/corners still draw. Auto
does not infer occlusion by other widgets/materials, and native projected or
non-normalized homographies fall back rather than applying unsafe culling.
The sharp interior cutout is still evaluated per fragment, preventing filtered
mask texels from leaking into a transparent panel; this is not a full pixel
snapshot or a retained mesh. RGBA8 quantization/filtering can cause small visual
differences from the analytic shader.

The render-thread pool has at most 8 reusable slots and a 16 MiB logical storage
budget, with a 4 MiB/2048-pixel limit per mask. It uses native OpenGL only; blur
below 1 logical pixel, projective transforms, excessive scale/size, unsupported
backends, resource failure or cache pressure fall back to the analytic path.
Continuously changing parameters do not bake every frame; a slow step animation
that rests longer than the stable window can still bake. Missing/evicted nodes wait again,
recently sampled masks are protected from eviction for 100 ms, and GPU bake
attempts (including failures) are throttled to one per 16 ms after completion.
A full actively used pool leaves extra shadows analytic instead of rotating all
masks through repeated bakes. These are heuristics, not GPU-time budgets. No WebGL or Metal support is added by this option.

First bake, slot reuse and `shadow::clear_shadow_mask_cache()` flush queued work
before an offscreen pass or texture resize. They leave macroquad's camera, model,
material, viewport and scissor state intact, but cannot resume an arbitrary host
raw miniquad pass. Keep mue drawing outside host raw passes. Read
`shadow::shadow_mask_cache_stats()` for hits, bakes, fallbacks, Auto rejections,
culled draws, bounded pending keys and logical bytes;
clear on the live render thread to discard demand history, invalidate masks and
shrink valid slots to 1x1 (the 16 ms attempt throttle remains). Failed storage or
passes can reuse their existing slot after the wait; failed texture/FBO object
creation is never repaired by a resize or drawn through. Known shader initialization
failures fall back without repeating GPU work. The bounded pool and its baker live
until context teardown, not node drop.
In-flight driver storage may temporarily exceed the logical budget. Built-in rounded/sector
clip stacks are applied to both analytic and cached shadow sampling; arbitrary external
materials remain outside that stack.

`Style::clip_radius(Size::Px(12.))` clips the node's children to a rounded
rectangle. It does not clip the node's own background; use `border-radius` for
that. Rotation and scale are exact via the inverse model, and hit-testing
follows the clip. Retained text/shape use the matching retained clip material
while that clip pipeline is selected. If the matching shader is unavailable, or
an inner custom material replaces the clip, nodes fall back to immediate drawing.

`UiMaterial::with` / `with_material` use immediate geometry for text and shape
nodes; ordinary macroquad materials do not share the retained vertex layout.
Leaving the scope restores the previous pipeline and retained partner, not the
outer ordinary material's uniforms. Direct raw pipeline switches are checked by
pipeline identity; raw changes to the same material's uniforms or textures are
not mirrored into its retained partner. Temporary material fallback preserves
previously uploaded text/shape meshes. Large text uses retained quads only with a
solid foreground brush; gradients keep immediate per-vertex sampling. Text cache
keys track atlas evictions and the block's own glyph residency, not unrelated
glyph insertions. Fractional layout origins participate in baseline snapping.

`Style::clip_sector(Sector::new(center, start, end))` clips children to an
angular sector. `center` is relative to the node's layout origin, angles are
radians in local layout space, and `.feather(radians)` softens both edges. The
node transform applies to the sector shape. Hit-testing follows the hard
angular boundary; feather only affects drawing. Import `Sector` from
`mue_macroquad`.

`button(label)` defaults to the secondary variant. Use `primary(true)` or
`variant(ButtonVariant::Ghost)` for other weights, `disabled(...)` and
`loading(...)` with reactive values, and `on_activate(...)` for the action.
The current overlay host is a single layer: stacking, enter/exit animation, and
back priority are not implemented yet. IconButton and keyboard focus are also
pending. Button colors are fixed for now; replacing them with application tokens
is a follow-up.

## Textures

`SharedTexture` (`mue_macroquad::SharedTexture`) is the UI-thread handle:
`Rc<Texture2D>`, cheap to clone, and `!Send` by design. Use it for `image`,
`Portal`, shaders, and any render-thread-only sharing.

`SafeTexture` (`mue_macroquad::SafeTexture`) is the `Send + Sync` handle for
textures that may be dropped on a worker thread. It queues the underlying
`Texture2D`; `App::frame` flushes the queue on the render thread. Hosts that do
not use `App` call `flush_pending_texture_deletions()` after their draw calls.
`SafeTexture::to_shared()` clones the `Texture2D` into a `SharedTexture`
(sharing macroquad's managed slot); `try_into_inner()` and `into_shared()`
transfer only when there is a single owner.

Macroquad 0.4's `Texture2D` is `Clone` (not `Copy`); dropping the last clone on
a worker thread panics. Do not replace `SharedTexture` with `SafeTexture` for
UI-thread-only work: the `!Send` bound is a useful guard, and `SafeTexture`
adds atomic refcounting plus a deletion queue.

[scenes-guide]: scenes.md
[scene-input-guide]: scenes.md#input
[tasks-guide]: tasks.md
[themes-guide]: themes.md
[components-api]: https://github.com/void-cat-0/mue/blob/main/mue-components/src/lib.rs
