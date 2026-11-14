# Foreground tasks

`mue-core` provides a local executor backed by `async-task`. `spawn` accepts a non-`Send`, `'static` future and isolates each poll from the ambient reactive scope and effect tracker. It creates no reactive owner and does not inherit `provide` / `inject` context. Reactive handles remain `Copy` but are `!Send + !Sync`. A `Task<T>` handle is `Send` when `T: Send`, so it may move across threads for await/drop; this does not move or poll its future on the caller thread. The future is polled and destroyed on its original UI thread. Dropping a handle on another thread requests cancellation; the original thread must tick again or shut down to destroy the future. A completed output is an ordinary owned value and may be consumed or dropped on the observer thread. `Task::ready` wraps an already-computed value. `Task::fallible` converts a handle into a `FallibleTask<T>` that yields `Option<T>`; `None` reports a task stopped by the executor instead of panicking. `FallibleTask::try_take` polls once without driving the task: `Some(Some(value))` when ready, `Some(None)` when the executor stopped it, and `None` while pending or after the value/stop was taken. `Task::location` / `spawned_at` and `Debug` expose the spawn site.

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

Each plain task poll and future destructor runs under `Scope::null` and `Runtime::without_effect`: there is no ambient dependency tracker, `current_effect`, or `UpdateContext`. The previous scope and tracking context are restored on normal return; `Runtime::without_effect` also restores the tracker, `current_effect`, and `UpdateContext` on unwind, while `Scope::run` does not restore the ambient scope after a panic. `Scope::run(|| async { ... })` only constructs a future in the synchronous scope; it does not install that scope when the future is polled. Panics still propagate without recovery.

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

Dropping `Task<T>` cancels it. Reactive scopes do not register spawned tasks for cancellation; if a scope-owned value actually holds a `Task` handle, disposing that value still cancels through normal `Task::drop`. Awaiting `Task<T>` yields `T` directly, so `Task<Result<T, E>>` yields the business `Result<T, E>` without wrapping it in an `Option`. `detach()` explicitly transfers lifetime management to the executor and discards output observation; detached work remains executor-managed until completion or executor shutdown. `AsyncResource` instead owns its task handle and cancels it when the resource is dropped. Cancellation cannot roll back prior writes or external side effects, and cannot interrupt a running poll. Polling a completed task again panics. `Task::fallible` converts the handle into a `FallibleTask<T>` that yields `None` instead of panicking when the executor stops the task, for example when its thread exits; `detach_and_report` uses this path, so a stopped task is silent. Dropping the handle is cancellation, but it consumes the handle, so it cannot be awaited afterwards.

`tick(limit)` drives a bounded number of ready runnables without waiting for external events. `try_tick(limit)` returns `Err(ExecutorClosed)` instead of panicking when the executor is shutting down or the thread-local executor is already destroyed. `shutdown()` stops accepting new tasks: `try_spawn` returns `Err(ExecutorClosed)`, `spawn` panics, and existing tasks continue until they complete or the executor is dropped. `try_spawn` is safe to call from a future destructor during shutdown; `spawn` still panics, and a panic in a TLS destructor aborts. The Macroquad adapter calls `tick` after timeouts and before layout. The limit is a count of polls, not a time guarantee: one blocking call or long CPU loop still blocks the UI thread. Blocking work is provided by `spawn_blocking` (see [Blocking tasks](#blocking-tasks)); there is no built-in I/O reactor, Tokio context, or automatic retry, so I/O should run on the host's runtime and be awaited as a foreign future; timers are provided by `sleep`/`timeout` and fired by `tick` (see [Timers](#timers)). `App::frame` checks input interruptions before timeouts/tasks, then lays out and draws the UI before pointer dispatch. It does not call `begin_default_pass`/`end_pass`; macroquad batching manages passes, and raw miniquad hosts manage their own external passes. `App::frame_paused` first discards UI input and cancels pointer/capture/gesture state, then advances timeouts and tasks without layout or rendering. Use futures whose wake sources are independently driven. The host wake callback requests a later frame; it must not synchronously poll tasks. It is stored in the executor's wake slot, so a callback installed after a task is spawned is still observed. `App::new` installs the Macroquad callback before materializing its root builder; tasks started before that need the callback installed explicitly when using a blocking event loop.

## Timers

`sleep(duration)` returns a `Sleep` future that completes when the executor's clock reaches the deadline. `timeout(duration, future)` polls the inner future and the sleep together and yields `Result<T, Elapsed>`. Timers are driven by `tick`: each tick drains due deadlines before it takes its ready snapshot, so a task woken by a timer can run in the same tick. `App::frame` already calls `tick` every frame, so a timer fires on the first frame after its deadline. `next_timer_deadline()` returns the earliest pending deadline, or `None`, so an idle host can park until then and call `tick`.

```rust
use mue_core::{prelude::*, task::tick};
use std::time::Duration;

async fn example() {
    let task = spawn(async {
        sleep(Duration::from_millis(16)).await;
        timeout(Duration::from_secs(1), async { 42 }).await
    });
    tick(32);
    assert!(!task.is_finished());
}
```

`Sleep` and `Timeout` are `!Send`: they register with the current thread's executor and cannot be used from a background task. A background operation that needs a timer should use a blocking pool (not implemented) or a separate host timer. A host that stops ticking (for example an idle blocking event loop) must arrange a wake at the deadline; the current Macroquad adapter renders continuously, so the frame loop is enough.

Install a clock before creating timers to make them deterministic in tests:

```rust
use mue_core::task::{set_clock, sleep, spawn, tick};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

let clock = Arc::new(Mutex::new(Instant::now()));
let clock_for_clock = clock.clone();
set_clock(Some(Arc::new(move || *clock_for_clock.lock().unwrap())));
let task = spawn(async { sleep(Duration::from_millis(10)).await });
tick(1);
*clock.lock().unwrap() += Duration::from_millis(10);
tick(1);
assert!(task.is_finished());
set_clock(None);
```

Dropping a `Sleep` before its deadline leaves its timer entry in the executor until the deadline; waking the stale waker is harmless. There is no `interval`/debounce helper yet.

## Joining tasks

`JoinSet<T>` owns foreground tasks and joins them. `spawn` adds a task; `join_next` yields outputs as tasks complete; `join_all` drains the set and returns outputs in completion order. Dropping the set, or the `join_all` future, cancels the remaining tasks. `JoinSet` is foreground-only and uses the same `tick` budget; it does not create a background pool.

```rust
use mue_core::{prelude::*, task::{JoinSet, tick}};

let mut set = JoinSet::new();
set.spawn(async { 1u32 });
set.spawn(async { 2u32 });
let task = spawn(async move { set.join_all().await });
tick(32);
```

## Blocking on a future

`block_on(future)` drives the current thread's tasks and timers with `tick` while the future is pending, parking between polls. `block_with_timeout(duration, future)` returns `None` on wall-clock timeout and drops the future. These are test/headless helpers; do not call them on the UI thread while the frame loop is running. `block_on` installs a temporary executor wake callback that unparks the thread and restores the previous callback when it returns. It is not reentrant with `tick`.

```rust
use mue_core::{prelude::*, task::{block_on, sleep}};
use std::time::Duration;

let task = spawn(async {
    sleep(Duration::from_millis(1)).await;
    7u32
});
assert_eq!(block_on(task), 7);
```

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
let resource = owner.run(|| AsyncResource::<String, String>::new(async {
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

`AsyncResource` owns an independent state owner and its task handle. The result state remains alive after the operation completes, until the resource is dropped; ending the configuration scope does not dispose it. `new` runs the operation on the foreground executor and accepts a non-`Send` future; if that future only awaits a foreign handle (for example a host I/O task), the actual work happens on the host's runtime. `new_blocking` runs a blocking closure on a fresh OS thread and publishes its result on the UI thread; use it for low-frequency file I/O or decoding. In both cases the result state is published on the UI thread by a foreground task owned by the resource. The foreground operation follows plain `spawn` semantics, with no inherited context or automatic resource owner, unless the caller passes `scope_future(operation)`. A blocking operation cannot use `scope_future` because the closure is `Send`; if it needs operation-local reactive resources, keep the operation on the foreground or split it into blocking loading plus a foreground apply step. That wrapper's operation-local resources are released at completion, while the resource's result state persists. The caption projection above belongs to `owner`, so retain both owners for as long as the UI uses the binding.

`Task<Result<T, E>>::detach_and_report(|error, location| { ... })` explicitly observes a business failure once and supplies the reporting call site. Its reporter runs as a plain task with no automatic scope or inherited context, detached from reactive-scope lifetimes. Code captured by the reporter must not assume ordinary signal/scope handles remain alive; resources it creates need explicit ownership or disposal. Cancellation is silent. If the executor is already shutting down, the reporter is not spawned. Plain `detach()` intentionally discards all output.

`AsyncResource` retains the original typed error as reactive state and does not report it automatically. Handle it in the UI or explicitly report it at the application boundary. Replacing the resource cancels the old operation; different resource instances do not share a result destination. Reusing a separate destination across requests still requires request identity/version checks.

Panics propagate from polling. There is no task panic isolation, `catch_unwind`, state rollback, or promise that the runtime can be reused after a panic. mue does not provide a panic unwind UI or crash screen: catching the panic and rendering any fallback screen is the host's responsibility, and that screen must not use mue's runtime. `tick`'s `in_tick` flag and the task isolation tracker/`current_effect` are restored on unwind, so a caught poll panic does not permanently wedge the executor; the ambient scope may still be stale. Ordinary RAII cleanup and task cancellation are supported; panic recovery is not.

## Known limitations

These are the remaining known edge cases in the current implementation. They are
documented rather than fully fixed; the framework's own code does not exercise
them.

- **`spawn`/`tick` still panic if called from a future destructor during executor
  shutdown.** `Executor::drop` runs after the executor TLS is marked destroyed.
  A future destructor that calls `spawn`/`tick` there panics inside async-task's
  `abort_on_panic`, which aborts the process. Use `try_spawn`/`try_tick` in
  destructors and other shutdown paths: they return `Err(ExecutorClosed)` without
  touching the destroyed TLS. The framework's own destructors do not submit new
  work.
- **A `ScopedFuture` must not outlive the reactive runtime.** The wrapper is
  bound to the current thread's runtime. If it is stored in a TLS slot (or
  another owner) that outlives the runtime, its `Drop` calls
  `owner.run(...)`, which asserts the scope is still alive; after runtime
  teardown this panics, and a panic in a TLS destructor usually aborts. Keep
  the wrapper in a local or task-owned value that is dropped before the
  runtime. A best-effort leak guard in `Drop` is a possible future change, not
  implemented.
- **Panic recovery is not supported.** A caught task poll panic restores
  `tick`'s `in_tick` flag and the task isolation tracker/`current_effect`/
  `UpdateContext`, but the ambient scope may still be stale and the panicking
  task's output is lost.
  Do not treat a task panic as a recoverable error path; the panic-fatal policy
  still applies.
- **Timers are foreground-only and frame-driven.** `Sleep`/`Timeout` are `!Send`
  and register with the current thread's executor. A blocking or idle host must
  arrange a wake at the deadline; the current Macroquad adapter renders
  continuously. Dropping a `Sleep` leaves its timer entry until the deadline.
  There is no `interval`/debounce helper yet.
- **`JoinSet` is foreground-only.** It returns outputs in completion order and
  uses the same `tick` budget; dropping the set or the `join_all` future cancels
  the remaining tasks.
- **`spawn_blocking` is one thread per call.** There is no bounded blocking pool
  or backpressure, and dropping the handle does not cancel or join the closure.
  A panic in the closure is rethrown when the handle is awaited.
- **`block_on`/`block_with_timeout` are test/headless helpers.** They park the
  calling thread, install a temporary executor wake callback, and must not run
  concurrently with a UI frame loop. `block_on` is not reentrant with `tick`.

# Blocking tasks

`spawn_blocking(closure)` runs a `Send + 'static` closure on a fresh OS thread and returns a `BlockingTask<T>`. Use it for low-frequency file I/O, image decoding, and other blocking work that would occupy an async worker. The closure must not access signals, scopes, context, or UI-thread resources; the thread is marked as a background worker, so reactive access panics there. For frequent blocking work, use the host's blocking pool instead of one thread per call.

`BlockingTask<T>` is a future yielding `T`; `.await` returns the closure's output. Dropping the handle does not cancel or join the thread: the closure runs to completion and the result is discarded. `detach()` is the explicit no-observe form. A panic in the closure wakes the handle; awaiting it panics on the awaiting thread. Each call creates one OS thread named `mue-blocking-N`; `spawn_blocking` panics if the thread cannot be spawned.

```rust
use mue_core::{prelude::*, task::spawn_blocking};

let task = spawn_blocking(|| 21u32 * 2);
let publisher = spawn(async move { task.await });
// App::frame ticks the publisher.
```

There is no bounded blocking pool or backpressure: one call is one thread. A blocking closure cannot be cancelled; dropping the handle only discards the result.

## AsyncResource

`AsyncResource::new` packages the foreground publisher for the common loading case. `new_blocking` does the same for a blocking closure. The `Loading` / `Ready(Result<T, E>)` state is published on the UI thread.

~~~rust
use mue_core::{prelude::*, task::{AsyncResource, AsyncState}};

let resource = AsyncResource::<String, String>::new(async {
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

`AsyncResource::new` runs the operation on the foreground executor; `new_blocking` runs a blocking closure on a fresh OS thread. For network or other host-runtime I/O, keep the I/O on the host runtime and await its handle from `new` (for example `AsyncResource::new(async move { handle.await })`). `AsyncResource` retains the original typed error as reactive state and does not report it automatically.

`reload_with(future)` replaces the current operation: it cancels the previous publisher, returns the state to `Loading`, and only the latest operation publishes its result. `reload_blocking(closure)` is the blocking counterpart. `AsyncState` exposes `is_loading`, `value`, `error`, `map`, and `map_err`.

The playground uses this pattern for its bundled font database: `fonts::database()` runs on a blocking thread, and the main loop calls `font::initialize` on the UI thread once the state is `Ready`, replacing a text-free loading screen with the real UI. Its graphics and input pages also decode PNGs to an `Image` on a blocking thread, then create the GPU texture from a foreground effect.

## Cancellation and lifecycle

Dropping an `AsyncResource` cancels its foreground publisher and disposes the state. Dropping a `BlockingTask` does not cancel or join the closure; the closure runs to completion and the result is discarded. There is no progress channel and no background timer; use the host's runtime for those.

## Panics

Panics propagate from polling. A blocking closure's panic is rethrown when the handle is awaited. There is no panic recovery or state rollback.

## Limitations

- Blocking work uses `spawn_blocking` (one OS thread per call, no bounded pool or backpressure). There is no progress channel or background timer. Do not run long blocking work on the UI thread; use `spawn_blocking` or the host's blocking pool.
- `AsyncResource::new_blocking` requires `T: Send` and `E: Send`. UI-only resources such as textures should be created on the foreground after loading `Send` bytes.
- Awaiting a host-runtime future on the foreground executor is fine only when the future is cheap to poll (for example a `JoinHandle`); do not poll I/O futures that require the host runtime's context on a bare worker thread.
