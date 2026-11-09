# Foreground tasks

`mue-core` provides a local executor backed by `async-task`. `spawn` accepts a non-`Send`, `'static` future and isolates each poll from the ambient reactive scope and effect tracker. It creates no reactive owner and does not inherit `provide` / `inject` context. Reactive handles remain `Copy` but are `!Send + !Sync`. A `Task<T>` handle is `Send` when `T: Send`, so it may move across threads for await/drop; this does not move or poll its future on the caller thread. The future is polled and destroyed on its original UI thread. Dropping a handle on another thread requests cancellation; the original thread must tick again or shut down to destroy the future. A completed output is an ordinary owned value and may be consumed or dropped on the observer thread.

```rust
use mue_core::task::{spawn, tick};

async fn example() {
    let task = spawn(async { Ok::<_, String>(42) });
    assert!(!task.is_finished());
    // A standalone host drives tasks. Macroquad App::frame does this automatically.
    tick(32);
    assert!(task.is_finished());
    assert_eq!(task.await, Ok(42));
}
```

Each plain task poll and future destructor runs under `Scope::null` and `Runtime::without_effect`: there is no ambient dependency tracker or `current_effect`. The previous scope and tracking context are restored on normal return. `Scope::run(|| async { ... })` only constructs a future in the synchronous scope; it does not install that scope when the future is polled. Panics still propagate without recovery.

Plain tasks should communicate with already-owned state through captured handles; use `try_get` / `try_set` when an owner may disappear between polls. These methods handle a missing direct signal, not a live derived signal whose dependencies were disposed. Signals, effects, and derived resources created in a plain task have no automatic owner: arrange explicit disposal, or construct them under an explicitly retained owner. Dropping their non-owning handles or completing/cancelling the task does not dispose them.

## Opt into an operation scope

`mue_core::scope::scope_future(future) -> ScopedFuture<F>` is also exported from the crate root and prelude. Calling it creates an independent `OwnedScope::new_inherited()` immediately, with a weak parent link for context lookup. This neither keeps the parent alive nor snapshots its context: after the parent is disposed, its values are no longer injectable. The wrapper installs its own scope while polling or destroying the inner future. It works as a standalone future or as `spawn(scope_future(...))`; standalone use controls scope and ownership only. `spawn` supplies task execution isolation from the ambient effect tracker. The wrapper itself has no `'static` requirement and can borrow local data, while passing it to `spawn` still requires a `'static` future. Its owner makes `ScopedFuture<F>` `!Send + !Sync`.

```rust
use mue_core::{prelude::*, scope::{inject, provide}, task::tick};

#[derive(Clone)]
struct Label(String);

async fn scoped_example() {
    let parent = OwnedScope::new();
    let operation = parent.run(|| {
        provide(Label("Loaded".to_owned()));
        scope_future(async {
            let label = inject::<Label>().expect("parent context remains alive");
            // Constructed when polled under the wrapper, so the wrapper owns it.
            let local = signal(label.0);
            local.get_clone() // Return owned data, not the local signal handle.
        })
    });
    let task = spawn(operation);
    tick(32);
    assert_eq!(task.await, "Loaded");
    drop(parent);
}
```

On `Poll::Ready`, the wrapper drops the inner future under its scope and drops the owner before returning the output, even if the completed wrapper is retained. Dropping a pending wrapper performs the same scoped destruction and owner cleanup. Owned effect and computed cleanups run before the scope tree's signals are removed, so they may read still-live signals in that tree. During teardown, cleanup must not create reactive resources, reparent scopes, or mutate the disposing graph; cleanup panics remain fatal. Local reactive handles returned as output are already invalid; return owned data or use a separate owner that outlives the operation. Only resources constructed while the wrapper's scope is installed belong to it. Capturing a pre-created signal, effect, or derived resource does not transfer ownership. Effect callbacks keep their existing rerun behavior: rerunning an effect does not restore its creation scope.

Dropping `Task<T>` cancels it. Reactive scopes do not register spawned tasks for cancellation; if a scope-owned value actually holds a `Task` handle, disposing that value still cancels through normal `Task::drop`. Awaiting `Task<T>` yields `T` directly, so `Task<Result<T, E>>` yields the business `Result<T, E>` without wrapping it in an `Option`. `detach()` explicitly transfers lifetime management to the executor and discards output observation; detached work remains executor-managed until completion or executor shutdown. `AsyncResource` instead owns its task handle and cancels it when the resource is dropped. Cancellation cannot roll back prior writes or external side effects, and cannot interrupt a running poll. Polling a completed task again, or awaiting a task forcibly stopped at executor shutdown, panics; there is no fallible `Task` API or panic recovery.

`tick(limit)` drives a bounded number of ready runnables without waiting for external events. The Macroquad adapter calls it after timeouts and before layout. The limit is a count of polls, not a time guarantee: one blocking call or long CPU loop still blocks the UI thread. Background work is provided by `spawn_background` and `AsyncResource::new_background` (see [Background tasks](#background-tasks)); there is still no blocking-I/O adapter, timer reactor, Tokio context, or automatic retry. Use futures whose wake sources are independently driven. The host wake callback requests a later frame; it must not synchronously poll tasks. It is stored in the executor's wake slot, so a callback installed after a task is spawned is still observed. `App::new` installs the Macroquad callback before materializing its root builder; tasks started before that need the callback installed explicitly when using a blocking event loop.

A retained task can safely skip publishing when its destination is gone:

```rust
use mue_core::{prelude::*, task::{spawn, tick}};
let owner = OwnedScope::new();
let destination = owner.run(|| Signal::new(0u32));
let task = spawn(async move {
    let result = 42; // Real work can await external events here.
    let _ = destination.try_set(result); // Err(result) if the destination is gone.
});
drop(owner);
tick(32);
assert!(task.is_finished());
drop(task);
```

`ReadSignal` and `Prop` provide `try_get`, `try_get_clone`, and untracked variants. They do not keep the signal alive; the ordinary accessors keep their existing lifecycle requirements. Retain task handles in the intended UI/resource owner, or explicitly detach them. Detached work may continue using CPU, I/O, and captured data after its original UI disappears.

## Rendering a result

```rust
use mue_core::{prelude::*, task::{AsyncResource, AsyncState}};
let owner = OwnedScope::new();
let resource = owner.run(|| AsyncResource::<String, String>::new_background(async {
    Ok("Loaded".to_owned())
}));
let caption = owner.run(|| resource.state().map(|state| match state {
    AsyncState::Loading => "Loading…".to_owned(),
    AsyncState::Ready(Ok(value)) => value,
    AsyncState::Ready(Err(error)) => format!("Could not load: {error}"),
}));
// Retain owner and resource while the UI uses caption; App::frame polls the task.
// Once the UI no longer uses caption:
drop(owner);    // Dispose the caption projection.
drop(resource); // Cancel pending work and dispose result state.
```

`AsyncResource` owns an independent state owner and its task handle. The result state remains alive after the operation completes, until the resource is dropped; ending the configuration scope does not dispose it. `new` runs the operation on the foreground executor and accepts a non-`Send` future; `new_background` runs it on the global background pool and requires the future and output to be `Send`; `new_background_with` uses a specific pool. In both cases the result state is published on the UI thread by a foreground task owned by the resource. The foreground operation follows plain `spawn` semantics, with no inherited context or automatic resource owner, unless the caller passes `scope_future(operation)`. A background operation cannot use `scope_future` because that wrapper is `!Send`; if it needs operation-local reactive resources, keep the operation on the foreground or split it into background loading plus a foreground apply step. That wrapper's operation-local resources are released at completion, while the resource's result state persists. The caption projection above belongs to `owner`, so retain both owners for as long as the UI uses the binding.

`Task<Result<T, E>>::detach_and_report(|error, location| { ... })` explicitly observes a business failure once and supplies the reporting call site. Its reporter runs as a plain task with no automatic scope or inherited context, detached from reactive-scope lifetimes. Code captured by the reporter must not assume ordinary signal/scope handles remain alive; resources it creates need explicit ownership or disposal. Cancellation is silent. Plain `detach()` intentionally discards all output.

`AsyncResource` retains the original typed error as reactive state and does not report it automatically. Handle it in the UI or explicitly report it at the application boundary. Replacing the resource cancels the old operation; different resource instances do not share a result destination. Reusing a separate destination across requests still requires request identity/version checks.

Panics propagate from polling. There is no task panic isolation, `catch_unwind`, state rollback, or promise that the runtime can be reused after a panic. Ordinary RAII cleanup and task cancellation are supported; panic recovery is not.

## Known limitations

These are known edge cases in the current implementation. They are documented
rather than fixed; the framework's own code does not exercise them.

- **Task poll/drop inherits `UpdateContext`.** `Runtime::without_effect` clears
  the ambient dependency tracker and `current_effect`, but `UpdateContext` is a
  separate TLS. If `tick` or `Task::drop` runs inside
  `with_update_context(...)`, signal writes made by the task or by its future
  destructor carry that metadata to downstream effects. The current production
  code does not call `with_update_context`; this is a latent contract issue. If
  task isolation is the intended contract, the fix is to save, clear, and
  restore `UpdateContext` in `without_effect` (and initialize
  `UpdateContext::current()` in `Executor::new` so its TLS outlives the
  executor). If metadata propagation is intended, this should be stated
  explicitly instead.
- **`spawn`/`tick` abort if called from a future destructor during executor
  shutdown.** `Executor::drop` runs after the executor TLS is marked destroyed.
  A future destructor that calls `spawn`/`tick` there panics inside async-task's
  `abort_on_panic`, which aborts the process. The framework's own destructors do
  not do this. Do not submit new work from a future destructor or another TLS
  destructor during shutdown; perform synchronous cleanup instead. A fallible
  `try_spawn`/`try_tick` API is a possible future addition, not implemented.
- **A `ScopedFuture` must not outlive the reactive runtime.** The wrapper is
  bound to the current thread's runtime. If it is stored in a TLS slot (or
  another owner) that outlives the runtime, its `Drop` calls
  `owner.run(...)`, which asserts the scope is still alive; after runtime
  teardown this panics, and a panic in a TLS destructor usually aborts. Keep
  the wrapper in a local or task-owned value that is dropped before the
  runtime. A best-effort leak guard in `Drop` is a possible future change, not
  implemented.
- **Panic recovery is not supported.** A caught task poll panic leaves
  `tick`'s `in_tick` flag set and may leave the ambient scope/tracker stale.
  Do not reuse the runtime after catching a task panic. This is the existing
  panic-fatal policy, not a recoverable error path.

# Background tasks

`spawn_background` runs a `Send + 'static` future on a fixed-size worker pool and returns a `BackgroundTask<T>`. The output must also be `Send + 'static`. The future is polled and destroyed on a worker thread; it must not access signals, scopes, context, or UI-thread resources. Return owned `Send` data instead, then publish it from a foreground task. It may be called from the UI thread or a background thread; the foreground `spawn` and `tick` are rejected on background workers in debug builds.

~~~rust
use mue_core::{prelude::*, task::{spawn, spawn_background, tick}};

let destination = signal(0u32);
let background = spawn_background(async move { 42u32 });
let publisher = spawn(async move {
    let value = background.await;
    let _ = destination.try_set(value);
});
// App::frame drives the publisher after the background task wakes it.
// The result is not guaranteed to be ready after one tick.
tick(32);
~~~

The foreground publisher is an ordinary `Task<()>`; dropping it drops the `BackgroundTask`, which cancels the background work. The background task wakes the foreground publisher through the normal executor wake path, so `App::frame` can run the continuation on the next tick. The wake callback is stored in the executor's wake slot and may be installed after the task is spawned.

## AsyncResource on the background pool

`AsyncResource::new_background` packages the publisher for the common loading case. The operation and its result must be `Send`; the `Loading` / `Ready(Result<T, E>)` state is still published on the UI thread.

~~~rust
use mue_core::{prelude::*, task::{AsyncResource, AsyncState}};

let resource = AsyncResource::<String, String>::new_background(async {
    Ok("Loaded".to_owned())
});
let caption = resource.state().map(|state| match state {
    AsyncState::Loading => "Loading…".to_owned(),
    AsyncState::Ready(Ok(value)) => value,
    AsyncState::Ready(Err(error)) => format!("Could not load: {error}"),
});
// Retain the resource while the UI uses caption; App::frame ticks the publisher.
// Once the UI no longer uses caption:
drop(resource);
~~~

`AsyncResource::new` remains the foreground constructor for operations that must touch UI-thread state. `new_background_with(&executor, operation)` uses a specific pool; the executor must outlive the resource. `from_background_task(task)` wraps an already-spawned `BackgroundTask<Result<T, E>>`.

The playground uses this pattern for its bundled font database: `fonts::database()` runs on the background pool, and the main loop calls `font::initialize` on the UI thread once the state is `Ready`, replacing a text-free loading screen with the real UI. Its graphics and input pages also decode PNGs to an `Image` on the background pool, then create the GPU texture from a foreground effect.

## Cancellation and lifecycle

Dropping a `BackgroundTask` requests cancellation. A queued task is dropped by a worker; a running poll cannot be interrupted, and a blocking syscall continues until it returns. `detach()` lets the executor run the task without an observer. `is_finished()` reports completion or closure. The global pool used by `spawn_background` lives for the process lifetime. A custom `BackgroundExecutor` closes its queue and joins its workers when the last handle is dropped; tasks still held by a `BackgroundTask` are destroyed when that handle is dropped, on the dropping thread. The future is `Send`, so that cleanup is safe, but it may block the caller.

## Panics

Background tasks use async-task's panic propagation: a panic is caught by the executor, the worker survives, and awaiting the task resumes the panic on the awaiting thread. A detached task's panic is stored and dropped with the task allocation, so it is not reported. There is no panic recovery or state rollback.

## Limitations

- There is no blocking-I/O pool, timer reactor, priority, or progress channel yet. Do not run long blocking work on the async pool; it occupies a worker.
- Awaiting a background task from inside a task on the same pool can deadlock when all workers are occupied. Use a separate executor or restructure the work.
- `AsyncResource::new_background` requires `T: Send` and `E: Send`. UI-only resources such as textures should be created on the foreground after loading `Send` bytes.
- Background tasks are not supported on wasm targets that lack OS threads.
- Dropping a custom `BackgroundExecutor` stops its workers; a pending task handle that is still awaited can then hang. Keep the executor alive for the task's lifetime.
