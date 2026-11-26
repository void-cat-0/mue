# Your first window

[Learn contents][learn-index]

In this chapter you open a window, show a countdown, and make a button that
starts and pauses it. By the end you will know the three things every mue app
is made of: a view built from small pieces, state that the view reads, and a
loop that draws frames.

## Create the project

Make a new binary crate and add mue:

```toml
[dependencies]
mue = { git = "https://github.com/void-cat-0/mue", branch = "main" }
```

mue draws with Macroquad. Use the copy that mue re-exports (`mue::macroquad`)
so your app and the renderer always agree on the version.

## Show something

Replace `src/main.rs` with this and run `cargo run`:

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use mue::{App, macroquad, prelude::*};

#[macroquad::main("Focus")]
async fn main() {
    let view = center()
        .style(style! { width: 100%; height: 100%; background: #f4efe6; })
        .children(text("25:00").style(style! {
            font-size: 64px;
            color: #2d2a26;
        }));

    let mut app = App::new(view);
    loop {
        app.frame();
        macroquad::window::next_frame().await;
    }
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![The text 25:00 centered on a warm beige window][shot-guide-window]

</figure>
</div>

A view is a tree of **builders**. `center()` is a box that centers its
children, and `text(...)` is a label. Text is white unless you set its `color`,
which is why every label in this guide sets one. You configure a builder with methods such
as `.style(...)` and `.children(...)`. The `style!` macro reads like CSS:
`width: 100%` means the full width of the parent, which for the root is the
window.

`App` owns the view. Create it once, then call `app.frame()` every frame: it
handles input, lays out the tree, and draws it. `next_frame().await` lets
Macroquad show the result and wait for the next frame.

Try resizing the window: the text stays in the middle, because the root box
follows the window size.

## Make it interactive

A countdown needs something that changes. In mue, changing values live in
**signals**. Move the view into a component and add a Start button:

```rust
use mue::prelude::*;

#[component]
fn timer() -> impl Component {
    let running = signal(false);

    vstack().gap(16.).style(style! { align-items: center; }).children((
        text("25:00").style(style! { font-size: 64px; color: #2d2a26; }),
        button(running.pick("Pause", "Start"))
            .on_activate(move || running.update(|on| *on = !*on)),
    ))
}
```

In `main`, pass `center().children(timer())` to `App::new` (keep the style from
before).

Three things happen here:

- `signal(false)` creates a value that can change later.
- `running.pick("Pause", "Start")` creates a label that *follows* the signal:
  "Pause" while it is true, "Start" while it is false.
- `on_activate` runs when the button is clicked, tapped, or pressed with Enter
  or Space while focused. It flips the signal, and the label updates by itself.

`#[component]` turns the function into a builder you can style and place like
any other. The function body runs **once**, when the view is built. That is why
you use `pick` (or `map`, below) instead of an `if`: an `if running.get()` in
the body would choose a label once and never change it.

## Count down

Now let the timer run. Keep the remaining time in a second signal and subtract
a second while the timer runs:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{components::use_interval, prelude::*};
use std::rc::Rc;

#[component]
fn timer() -> impl Component {
    let running = signal(false);
    let remaining = signal(25 * 60_u32);

    use_interval(1., move || {
        if running.get() && remaining.get() > 0 {
            remaining.update(|seconds| *seconds -= 1);
        }
    });

    let clock = remaining.map(|seconds| {
        Rc::<str>::from(format!("{:02}:{:02}", seconds / 60, seconds % 60))
    });

    vstack().gap(16.).style(style! { align-items: center; }).children((
        text(clock).style(style! { font-size: 64px; color: #2d2a26; }),
        button(running.pick("Pause", "Start"))
            .primary(true)
            .on_activate(move || running.update(|on| *on = !*on)),
    ))
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![The timer showing 24:57 above a blue Pause button][shot-guide-timer]

</figure>
</div>

`use_interval(1., tick)` calls `tick` every second for as long as the component
exists. `remaining.map(...)` turns the number of seconds into the text
`mm:ss`; like `pick`, it updates whenever the signal changes.

Press **Start** and watch the clock count down. Press **Pause** and it stops.

## What you learned

- A view is a tree of builders configured with methods and `style!`.
- Signals hold values that change; `map` and `pick` derive values that follow
  them.
- A component's body runs once. Anything that should update later must be a
  signal or derived from one.
- `App::frame()` does a whole frame: input, layout, and drawing.

In the next chapter the timer remembers the sessions you finish.

**Previous** [Learn][learn-index] · **Next** [State and lists][basics-guide]

[shot-guide-window]: ../images/guide-window.png
[shot-guide-timer]: ../images/guide-timer.png

[learn-index]: index.md
[basics-guide]: basics.md
