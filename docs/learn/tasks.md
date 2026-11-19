# Asynchronous work

[Learn contents][learn-index]

Keep the frame loop responsive while a forecast request runs. Start by drawing all three result states, then choose where the real work should run.

## Render pending, ready, and failed results

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use mue::core::{
    prelude::{AsyncResource, AsyncState},
    task::sleep,
};
use mue::prelude::*;
use std::{rc::Rc, time::Duration};

let owner = OwnedScope::new();
let mut forecast = AsyncResource::<u32, String>::new(async {
    sleep(Duration::from_secs(1)).await;
    Ok(18) // Stand-in for an async forecast request.
});
let caption = owner.run(|| forecast.state().map(|state| Rc::<str>::from(match state {
    AsyncState::Loading => "Checking forecast…".to_owned(),
    AsyncState::Ready(Ok(temp)) => format!("Forecast   {temp} °C"),
    AsyncState::Ready(Err(error)) => format!("Forecast unavailable ({error})"),
})));
let view = text(caption).style(style! { font-size: 24px; color: #183c2e; });
```

</div>
<figure class="mue-demo-preview mue-preview-pair">

![Checking forecast while the timer is pending][shot-forecast-loading]

![Forecast at 18 degrees after completion][shot-forecast-ready]

</figure>
</div>

Pass `view` to `App::new` and run the usual frame loop. Keep `forecast` and `owner` alive while the view uses `caption`. The resource owns the operation and result. The scope owns the caption binding. Change `Ok(18)` to `Err("offline".into())` to try the error state. The `sleep` future waits without occupying the UI thread. Keep ticking `App::frame()` so the timer can wake the task and the resource can publish the result.

## Refresh the existing resource

When settings change, replace the resource's pending operation. Keep the same resource so the view can keep its state binding. If the previous request finishes later, its cancelled publisher cannot overwrite the new result.

```rust,no_run
# use mue::core::prelude::AsyncResource;
# let mut forecast = AsyncResource::<u32, String>::new(async { Ok(18) });
forecast.reload_with(async { Ok(21) });
```

Try refreshing while the one-second timer is still running. The label can show the replacement result as soon as the App polls it.

## Choose where work runs

| Need | Use |
| --- | --- |
| Update a view from a future | `AsyncResource::new` and `state()` |
| Keep or await a task handle | `spawn` |
| Read a file or decode bytes with a blocking API | `spawn_blocking` or `AsyncResource::new_blocking` |
| Build a texture from loaded bytes | Do that step on the UI thread after the worker returns |

`App::frame` polls foreground tasks and timers. A host without an App can drive them with `tick` and `tick_timeouts`. Sleep and interval need those ticks to advance. Use the host's runtime for I/O that depends on it and await its result on the foreground.

## Handle task lifetimes

- Keep a `Task` handle to await its output. Dropping it requests cancellation. Use `detach()` to let it run and discard the result, or `detach_and_report` to report a business error.
- Plain tasks run outside the reactive scope and effect tracker. Capture owned values or live handles that have an owner. Use `try_set` on a direct signal if its owner might disappear before publication.
- To give a future scoped resources, use `spawn(scope_future(future))`. The operation has its own owner and can look up parent context while the parent lives. Calling `owner.run(|| async { ... })` scopes construction of the future only.
- Use `reload_with` or `reload_blocking` to replace a resource's operation. The resource returns to `Loading` and accepts only the replacement publisher's output.

## Take blocking work off the frame loop

`spawn_blocking` starts one OS thread per call. Send it owned input and return owned data. Use a host pool for frequent jobs. A worker cannot read signals, access reactive context, or use GPU state. Dropping its handle discards the result while the worker continues to completion. A worker panic propagates to the code awaiting its handle.

The previous forecast example uses a timer. To read a real file, move its path into a blocking closure and keep the result mapping on the UI thread. Mount `view` before requesting a reload and keep both owners alive.

```rust,no_run
use std::path::PathBuf;
use mue::{core::task::{AsyncResource, AsyncState}, prelude::*};

let path = PathBuf::from("release.txt");
let mut file = AsyncResource::new_blocking(move || {
    std::fs::read_to_string(path).map_err(|error| error.to_string())
});
let owner = OwnedScope::new();
let summary = owner.run(|| file.state().map(|state| match state {
    AsyncState::Loading => "Opening file…".into(),
    AsyncState::Ready(Ok(contents)) => format!("{} lines", contents.lines().count()),
    AsyncState::Ready(Err(error)) => format!("Open failed: {error}"),
}));
let view = text(summary);

// In the host, when a different file is selected:
let selected = PathBuf::from("next-release.txt");
file.reload_blocking(move || {
    std::fs::read_to_string(selected).map_err(|error| error.to_string())
});
```

Load a file that exists, then reload with a missing path. The same label follows both requests because `reload_blocking` keeps the state signal. For a complete editor that also writes the file, see the [release checklist project][release-checklist-guide].

See [task execution and ownership][tasks-reference] for timer policies, awaiting and joining, executor shutdown, and manual driving.

[task-api]: https://github.com/void-cat-0/mue/blob/main/mue-core/src/task.rs

[shot-forecast-loading]: ../images/forecast-loading.png
[shot-forecast-ready]: ../images/forecast-ready.png


**Previous** [Input routing][scene-input-guide] · **Next** [Localized interfaces][i18n-guide]

[learn-index]: index.md
[scene-input-guide]: input.md
[i18n-guide]: i18n.md
[release-checklist-guide]: ../example-app.md
[tasks-reference]: ../reference/tasks.md
