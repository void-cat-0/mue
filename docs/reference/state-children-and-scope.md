# Reactive runtime and ownership

[Reference contents][guide-reference] · [Related lesson][basics-guide]

- [Reactive state and owners](#reactive-state-and-owners)
- [Children and retained state](#children-and-retained-state)
- [Scope, failure, and async boundaries](#scope-failure-and-async-boundaries)

## Reactive state and owners

| Type or operation | Contract |
| --- | --- |
| `Signal<T>` | A writable handle. `set` replaces the current value and `update` edits it in place. The handle is thread-affine and does not own the signal storage. |
| `ReadSignal<T>` | A read-only, thread-affine handle. `get` copies and `get_clone` clones. Both reads register a dependency inside a reactive computation. |
| `Prop<T>` | A static or reactive input. A component can accept a fixed value at one call site and a derived signal at another. |
| `map`, `computed` | Allocate a derived binding in the current scope. Use untracked reads when a read must not subscribe to its source. |
| `OwnedScope` | Keeps a scope alive. Dropping the owner disposes the values, effects, scopes, and other registered resources it owns. Copying an access handle does not extend its lifetime. |

```rust
use mue::prelude::*;

let owner = OwnedScope::new();
let (count, label) = owner.run(|| {
    let count = signal(2_u32);
    let label = count.map(|count| format!("{count} queued"));
    (count, label)
});
count.set(3);
assert_eq!(label.get_clone(), "3 queued");
```

Keep `owner` alive while callers use either handle. See the [theme context reference][themes-reference] for ancestry and dependency rules, and the [task reference][tasks-reference] for futures that own an operation scope.
### Writes, tracking, and batching

| Operation | Notification and timing |
| --- | --- |
| `set` / `set_with` | Compare against the previous value. An equal result suppresses notification for a signal created with `signal`. `Signal::new_always` opts out of equality filtering. |
| `update` | Borrows the value mutably for the closure and notifies after the closure. The ordinary comparator does not suppress this notification. |
| `set_untracked` / `update_untracked` | Change stored data without scheduling subscribers. This can leave already-derived values unchanged until a later notification. |
| `get_untracked` / `get_clone_untracked` | Read without subscribing the current computation to this signal. |
| `try_get` / `try_set` | Handle missing or temporarily unavailable storage. User callbacks and comparators can still panic. |
| `batch` | Delay queued notifications until the outer batch finishes. A tracked read can still demand an up-to-date derived value within the batch. |

```rust
use mue::{core::runtime::batch, prelude::*};

let owner = OwnedScope::new();
let (done, total, caption) = owner.run(|| {
    let done = signal(1_u32);
    let total = signal(2_u32);
    let caption = computed(move |_| format!("{} / {}", done.get(), total.get()));
    (done, total, caption)
});
batch(|| {
    done.set(2);
    total.set(4);
});
assert_eq!(caption.get_clone(), "2 / 4");
drop(owner);
assert_eq!(caption.try_get_clone(), None);
```

Do not read or write the same signal from inside its `update` callback. Use the reference supplied to the callback because the signal temporarily takes its stored value out of the runtime. A signal handle is also unavailable once its scope is disposed. Check ownership before using an ordinary read or write. Use `try_get_clone` or `try_set` at boundaries where disposal is expected.

### Derived values, side effects, and context

Use `computed` or `map` to publish a value. `computed` receives an optional reference to the previous result and suppresses downstream updates when the result compares equal. `computed_always` publishes on each evaluation. Keep derivations free of writes and other side effects so pulling a derived value during layout or input cannot change unrelated state.

Use `watch` when a source change should run a side effect after setup. Use `watch_immediate` to run the callback once at setup as well, or `watch_effect` to track the reads made by the effect body. `on_cleanup` registers work for before a tracked callback reruns and when its owner releases it. A scope both owns resources and provides a context lookup site. `OwnedScope::new` starts without inherited context, and `new_inherited` also keeps the current scope as its context parent.

Custom stored data can enter a builder through `Prop::Static(value)`. Passing a bare value uses the convenience `PropValue` conversion. To give a component a writable input, declare `#[model] value: T` and supply a `Signal<T>`.

## Children and retained state

`.children(...)` saves a `ChildrenPlan`; configuring the parent does not run child component or node bodies. During synchronous `finish`, the parent installs its node context and binds its style, including font and transform state, then materializes the declared child plan before running `body`.

`take_children()` and layout use that already built list. Each real child gets its own owner and inherits ordinary scope context lookup.

Place shared `provide` calls in the parent component body before returning its builder, or in the node author body before `ctx.finish(...)`, so they are available to child construction.

Tuples can mix builders, prebuilt nodes, plans, and existing child lists. Vectors and arrays preserve order, and existing owning node containers retain their ownership behavior.

A prebuilt `Node` keeps the context in which it was constructed; attaching it to a new parent does not rebuild it. This also applies to an independent node manually constructed with `into_node()` inside a component body: it observes the context available at that call and is not retroactively updated when the component's returned builder later establishes the final parent bindings.

Return builders and use ordinary `.children(...)` plans when children should observe that final parent context. Deferred construction occurs synchronously during the parent's `finish`, with no need for an application-written lazy closure.

`ChildrenPlan` retains a one-shot owned callback until then, so its stored callback and inputs keep their existing `'static` requirements. Setter argument expressions still evaluate at their ordinary Rust call sites, and explicitly calling `into_children` requests immediate conversion.

`map_keyed` waits until the parent context is installed before starting its list computation. Later new keys restore that context and get a separate owner for resources created in their callback.

Keys must be unique. Reordering retained keys preserves nodes, original content, and local state without rerunning the callback; changed data under an unchanged key needs its own reactive binding.

Removing a key releases its node and callback-owned resources. Adding that key again creates a new node.

Use `map_keyed_prop(list, key_fn, node_fn)` when each retained node needs the latest item or its current position. The construction callback receives `KeyedItem { key, value: Prop<T>, index: Prop<usize> }` and still runs only for new keys.

Bind properties through `value.map(...)` or a computed that reads `index`; event callbacks can read `value` when the event occurs:

```rust,no_run
# use std::rc::Rc;
# use mue::{prelude::*, renderer::node::map_keyed_prop};
# #[derive(Clone, PartialEq)]
# struct Item { id: u32, name: Rc<str> }
# let items = signal(Rc::<[Item]>::from([]));
# fn rename(_: Item) {}
let rows = map_keyed_prop(items, |item| item.id, |row| {
    let value = row.value;
    let label = value.map(|item| item.name);
    button(label).on_activate(move || rename(value.get_clone()))
});
```

This variant requires `T: Clone + PartialEq` and `K: Clone + Hash + Eq`. Same-key updates change the value binding without rebuilding; reordering changes the index binding; equal values do not notify their consumers.

Both bindings derive from a shared computed list snapshot, so reading them inside a batch refreshes the latest observable list. The owning children releases all row bindings on drop.

A removed row's cleanup may read its last value/index from that snapshot; intermediate list writes never observed inside a batch may be coalesced. Construction callbacks and row cleanup must not read the returned children signal while it is being recomputed: this is a reentrant read of the same computed output.

Cleanup should use the row's value/index bindings instead. The extra snapshot and per-row computed bindings are opt-in costs; `map_keyed` remains the simpler API for fixed row inputs.

Ordered `list` uses this variant to retain rows while their displayed indices change.

Use `keyed(value, node_fn)` to switch a single subtree from a scalar `Prop<T>` or signal, without wrapping the value in a one-element list. It requires `T: Clone + PartialEq`, with no hashing or separate key function.

An equal value retains the current node; a different value creates a new node and releases the old node and its callback-owned resources. Switching back builds a fresh node.

Like `map_keyed`, construction is deferred until mount and later builds inherit the mount context. Reads directly inside `node_fn` are snapshots and do not subscribe the selector; nested reactive bindings still track normally.

For example:

```rust,no_run
# use mue::{prelude::*, renderer::node::keyed};
# #[derive(Clone, PartialEq)]
# enum Phase { Idle, Loading, Ready, Failed }
# let phase = signal(Phase::Idle);
# let data = signal(Some("Morning reading"));
# let error = signal(Some("Forecast unavailable"));
# fn loading_spinner() -> Node { text("Checking forecast…").into_node() }
# fn content_view(value: &str) -> Node { text(value).into_node() }
# fn error_view(value: &str) -> Node { text(value).into_node() }
let region = keyed(phase, move |phase| match phase {
    Phase::Idle => text("Press Load to fetch data.").into_node(),
    Phase::Loading => loading_spinner(),
    Phase::Ready => content_view(data.get_clone().unwrap()),
    Phase::Failed => error_view(error.get_clone().unwrap()),
});
```

Updating `data` while `phase` stays `Ready` does not rebuild this region; pass a reactive binding to `content_view` if it should update without a phase change. The scalar helper removes the input list allocation, list projection, and keyed-list bookkeeping. Its output still uses the shared `Children` representation, `Rc<[NodeRef]>`.

`show_if` also returns a `ChildrenPlan`. Materializing the plan during `finish` constructs its node once, even if the initial condition is false.

Toggling visibility changes the visible child list while retaining that node and its local state; hiding it does not dispose it or rerun the body when shown again. Dropping the owning child list releases it.

| Change to a child list | What stays mounted |
| --- | --- |
| Swap two surviving keys | Both row nodes and their local state |
| Change a field for an existing key | The same row node. Bind to the live value with `map_keyed_prop` to see the edit. |
| Remove a key | The other rows. Adding the key again creates a fresh row. |
| Set a `show_if` condition to false | Its owned subtree. It no longer participates as a displayed child. |

## Scope, failure, and async boundaries

The node owns resources created in its synchronous construction scope. An unconsumed builder or child plan releases what it already holds when dropped, without executing its declaration bodies.

A panic during construction is not recovered: it propagates to the caller, the unfinished node and its owner are never released, layout and gesture registrations made so far stay in place, and the node stack is not unwound. Nothing in the library catches panics, so treat a construction panic as fatal for the process instead of a recoverable error.

`Scope`, `Signal` and `NodeRef` handles are thread-local, non-owning handles; copying them does not extend their owner's lifetime or make them transferable to another thread. `Scope::run(|| async { ... })` installs the scope only while constructing the future.

`mue_core::spawn` returns a caller-owned task with no `OwnedScope` or inherited context. Each poll and future destruction runs under `Scope::null` and `Runtime::without_effect`, with no ambient tracker or `current_effect`; the previous context is restored on normal return.

Captured handles still depend on their original owners. Resources created by a plain task need an explicit owner or manual disposal.

Use `scope_future(future)` to opt into an independent `OwnedScope::new_inherited()` created at wrapping time. It installs its own scope for polling and inner future destruction, and drops the inner future and owner before returning a completed output, even if the wrapper is retained.

Its parent link supports weak context lookup, not keepalive or a snapshot. Only resources constructed under that scope are owned by the wrapper; capturing an existing resource does not transfer ownership.

Returning a local reactive handle leaves an invalid handle after completion. The wrapper works standalone for scope ownership or in `spawn(scope_future(...))` for task execution isolation as well.

Effect reruns retain their existing behavior and do not restore their creation scope. See [foreground tasks][tasks-guide] for a concrete example.

Disposing the creator scope does not cancel the task. Dropping the task handle cancels it; `detach()` transfers its lifetime management to the executor.

`Task<T>.await` yields `T` directly. Its handle is `Send` when `T: Send` and may cross threads for await/drop, but its future is polled and destroyed on its original UI thread, where any scoped cleanup also runs.

`mue_core::task::tick(limit)` polls at most `limit` ready tasks without blocking. `sleep` / `timeout` register with the same foreground executor and are driven by `tick`; they are `!Send` and cannot be used from background tasks.

`spawn_blocking` runs blocking closures on fresh threads; `JoinSet` joins foreground tasks; `block_on` is for tests and headless hosts. `mue_macroquad::App::frame` drives up to 32 task polls after timeouts and before layout.

One poll is not preempted: synchronous I/O or CPU-heavy code still blocks that frame. Blocking work uses `spawn_blocking` or `AsyncResource::new_blocking`; the result is published on the UI thread.

Network/IO work runs on the host's runtime and is awaited from `AsyncResource::new`. For expected failures, use a `Result<T, E>` task output or `AsyncResource<T, E>`, which owns its state owner and task and exposes `Loading` then `Ready(Result<T, E>)` as a reactive signal.

Its state persists until resource drop; `new` runs the operation on the foreground executor, while `new_blocking` runs a blocking closure on a fresh OS thread. A blocking operation cannot use `scope_future`.

`is_finished()` becomes true after future destruction and any opt-in scoped cleanup. Polling a completed task again panics.

`Task::fallible` converts the handle into a `FallibleTask<T>` that yields `Option<T>`; `None` reports a task stopped by the executor instead of panicking. `try_spawn` / `try_tick` return `Err(ExecutorClosed)` during shutdown; panics still propagate without recovery.

For application-owned tokens, `provide` / `inject`, snapshots versus bindings, scope ownership, asynchronous boundaries, explicit hover, and inherited foreground, see [application tokens and context][themes-guide].


[components-api]: https://github.com/void-cat-0/mue/blob/main/mue-components/src/lib.rs

[basics-guide]: ../learn/basics.md
[themes-guide]: ../learn/themes.md
[tasks-guide]: ../learn/tasks.md
[themes-reference]: themes-and-tokens.md
[tasks-reference]: tasks.md

[guide-reference]: index.md
