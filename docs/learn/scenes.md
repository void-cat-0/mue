# Screens

[Learn contents][learn-index]

Focus needs a settings screen where you choose the session length. In this
chapter you turn the timer into a **scene**, push a settings scene on top of
it, and bring the chosen length back when settings closes. The timer keeps its
state the whole time.

## What a scene is

So far you passed a view to `App::new`. That creates one scene behind the
scenes. A scene is a screen with its own lifecycle: it is told when it starts,
pauses, and resumes, it can update every frame, and it decides what to draw.
The App keeps a **stack** of scenes. Only the top one runs; the ones below are
kept, with all their state, until the top is removed.

A scene implements the `Scene` trait. All methods have defaults, so you only
write the ones you need:

| Method | Called |
| --- | --- |
| `event` | When the scene starts, pauses, resumes, receives input, or gets a result from a scene it pushed. |
| `update` | Once per frame while the scene is on top. Request screen changes here. |
| `layout` | Each frame, to list the views (roots) the scene shows. |
| `render` | Each frame, to draw those views and anything else. |

## The timer scene

The timer scene owns its view and two signals: the session length, and a flag
that the Settings button sets. In `update` it checks the flag and pushes the
settings scene:

```rust
use mue::{Scene, prelude::*};
use mue::renderer::{
    CallbackResult, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
};
use std::rc::Rc;

struct TimerScene {
    view: Node,
    minutes: Signal<u32>,
    open_settings: Signal<bool>,
    _owner: OwnedScope,
}

impl TimerScene {
    fn new() -> Self {
        let owner = OwnedScope::new();
        let (view, minutes, open_settings) = owner.run(|| {
            let minutes = signal(25_u32);
            let open_settings = signal(false);
            let label = minutes.map(|m| Rc::<str>::from(format!("{m}:00")));
            let view = vstack()
                .style(style! {
                    width: 100%; height: 100%; gap: 16px;
                    align-items: center; justify-content: center;
                    background: #f4efe6;
                })
                .children((
                    text(label).style(style! { font-size: 56px; color: #2d2a26; }),
                    button("Settings").on_activate(move || open_settings.set(true)),
                ))
                .into_node();
            (view, minutes, open_settings)
        });
        Self { view, minutes, open_settings, _owner: owner }
    }
}

impl Scene for TimerScene {
    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        if self.open_settings.get_untracked() {
            self.open_settings.set(false);
            ctx.push(SettingsScene::new(self.minutes.get_untracked()));
        }
        Ok(())
    }

    fn event(&mut self, event: SceneEvent<'_>, _ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        if let SceneEvent::Returned(result) = event
            && let Ok(minutes) = result.downcast::<u32>()
        {
            self.minutes.set(*minutes);
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(SceneRoot::new(*self.view, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        render.draw(*self.view)?;
        Ok(())
    }
}
# struct SettingsScene;
# impl SettingsScene { fn new(_: u32) -> Self { Self } }
# impl Scene for SettingsScene {}
```

Start the app with `App::with_scene(TimerScene::new())` instead of
`App::new(view)`. The `OwnedScope` keeps the signals alive as long as the
scene exists; when the scene is dropped, they are dropped with it.

## The settings scene

The settings scene shows a length picker. **Done** removes it from the stack
and hands the chosen length back with `pop_with_result`:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{Scene, prelude::*};
use mue::renderer::{CallbackResult, SceneContext, SceneFrame, SceneRender, SceneRoot};
use std::rc::Rc;

struct SettingsScene {
    view: Node,
    minutes: Signal<u32>,
    done: Signal<bool>,
    _owner: OwnedScope,
}

impl SettingsScene {
    fn new(current: u32) -> Self {
        let owner = OwnedScope::new();
        let (view, minutes, done) = owner.run(|| {
            let minutes = signal(current);
            let done = signal(false);
            let label = minutes.map(|m| Rc::<str>::from(format!("Session length: {m} min")));
            let view = vstack()
                .style(style! {
                    width: 100%; height: 100%; gap: 16px;
                    align-items: center; justify-content: center;
                    background: #fffaf3;
                })
                .children((
                    text(label).style(style! { font-size: 22px; color: #2d2a26; }),
                    hstack().gap(8.).children((
                        button("15").on_activate(move || minutes.set(15)),
                        button("25").on_activate(move || minutes.set(25)),
                        button("50").on_activate(move || minutes.set(50)),
                    )),
                    button("Done").primary(true).on_activate(move || done.set(true)),
                ))
                .into_node();
            (view, minutes, done)
        });
        Self { view, minutes, done, _owner: owner }
    }
}

impl Scene for SettingsScene {
    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        if self.done.get_untracked() {
            ctx.pop_with_result(self.minutes.get_untracked());
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        roots.push(SceneRoot::new(*self.view, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        render.draw(*self.view)?;
        Ok(())
    }
}
```

</div>
<figure class="mue-demo-preview mue-focus-card">

![The settings screen with 50 minutes chosen and a Done button][shot-guide-settings]

</figure>
</div>

When settings pops, the timer scene receives `SceneEvent::Returned` with the
value, then `SceneEvent::Resume`. Results are boxed `Any` values; downcast them
to the type you sent. `ctx.replace(scene)` swaps the top scene instead of
stacking a new one, and `ctx.pop()` closes without a result.

A scene can draw more than nodes: anything you draw with Macroquad in `render`
appears in order with the views you `render.draw`. That is how a game draws its
world under a menu.

## Scene transitions

Screen changes can animate. Attach a transition to the request:

```rust
use mue::{Scene, renderer::{SceneContext, SceneTransition, motion::transition::Transition}};

fn open_with_fade(ctx: &mut SceneContext<'_>, settings: impl Scene + 'static) {
    ctx.push(settings)
        .transition(SceneTransition::crossfade(Transition::ease_out(0.25)));
}
```

`crossfade` blends a picture of the old screen into the new one. `composite`
lets you draw your own blend (for example a slide), and `overlay` draws an
effect such as a fade to black over the live screen, switching scenes when the
cover is opaque. `App::set_default_scene_transition` applies one effect to
every change. The [scene reference][scene-transitions-reference] explains the
timing in detail.

## What you learned

- A scene is a screen with a lifecycle; the App keeps a stack of them.
- Push with `ctx.push`, close with `ctx.pop_with_result`, and read the result
  from `SceneEvent::Returned`.
- Keep a scene's signals in an `OwnedScope` owned by the scene.
- Transitions attach to a request with `.transition(...)`.

The [scene reference][scenes-reference] covers every lifecycle event, input
routing between scenes, and error recovery.

**Previous** [Themes][themes-guide] · **Next** [Keyboard and text input][scene-input-guide]

[shot-guide-settings]: ../images/guide-settings.png

[learn-index]: index.md
[themes-guide]: themes.md
[scene-input-guide]: input.md
[scenes-reference]: ../reference/scene.md
[scene-transitions-reference]: ../reference/scene.md#scene-transitions
