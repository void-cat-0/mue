# Your first window

[Learn contents][learn-index]

Build a playback control in three passes. Open a window, update its state from a button, then use the same state to drive a second part of the view.

## Open a window

Run `cargo new playback-control`. Add Mue to `Cargo.toml` and use `mue::macroquad` in the application. That import uses the same Macroquad fork as the renderer.

```toml
[dependencies]
mue = { git = "https://github.com/void-cat-0/mue", branch = "main" }
```

Paste this into `src/main.rs` and run `cargo run`.

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use mue::{App, macroquad, prelude::*};

#[macroquad::main("Playback control")]
async fn main() {
    let mut app = App::new(center()
        .style(style! {
            width: 100%; height: 100%;
            background: #eaf0fc;
        })
        .children(text("Preview paused").style(style! {
            font-size: 28px; color: #172c56;
        })));

    loop {
        app.frame();
        macroquad::window::next_frame().await;
    }
}
```

</div>
<figure class="mue-demo-preview mue-playback">

![Preview paused centered on a pale blue canvas][shot-playback]

</figure>
</div>

`center()` is the root builder. Its two percentage sizes refer to the window, so resizing the window keeps the label centered. Change the font size to `40px` and resize the window to see how the label and its parent use different sizes.

Construct `App` once. It retains the built node tree across frames. Each call to `frame()` processes input and tasks, prepares layout, and draws. `next_frame().await` hands control back to Macroquad.

## Handle a button press

Add `transport()` above `main` and replace the argument to `App::new` with `transport()`. Press the button twice. The first press changes **Play** to **Pause**. The second reverses the change.

```rust
use mue::prelude::*;

#[component]
fn transport() -> impl Component {
    let playing = signal(false);
    let action = playing.pick("Pause", "Play");
    center().style(style! {
        width: 100%; height: 100%; background: #eaf0fc;
    }).children(button(action).on_activate(move || {
        playing.update(|value| *value = !*value);
    }))
}
```

The component creates `playing` when Mue builds the node. `pick` creates a reactive property. The button keeps that property and can read the new value after activation. A Rust `if playing.get()` in the component body chooses a label once, during construction. Use `if` inside `computed` or `map` when the choice must follow future writes.

## Drive two bindings from one signal

Keep the signal and the outer `center` from the previous step. Define a second property and pass a column of two children to `center().children(...)`.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
# #[component]
# fn example() -> impl Component {
# let playing = signal(false);
# let action = playing.pick("Pause", "Play");
let status = playing.pick("Preview playing", "Preview paused");
let controls = vstack().gap(16.).children((
    text(status).style(style! { font-size: 28px; color: #172c56; }),
    button(action).on_activate(move || {
        playing.update(|value| *value = !*value);
    }),
));
# controls
# }
```

</div>
<figure class="mue-demo-preview mue-playback">

![The playing status above the Pause button after activation][shot-playback-playing]

</figure>
</div>

Both labels follow the same state. To drive a real audio player, pass its current playback state to the view and use activation to request a change. Try mounting two `transport()` instances first. Each instance has its own signal. The next chapter moves state into a parent and renders a changing list.


[shot-playback]: ../images/playback.png
[shot-playback-playing]: ../images/playback-playing.png


**Previous** [Learn][learn-index] · **Next** [Reactive state and children][basics-guide]

[learn-index]: index.md
[basics-guide]: basics.md
