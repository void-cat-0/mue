# Application tokens and context

Applications define their own colors, spacing, typography, and component variants as ordinary Rust data. Pass the values explicitly to component parameters and style setters. Share them across a subtree with `provide` / `inject` from `mue_core::scope` when passing every value through intermediate components would be cumbersome. The structure and naming of these tokens belong to the application.

This page keeps the former theme guide's URL. Obsolete selector-theme records have been removed from the document tree; historical versions remain in Git. See the [project status and design record](project-status.md) for the current implementation and decisions.

## Bind values explicitly

A `Prop<T>` holds either a static value or a read-only signal. A setter accepts that property without taking a snapshot of its current value. Use `map` to project application data into the exact type required by a style property. For example, foreground and background setters accept `Prop<Brush>`, so this example maps palette colors to `Brush::Solid`:

```rust
use std::rc::Rc;
use mue_components::prelude::*;
use mue_core::{prelude::*, scope::{inject, provide}};
use mue_macroquad::{
    App,
    color::Color,
    shader::Brush,
    style::{Styleable, StyleableExt},
};

#[derive(Clone, PartialEq)]
struct Palette {
    foreground: Color,
    surface: Color,
}

fn light() -> Palette {
    Palette { foreground: Color::BLACK, surface: Color::WHITE }
}

fn dark() -> Palette {
    Palette {
        foreground: Color::WHITE,
        surface: Color::new(0.08, 0.10, 0.14, 1.),
    }
}

#[derive(Clone)]
struct AppTokens {
    palette: Prop<Palette>,
}

#[component]
fn token_label(caption: Rc<str>) -> impl Component {
    let tokens = inject::<AppTokens>().expect("AppTokens provider");
    // These local projections belong to this component's node build scope.
    let foreground = tokens.palette.map(|p| Brush::Solid(p.foreground));
    let surface = tokens.palette.map(|p| Brush::Solid(p.surface));
    text(caption).color(foreground).background(surface).padding((12.,))
}

#[component]
fn app_root() -> impl Component {
    let dark_mode = signal(false);
    let palette: Prop<Palette> = dark_mode
        .map(|enabled| if enabled { dark() } else { light() })
        .into();
    provide(AppTokens { palette });

    stack().children((
        token_label("Application colors"),
        text("Toggle colors").on_activate(move || dark_mode.toggle()),
    ))
}

// In the Macroquad entry point, then call app.frame() each frame:
let mut app = App::new(app_root());
```

The root creates the shared source. Each `token_label` creates only the projections it needs in its own component scope. Changing `dark_mode` updates existing style bindings; it does not rerun the component body or recreate its local state. A static palette can be supplied as `Prop::Static(light())`. Custom data does not need a macro or a marker trait when constructed with `Prop::Static`; implementing `PropValue` is only needed for the convenience conversion from a bare custom value to `Prop<T>`.

A plain wrapper function can accept `Prop<Palette>` directly instead of using context. Component parameters written as ordinary value types become `Prop<T>` inside a `#[component]` body; use `#[model]` only for a writable signal. Combine a variant parameter and tokens in `computed(move |_| ...)` when the final value depends on several inputs. Write those inputs' tracked reads inside the computation.

## Optional token store

`mue_components::tokens` covers the common case: many named values of a few types, registered once and read as ordinary properties. It is a value store, not a theme engine — no selectors, no cascade, no dedicated syntax.

```rust
use mue_components::{prelude::*, tokens::{Theme, TokenSet, provide_theme}};
use mue_macroquad::{color::Color, shader::Brush};
use std::rc::Rc;

let theme = Rc::new(Theme::new());
theme.register(&TokenSet::new()
    .with("panel", Brush::Solid(Color::WHITE))
    .with("gap", Size::Px(12.)));

// Read a key as a live property; the setter binds once.
text("Label").background(theme.get::<Brush>("panel"));

// Switch every registered key in place; existing bindings update.
theme.apply(&TokenSet::new().with("panel", Brush::Solid(Color::BLACK)));
```

Keys are indexed by value type first, so the same name can hold a `Brush` and a `FontSize` without colliding, and a key can never be read as the wrong type. `provide_theme(theme, || view)` installs a theme for a subtree through the ordinary scope context, and `Theme::current()` reads it back; `provide_foreground` remains the separate, narrower helper for inherited text color.

## Snapshots and bindings

`get()` copies the current value and `get_clone()` clones it. Both track a dependency when called inside an active reactive computation. In ordinary setup code they return a snapshot: `text("Label").color(tokens.palette.get_clone().foreground)` sets the color read at that moment. A component body runs once during synchronous `into_node` construction; a plain `if` there chooses a branch once, and later signal changes update existing bindings without rerunning the body.

Passing a signal or `Prop`, or passing the result of `map` / `computed`, retains a binding. For `Prop::Static`, `map` evaluates immediately and returns a static property. For `Prop::Dynamic` it creates a derived signal in the scope active when `map` is called; equal projected values suppress downstream notifications through `PartialEq`. The untracked read methods return the latest value without subscribing, including when used inside a computation.

`inject::<AppTokens>()` finds the nearest context value of that type and clones it. Cloning a struct containing `Prop` or signal handles retains access to the same bindings; it does not read their values. `provide` itself is a context insertion, not a reactive setter. Replacing a provided struct later neither notifies existing consumers nor changes previously injected clones. For live updates, provide the stable signal/Prop handles once and update their sources.

## Scope ownership and local derivations

Create shared sources in an application-owned scope that outlives their consumers. A root component is a convenient owner, as in the example above. Generated constructors and setters configure builders without running declaration bodies. When `into_node` consumes the builder, component bodies run synchronously in the resulting node's scope and return builders for the same node, preserving one owner and the accumulated style chain. Local `provide`, `map`, `computed`, signals, and effects are valid there. A terminal `#[node]` body receives `NodeContext` positionally as its first parameter and returns `Node` through `ctx.finish(defaults, body)`. The parameter may have any simple name; its original type is preserved and Rust checks it is `NodeContext`, including qualified paths and aliases. It is excluded from builder inputs and setters, cannot use `#[default]` or `#[model]`, and defaults cannot refer to its name. `finish` merges defaults, binds transition, opacity, font, and transform, materializes the declared child plan in the parent scope, and then runs `body` to register layout, paint, and event consumers. Set up shared `provide` / context in the node author body before calling `ctx.finish(...)`, or in the component body before returning its builder. `Style` has ordinary public fields; hooks can be registered with `style.on_render.append(...)`, while fluent `.on_render(...)` methods remain available. The resulting node retains the scope owner and releases local resources on drop.

Calling an ordinary helper, constructing setter arguments, or invoking a configuration callback follows normal Rust timing. Argument expressions and immediate configuration callbacks can therefore execute user code before `into_node`, even though the declaration body has not run. A helper that creates a derived signal uses the current scope at that call. Do not create a binding in a temporary scope, dispose that scope, and then pass the binding to a longer-lived node. Copying a `Scope`, `Signal`, `ReadSignal`, or dynamic `Prop` does not keep its resource owner alive. Dropping a node releases its local resources, while an external source remains owned by the scope that created it.

For custom ownership, `OwnedScope` disposes its resources on drop. `OwnedScope::new()` starts detached from ambient ownership and context. `OwnedScope::new_inherited()` inherits context lookup from the current scope while retaining separate ownership; it does not keep the context parent's resources alive. Retain the owner for as long as its bindings are used. `Scope::run` temporarily installs a scope for synchronous code and restores the previous scope afterward.

`.children(...)` stores a `ChildrenPlan` and leaves child declaration bodies unexecuted until `finish` has installed the parent context and style bindings. Shared `provide` calls in the parent component body or node author body before `ctx.finish(...)` therefore precede child materialization; `body` operates on the already materialized children. Later `map_keyed` items restore that context and use an owner for each unique key; retained keys keep their original nodes, content, and local state without rerunning their callback, while removed keys release their resources. Use explicit bindings for changing content under a retained key. `show_if` constructs its node when the child plan is consumed, including when initially false, and preserves that node and state while hidden. A prebuilt node retains the context in which it was built. This deferred subtree construction is synchronous and does not provide asynchronous task context propagation.

Dropping a builder or plan before consumption releases its held inputs and owned resources without running declaration bodies. A panic during construction is not recovered either: the unfinished node and everything it owned stays registered, and the node stack is not unwound. These guarantees cover ordinary dropping and successful completion only, because no library code path catches panics.

## Asynchronous boundaries

The current scope is synchronous, thread-local state. Calling `Scope::run(|| async { ... })` only creates a future in that scope; it does not install the scope when the future is later polled or after an `await`. Do not rely on an earlier `provide` or `inject` ambient context being present in an arbitrary completion callback.

Capture the required values or handles explicitly before starting asynchronous work. Keep their owner alive, arrange cancellation or ignore results after that owner is disposed, and perform UI updates on the UI runtime's thread. If a completion must create new reactive resources or look up context, run that synchronous section in the intended live scope; a copied scope handle alone does not extend its lifetime. Ordinary callbacks and futures are not automatically scoped or cancelled by the component API.

## Hover and inherited foreground

Hover is an explicit input to ordinary style configuration:

```rust
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::style::{Styleable, StyleableExt};

let view = text("Hover me").with_hovered(|view, hovered| {
    view.opacity(hovered.select(0.7, 1.))
});
```

The callback runs immediately once. Its derived resources are owned by the hover context and retained when the configured style is finished. Use the supplied read-only handle in a binding; a plain `if hovered.get()` only samples its initial value. The pointer system updates enabled nodes on the topmost hit path; touch does not generate hover. A node reference's hover query does not enable tracking or keep a destroyed node's signal alive.

For text foreground inheritance, use the renderer's explicit subtree helper:

```rust
use mue_components::prelude::*;
use mue_macroquad::{color::Color, inherit::provide_foreground};

let subtree = provide_foreground(Color::BLACK, || {
    stack().children((text("Inherited color"), text("Another label")))
});
```

`mue_macroquad::inherit::provide_foreground` accepts `Prop<Brush>`-compatible inputs and retains its context owner with the subtree. Explicit text color takes priority over this inherited default. Custom text-like primitives can use `mue_macroquad::inherit::inherited_foreground()` to obtain the optional property. This inheritance helper supplies one foreground value; application tokens remain ordinary data consumed explicitly.

See [component and node authoring](components.md), [reusable component declarations](factories.md), [styles and units](style.md), and [transitions](transitions.md).
