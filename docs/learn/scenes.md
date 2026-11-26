# Screen navigation

[Learn contents][learn-index]

A Scene can draw its own graphics and a retained Mue tree in the same frame. Build a moonrise view, then give it a notes screen that returns a result. The notes screen can leave the stack without rebuilding the moonrise state.

## Draw a Scene

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use mue::{Scene, prelude::*};
use mue::macroquad::prelude::{Color, draw_circle, screen_width};
use mue::renderer::{CallbackResult, SceneFrame, SceneRender, SceneRoot};

struct Moonrise { caption: Node }

impl Scene for Moonrise {
    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(SceneRoot::new(*self.caption, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        draw_circle(screen_width() / 2., 63., 27., Color::from_hex_rgb(0xfee6ad));
        render.draw(*self.caption)?;
        Ok(())
    }
}

let caption = center()
    .style(style! { width: 100%; height: 100%; padding-top: 92px; })
    .children(text("Moonrise").style(style! {
        font-size: 24px;
        color: #ffeece;
    }))
    .into_node();
let mut app = mue::App::with_scene(Moonrise { caption });
```

</div>
<figure class="mue-demo-preview mue-sky">

![A pale moon over the Moonrise caption on a blue canvas][shot-moonrise]

</figure>
</div>

Construct the Scene after Macroquad starts. In the main loop, call `app.frame()` and then `next_frame().await`. Clear the canvas with your chosen sky color, such as `#18283e`, before drawing.

| Callback | Use it to |
| --- | --- |
| `event` | Handle raw input, lifecycle notifications, and returned values |
| `update` | Advance screen or gameplay state |
| `layout` | Submit node roots with bounds and input policy |
| `render` | Draw your content and the submitted roots in order |

Store each `Node` on the Scene. Submit a borrow-like `NodeRef` in layout and call `render.draw` with that root in render. Use paint order in `render`: the last root you draw gets the first chance at eligible Node input in the next frame. The order of `roots.push(...)` does not set input stacking.

## Push a notes screen

When you push a Scene, Mue pauses the old top and keeps it on the stack. Only the new top updates and draws. Popping it drops the child and resumes the retained parent.

```rust
use mue::{Scene, renderer::SceneContext};

struct TrailNotes;
impl Scene for TrailNotes {}

fn open_notes(ctx: &mut SceneContext<'_>) {
    ctx.push(TrailNotes);
}

fn save_notes(ctx: &mut SceneContext<'_>) {
    ctx.pop_with_result(String::from("Trail clear"));
}
```

Store the last note on the retained parent. In its `event` callback, inspect a returned value before it handles `Resume`.

```rust
use mue::renderer::SceneEvent;

fn latest_note(event: SceneEvent<'_>, note: &mut Option<String>) {
    if let SceneEvent::Returned(result) = event {
        if let Ok(saved) = result.downcast::<String>() {
            *note = Some(*saved);
        }
    }
}
```

Return owned data because the child drops before the parent consumes the result. If you receive several requests before the frame selects one, the last replaces the others. To animate a selected switch, attach an effect to its request. See [Scene transitions][scene-transitions-guide].

## Handle a frame error

Use `try_frame` when you want to handle errors from Scene callbacks. A failed frame clears input eligibility and faults the App. Replace the failed top with `replace_scene` or abandon the stack with `reset_scene`. Keep recovery explicit. For callback signatures, enter and resume ordering, suspended frames, root input modes, and recovery, see [Scene lifecycle and input][scenes-reference].


[input-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/input.rs
[renderer-api]: https://github.com/void-cat-0/mue/tree/main/mue-macroquad/src

[shot-moonrise]: ../images/moonrise.png

## Scene transitions

Crossfade from a forest path to a sunlit clearing, then compare that switch with an effect that covers the outgoing scene before the stack commits.

### Blend two Scene pictures

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{App, Scene};
use mue::renderer::{SceneTransition, motion::transition::Transition};

fn reveal_clearing(app: &mut App, clearing: impl Scene + 'static) {
    app.replace_scene(clearing)
        .transition(SceneTransition::crossfade(Transition::linear(0.6)));
}
```

</div>
<figure class="mue-demo-preview mue-sky">

<div class="mue-motion">

![Forest path blends into Sunlit clearing over 600 milliseconds.][shot-crossfade]

</div>
<div class="mue-still">

![Sunlit clearing at the end of the crossfade][shot-crossfade-end]

</div>

</figure>
</div>

Call `reveal_clearing` from the host while the path is the active Scene. Keep calling `App::frame()` and `next_frame().await`. The request queues the switch. Mue captures the path, installs the clearing, and blends from the capture into the live clearing. The outgoing Scene can leave the stack while its pixels are still visible. Picture effects require OpenGL and the default full-window target and camera.

### Choose the switch point

| Want to | Use |
| --- | --- |
| Blend an outgoing picture with the new Scene | `crossfade(curve)` |
| Draw your own picture blend or slide | `composite(curve, callback)` |
| Draw an effect over the live Scene | `overlay(timing, callback)` |

`AfterSwitch` commits the stack before the animation. With `AroundSwitch { exit, enter }`, Mue first draws the exit endpoint over the old Scene, commits at the next normal safe boundary, and then runs the enter segment. Use that form for an overlay that covers the old screen before revealing the new one. Overlays need no full-screen picture textures.

### Fade a cover through the switch

A cover uses rising opacity before the switch and falling opacity afterward. In an overlay callback, draw in logical viewport coordinates.

```rust
use mue::macroquad::prelude::{Color, draw_rectangle};
use mue::renderer::{SceneTransition, TransitionPhase, TransitionTiming, motion::transition::Transition};

fn mist_cover() -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.2),
            enter: Transition::ease_out(0.3),
        },
        |frame| {
            let opacity = match frame.phase {
                TransitionPhase::BeforeSwitch => frame.progress,
                TransitionPhase::AfterSwitch => 1. - frame.progress,
            };
            let rect = frame.viewport;
            let mist = Color::new(0.91, 0.94, 0.92, opacity);
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, mist);
            Ok(())
        },
    )
}
```

Attach the cover to a request from the old Scene. For example, change the helper in the first section to use the cover instead of the crossfade.

```rust
use mue::{App, Scene, renderer::SceneTransition};

fn reveal_under_cover(app: &mut App, clearing: impl Scene + 'static, cover: SceneTransition) {
    app.replace_scene(clearing).transition(cover);
}
```

Pass `mist_cover()` as `cover`. At the end of the exit segment the outgoing Scene is still active and the mist is opaque. The next normal frame commits the request and draws the new Scene under the mist. See the commit timeline in the [transition reference][scene-transitions-reference].

### Hold input until the new screen is ready

- A blend changes what you see. Input still follows the [Scene input contract][scene-input-guide]. Submit `RootInput::None` while a new root should draw without receiving presses, and gate raw gameplay input in the Scene.
- Draw with owned assets in an effect. A handle to a signal or node cannot keep the outgoing owner alive. Composite pictures are borrowed for that draw only.
- A newer request can cancel an effect. An already committed stack change stays committed. Host suspension freezes transition time.
- Install a fresh-effect factory with `set_default_scene_transition`. Override it per request with `.transition(...)` or `.without_transition()`.

Use `App::transition_state()` to inspect the presented progress. See [Scene transition timing and composition][scene-transitions-reference] for sample fields, endpoint acknowledgement, custom picture composition, interruption, and error behavior.


[transition-api]: https://github.com/void-cat-0/mue/blob/main/mue-macroquad/src/scene_transition.rs

[shot-crossfade]: ../images/crossfade.gif
[shot-crossfade-end]: ../images/crossfade-end.png


**Previous** [Shared appearance][themes-guide] · **Next** [Input routing][scene-input-guide]

[learn-index]: index.md
[themes-guide]: themes.md
[scene-transitions-guide]: scenes.md#scene-transitions
[scene-input-guide]: input.md
[scenes-reference]: ../reference/scene.md
[scene-transitions-reference]: ../reference/scene.md#scene-transitions
