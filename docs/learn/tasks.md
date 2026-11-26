# Background work

[Learn contents][learn-index]

Focus forgets its history when you close it. In this chapter you load the
history from a file when the app starts and save it after each session. Files
can be slow, and a frame that waits for one would freeze the window, so the
reading and writing happen in the background while the UI keeps drawing.

## Load once, show the state

An `AsyncResource` runs a job and exposes its progress as a signal:
`Loading` while it runs, then `Ready` with the job's `Result`.
`new_blocking` runs a blocking job, such as reading a file, on a worker thread:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{core::task::{AsyncResource, AsyncState}, prelude::*};
use std::rc::Rc;

fn load_history() -> Result<Vec<u32>, String> {
    let text = std::fs::read_to_string("focus-history.txt").map_err(|e| e.to_string())?;
    Ok(text.lines().filter_map(|line| line.trim().parse().ok()).collect())
}

#[component]
fn history_status() -> impl Component {
    let history = AsyncResource::new_blocking(load_history);
    let state = history.state();
    // The closure owns the resource, so the job lives as long as the label.
    let label = state.map(move |state| {
        let _ = &history;
        Rc::<str>::from(match state {
            AsyncState::Loading => "Loading your history…".to_owned(),
            AsyncState::Ready(Ok(minutes)) => format!(
                "{} sessions, {} minutes in total",
                minutes.len(),
                minutes.iter().sum::<u32>()
            ),
            AsyncState::Ready(Err(_)) => "No history yet".to_owned(),
        })
    });
    text(label).style(style! { font-size: 18px; color: #2d2a26; })
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![The text "3 sessions, 75 minutes in total"][shot-guide-tasks]

</figure>
</div>

The label first reads "Loading your history…", and changes as soon as the file
has been read; the window keeps drawing in between. Dropping a resource cancels
its job, so keep it somewhere that lives as long as you need the result; here
the label's closure owns it. The result comes back on
the UI thread, so updating signals from it is safe. A missing file is just an
`Err` that the view shows as "No history yet".

## Save without waiting

Saving works the same way. Keep the resource in the component and start a new
job with `reload_blocking` each time a session ends; the state goes back to
`Loading` and then to the new result:

```rust
use mue::{core::task::{AsyncResource, AsyncState}, prelude::*};
use std::rc::Rc;

#[component]
fn save_button(minutes: u32) -> impl Component {
    let mut save = AsyncResource::<(), String>::new_blocking(|| Ok(()));
    let status = save.state().map(|state| {
        Rc::<str>::from(match state {
            AsyncState::Loading => "Saving…",
            AsyncState::Ready(Ok(())) => "Saved",
            AsyncState::Ready(Err(_)) => "Could not save",
        })
    });
    let save = std::cell::RefCell::new(save);
    hstack().gap(12.).style(style! { align-items: center; }).children((
        button("Save session").on_activate(move || {
            let line = format!("{}\n", minutes.get());
            save.borrow_mut().reload_blocking(move || {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("focus-history.txt")
                    .map_err(|e| e.to_string())?;
                file.write_all(line.as_bytes()).map_err(|e| e.to_string())
            });
        }),
        text(status).style(style! { font-size: 14px; color: #8a8178; }),
    ))
}
```

Starting a new job replaces the previous one; if an older job finishes later,
its result is ignored. The button's handler owns the resource, so when the
component goes away the resource goes with it and late results are dropped.

## Async code and timers

`AsyncResource::new` takes a future instead of a closure, for code that awaits,
such as a network request. Inside async code, `mue::core::task::sleep` waits
without blocking the frame. For small delays in the UI, `set_timeout(seconds,
action)` runs an action later, and `use_interval`, which you used in chapter 1,
repeats one.

## What you learned

- `AsyncResource` runs work in the background and reports `Loading` and
  `Ready` through a signal.
- `new_blocking` and `reload_blocking` move blocking work such as file access to
  a worker thread.
- A newer job replaces an older one, and results never arrive after their
  component is gone.

The [task reference][tasks-reference] covers cancellation, operation scopes, and
worker threads.

**Previous** [Keyboard and text input][scene-input-guide] · **Next** [Localization][i18n-guide]

[shot-guide-tasks]: ../images/guide-tasks.png

[learn-index]: index.md
[scene-input-guide]: input.md
[i18n-guide]: i18n.md
[tasks-reference]: ../reference/tasks.md
