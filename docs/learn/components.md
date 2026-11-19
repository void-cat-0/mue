# Reusable views and custom nodes

[Learn contents][learn-index]

Start a component when you need a reusable view with a clear caller-facing interface. You already used signals and child lists in Basics. Here you will define parameters, pick who can write to them, and build a custom node when a builder cannot express the behavior.

## Keep one count per gauge

The first version has one button and a count. Add `rain_gauge()` to the root view and press **Record sample**. Mount two copies and press only one. Each component body creates a separate `samples` signal.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::prelude::*;
#[component]
fn rain_gauge() -> impl Component {
    let samples = signal(0_u32);
    let label = samples.map(|n| format!("Rain samples   {n}"));

    vstack().gap(14.).children((
        text(label).style(style! { font-size: 24px; color: #183c2e; }),
        button("Record sample")
            .on_activate(move || samples.set(samples.get() + 1)),
    ))
}
```

</div>
<figure class="mue-demo-preview">

![The gauge shows three samples above the Record sample button][shot-rain-gauge]

</figure>
</div>

`App::new` consumes its root builder. While building the tree, Mue runs the component body once for each gauge, then builds the returned `vstack` in the same node scope. A button press writes the signal and the `map` updates the existing label. If you build the label with `format!("{}", samples.get())` at construction time, it keeps the initial value.

## Take inputs from a parent

For the station's elevation readout, move the varying values into parameters. A required parameter becomes a constructor argument. In the component body, a parameter declared as `u32` is a `Prop<u32>`, so `map` works whether the caller supplied a fixed number or a signal.

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use std::rc::Rc;
use mue::prelude::*;

#[component]
fn elevation(name: Rc<str>, metres: u32) -> impl Component {
    let reading = metres.map(|m| Rc::<str>::from(format!("{m} m")));
    let ink = || style! { font-size: 22px; color: #183c2e; };
    hstack()
        .style(style! { width: 240px; justify-content: space-between; })
        .children((
            text(name).style(ink()),
            text(reading).style(ink()),
        ))
}

let view = vstack().gap(10.).children((
    elevation("River bend", 280),
    elevation("Lookout", 910),
));
```

</div>
<figure class="mue-demo-preview">

![Two rows align River bend at 280 m and Lookout at 910 m][shot-elevation]

</figure>
</div>

| Parameter | Call site | Inside the component |
| --- | --- | --- |
| `metres: u32` | `elevation(name, metres)` | `Prop<u32>` |
| `#[default(0)] metres: u32` | `elevation(name).metres(metres)` | `Prop<u32>` |
| `#[model] metres: u32` | Pass a writable signal | `Signal<u32>` |

Use a default for a value callers can omit. Use a model when the child should write state owned by its caller. A regular function returning a builder also works for sharing a fixed configuration.


## Author a terminal node

A `#[node]` declaration is appropriate when you need to define the actual layout, paint, or event consumers. This gutter consumes a height and reserves space without drawing anything.

```rust
use mue::{prelude::*, renderer::layout::use_layout};

#[node]
fn gutter(ctx: NodeContext, #[default(12.)] size: f32) -> Node {
    ctx.finish(Style::new().height(size), |style| {
        use_layout(style);
    })
}

let gap = gutter().size(24.);
```

`NodeContext` is injected during construction. The caller supplies ordinary builder parameters. Put `provide(...)` in the node body before `finish` so declared children can see the context. `finish` merges defaults and installs the parent's style bindings, then materializes children in the parent scope before running your closure. The closure can initialize layout, paint, and event consumers from the resulting `&mut Style`.

See [node construction and rendering][components-reference] for parameter restrictions, style precedence, and hook ordering. See [state, children, and scope][state-reference] for deferred children and ownership. See the [paint, masks, and textures reference][paint-reference] for renderer details.


[shot-rain-gauge]: ../images/rain-gauge.png
[shot-elevation]: ../images/elevation.png


**Previous** [Layout, paint, and motion][style-guide] · **Next** [Shared appearance][themes-guide]

[learn-index]: index.md
[style-guide]: style.md
[themes-guide]: themes.md
[components-reference]: ../reference/components-and-node.md
[paint-reference]: ../reference/paint-mask-and-textures.md
[state-reference]: ../reference/state-children-and-scope.md
