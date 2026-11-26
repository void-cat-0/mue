# Components

[Learn contents][learn-index]

Focus now has a timer card, a statistics row, and a session list, all in one
long function. In this chapter you split it into components that each do one
job, and see every way a component can talk to its caller: props, two-way
values, events, and slots. At the end you write a custom node that draws by
itself.

## Props

A component's parameters become its **props**. Required parameters go in the
constructor; parameters marked `#[default]` are optional and get a setter
method:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::prelude::*;
use std::rc::Rc;

#[component]
fn stat(label: Rc<str>, value: Rc<str>, #[default] highlight: bool) -> impl Component {
    let tint = highlight.map(|on| {
        mue::renderer::paint::shader::Brush::Solid(if on {
            mue::renderer::paint::color::Color::from_rgba(224, 122, 79, 255)
        } else {
            mue::renderer::paint::color::Color::from_rgba(45, 42, 38, 255)
        })
    });
    vstack().style(style! { flex-grow: 1; align-items: center; gap: 2px; }).children((
        text(value).style(style! { font-size: 20px; font-weight: 700; color: {tint}; }),
        text(label).style(style! { font-size: 13px; color: #8a8178; }),
    ))
}

let row = hstack().style(style! { width: 320px; }).children((
    stat("today", "75 min").highlight(true),
    stat("sessions", "3"),
    stat("streak", "4 days"),
));
```

</div>
<figure class="mue-demo-preview mue-focus">

![Three statistics; the first value is orange][shot-guide-components]

</figure>
</div>

Inside the body every prop is a `Prop<T>`: either a fixed value or one that
follows a signal. That is why `highlight.map(...)` works whether the caller
wrote `.highlight(true)` or `.highlight(some_signal)`. The returned builder can
still be styled by the caller, as with `stat(...).style(...)`; caller styles
win over the component's own.

## Two-way values

Some components change a value that the caller owns, like the Start button in
chapter 2. Mark that parameter `#[model]`. The caller can pass its own signal
to stay in control, or pass nothing and let the component keep its own state:

```rust
use mue::prelude::*;

#[component]
fn length_picker(#[model] #[default(25)] minutes: u32) -> impl Component {
    let label = minutes.map(|m| std::rc::Rc::<str>::from(format!("{m} min")));
    hstack().gap(8.).style(style! { align-items: center; }).children((
        button("−").on_activate(move || minutes.update(|m| *m = (*m - 5).max(5))),
        text(label),
        button("+").on_activate(move || minutes.update(|m| *m = (*m + 5).min(90))),
    ))
}

let shared = signal(25_u32);
let controlled = length_picker().minutes(shared);
let standalone = length_picker();
```

## Events

When a component should *report* something instead of changing a value,
declare an `#[event]` of type `Callback<T>` and call `emit`. Callers attach
handlers with the generated method:

```rust
use mue::{components::Callback, prelude::*};

#[component]
fn finish_button(minutes: u32, #[event] on_finish: Callback<u32>) -> impl Component {
    button("Finish session").on_activate(move || on_finish.emit(&minutes.get()))
}

let log = signal(0_u32);
let button = finish_button(25).on_finish(move |minutes| log.update(|total| *total += minutes));
```

Events are optional; emitting without handlers does nothing. A caller may add
several handlers by calling the method more than once.

## Slots

A slot lets the caller pass a whole view into your component, like an icon or
extra actions. Declare `#[slot] name: Slot` and place it among your children:

```rust
use mue::{components::Slot, prelude::*};

#[component]
fn panel(title: std::rc::Rc<str>, #[slot] actions: Slot) -> impl Component {
    vstack().gap(12.).style(style! { padding: 16px; border-radius: 16px; background: #fffaf3; })
        .children((
            text(title).style(style! { font-size: 18px; font-weight: 700; color: #2d2a26; }),
            actions.or_else(|| text("No actions")),
        ))
}

let history = panel("History").actions(button("Clear"));
```

For a list whose rows the caller should design, use `ScopedSlot<T>`: the caller
passes a template, and the component builds it once per item with
`render(item)`. The [component conventions][conventions-reference] describe
props, events, and slots in full.

## Draw with a custom node

Components assemble existing builders. When you need to draw something
yourself, write a **node** with `#[node]`. It receives a `NodeContext`; call
`finish` with the node's default style and a function that sets up what the
node does. This one draws twelve tick marks around a clock face:

```rust
use mue::prelude::*;
use mue::renderer::{
    layout::use_layout,
    math::Rect,
    paint::{color::Color, path::FillRule, use_paint},
};

#[node]
fn ticks(ctx: NodeContext, #[default(12)] count: u32) -> Node {
    ctx.finish(Style::new().width(200.).height(200.), move |style| {
        let rect = use_layout(style).rect;
        let paint = use_paint(style);
        style.on_render.append(move |_| {
            let rect = rect.get();
            let center = rect.center();
            let count = count.get();
            paint.draw(|p| {
                for i in 0..count {
                    let angle = i as f32 / count as f32 * std::f32::consts::TAU;
                    let mark = PathData::rect(Rect::new(-1.5, -rect.h / 2., 3., 10.))
                        .transformed(mue::renderer::math::Matrix::from_scale_angle_translation(
                            mue::renderer::math::vec2(1., 1.),
                            angle,
                            center,
                        ));
                    p.fill(&mark, FillRule::NonZero, Color::from_rgba(200, 190, 178, 255));
                }
            });
        });
    })
}
```

`use_layout` gives the node a box and returns its rectangle; `use_paint` gives
it a painter that respects opacity and transforms. Render hooks run every frame
and should only draw. Nodes can also react to input (`on_pointer_event`,
`on_key_down`) and advance animations (`on_update`). The
[view authoring reference][components-reference] lists every hook.

## What you learned

- Parameters are props; `#[default]` makes them optional.
- `#[model]` shares a writable value with the caller.
- `#[event]` reports what happened; `#[slot]` accepts views from the caller.
- `#[node]` builds a node from scratch, with layout, painting, and hooks.

**Previous** [Layout, paint, and motion][style-guide] · **Next** [Themes][themes-guide]

[shot-guide-components]: ../images/guide-components.png

[learn-index]: index.md
[style-guide]: style.md
[themes-guide]: themes.md
[conventions-reference]: ../reference/component-conventions.md
[components-reference]: ../reference/components-and-node.md
