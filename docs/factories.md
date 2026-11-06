# Reusable component declarations

This page keeps the former factory guide's URL. Reuse ordinary Rust functions and component declarations, with parameters for content, appearance, and behavior.

```rust
use std::rc::Rc;
use mue_components::prelude::*;

#[component]
fn tile(caption: Rc<str>, #[default] accent: bool) -> impl Component {
    let opacity = accent.map(|active| if active { 1. } else { 0.7 });
    center().padding((12.,)).opacity(opacity).children(text(caption))
}

let view = tile("Example").accent(true);
```

Call `tile(...)` for each view. Set optional parameters with generated setters such as `.accent(true)`; these also accept a signal or `Prop`. A `#[component]` parameter written as an ordinary value type becomes `Prop<T>` inside the body. Use its `map` method or a `computed` binding to derive style values. Use `#[model]` for a writable `Signal<T>`. Content and callbacks remain ordinary application concerns.

A plain wrapper function can return a configured builder for reuse. Both `#[component]` and `#[node]` generate builders; constructor and setter calls do not execute their declaration bodies. A terminal `#[node]` declaration receives `NodeContext` positionally as its first parameter and returns `Node` through `ctx.finish(defaults, body)`. The first parameter may have any simple name, such as `ctx`, while its declared type is preserved for Rust to check (including qualified paths and aliases). Non-first ordinary parameters named `build` or `style` remain reserved for now; the injected first parameter may use either name or another simple identifier. It is excluded from constructor inputs and setters; `#[default]` and `#[model]` are not allowed on it, and defaults cannot refer to its name because they run before injection. `finish` merges defaults, binds parent state, materializes declared children in the parent scope, and then runs `body`. Use `#[component]` for generated reactive parameter setters and a body that returns another builder. Consuming the outer builder with `into_node` executes the component body once, synchronously in the resulting node's scope, and continues through its returned builder. The returned builder keeps the same owner and accumulated style. Local `provide`, `map`, `computed`, signals, and effects use that owner and are released with the node; helpers called during ordinary setup use the scope active at that call. Reading a parameter and branching with an ordinary `if` samples its current value rather than installing a binding.

Share an application-defined tokens struct through explicit parameters or ordinary `mue_core::scope::provide` / `inject` context. The component chooses the tokens it consumes and passes them directly to style setters. Live changes come from the signals or dynamic `Prop` fields within those tokens. See [application tokens and context](themes.md) for a full example, including ownership and async boundaries, and [component/node authoring](components.md) for the injected `NodeContext`, explicit finish and body ordering, deferred children, and public hook fields. Set up shared `provide` / context in the node author body before `ctx.finish(...)`, or in the component body before returning its builder.
