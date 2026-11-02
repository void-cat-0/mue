# Themes and selectors

> This page follows the declaration-based theme API. The [design summary](theme-refactor.md) and [implementation specification](theme-refactor-implementation.md) explain the contract. Instance IDs, structural selectors, and nested theme scopes are outside this release.

A component opts into themes with a function-level attribute. Its ordinary parameters are the source of its public builder API; only parameters named in the theme allowlist become readable theme options:

```rust
use std::rc::Rc;
use mue_components::prelude::*;

#[component]
#[theme(primary, progress)]
fn button(caption: Rc<str>, #[default] primary: bool, #[default] progress: i32) -> impl Component {
    let _ = (primary, progress);
    text(caption)
}

let view = button("Save").primary(true).progress(8);
```

The macro generates `ButtonBuilder` and a `Button` type alias with the declaration's visibility. Selectors name the type alias, including concrete generic arguments when applicable. A wrapper function that merely returns an existing builder retains that builder's identity; a separate `#[component] #[theme]` declaration adds an identity for the same root node. Unannotated declarations are ordinary styleable builders.

```rust
use mue_macroquad::{App, theme::theme};

let colors = theme! {
    Button { background: gray; padding: 12px; }
    Button[$primary] { background: blue; }
    Button.selected { opacity: 90%; }
    Button.compact.!disabled[$progress < 32]:hover { opacity: 65%; }
    Button[$primary], Button.selected { color: white; }
    * { transition: 150ms ease-in-out; }
};
let app = App::with_theme(colors, || button("Save").primary(true).enable_hovered());
```

Compound selectors have one fixed order: an optional Rust type path or `*`, zero or more `.class` / `.!class` conditions, an optional `[$option predicate]`, then optional `:hover`. The option block requires a named type. `$primary` reads the generated accessor; ordinary names in predicates remain Rust locals, constants, or enum variants. Commas share one declaration block across alternative selectors. There are no descendant, child, or sibling combinators.

Classes are validated names stored in a whole set:

```rust
use std::collections::HashSet;
use mue_core::prelude::*;
use mue_macroquad::theme::{ClassName, Themeable, class};

const SELECTED: ClassName = class!("selected");
let names = signal(HashSet::from([SELECTED]));
let view = button("Save").classes(names).enable_hovered();
names.update(|set| { set.insert(class!("compact")); });
names.update(|set| { set.remove("selected"); });
```

Each `.classes(...)` call replaces the entire set and its binding. Derive a set explicitly when several boolean conditions contribute classes. Names match `[A-Za-z][A-Za-z0-9_-]*`, are case sensitive, and are checked at compile time by `class!`, including in constants. Runtime names use fallible `ClassName::try_from`. Class names do not require registration.

Hover tracking is explicit. `.enable_hovered()` makes `:hover` observable on an opted-in target. `.with_hovered(|view, hovered| ...)` immediately supplies a read-only signal for ordinary builder configuration, including builders without themes:

```rust
use mue_macroquad::style::{Styleable, StyleableExt};

let view = text("Hover me").with_hovered(|view, hovered| {
    view.opacity(hovered.select(0.7, 1.))
});
```

Install the only theme at the application root. `App::with_theme` accepts `Theme`, `Signal<Theme>`, or `ReadSignal<Theme>` through `Prop<Theme>`, and runs its build closure after installing the shared context. Dynamic children inherit that context. Combine modules in source order with `Theme::compose`:

```rust
use mue_macroquad::theme::Theme;

let shared = theme! { Button { padding: 12px; } };
let light = theme! { Button { background: white; color: black; } };
let dark = theme! { Button { background: black; color: white; } };
let dark_mode = signal(false);
let appearance = dark_mode.select(
    Theme::compose([shared.clone(), dark]),
    Theme::compose([shared, light]),
);
let app = App::with_theme(appearance, || button("Save"));
```

Changing the theme, options, classes, or hover state updates existing style bindings. It does not run the application builder, component prepare, or node mount again, so local state remains intact. There is no local or nested installation API; restrict rules with identities and classes.

The cascade compares declarations per property: caller and outer component explicit styles take priority over inner explicit styles, followed by matching theme rules, preparation defaults, and framework defaults or inheritance. Theme rules compare specificity lexicographically: class conditions, `$option` occurrences, and `:hover` increase the class count; a named type increases the type count; `*` adds no weight. Later source order breaks ties. A selector list uses the highest specificity of its branches that actually matched. Targets sharing one real node are resolved together without merging their separate options or class sets. When a rule stops matching or is removed, lower-priority declarations become visible again.

Theme rules contain visual, layout, font, and transition declarations. Children and event hooks belong to builders. For handwritten rules, use `Theme::new`, `Rule::new`, and explicit `Specificity`. `Selector::for_target::<T>` enforces one concrete target identity and enables indexed candidate lookup; its predicate supplies the remaining conditions. `Selector::new` supports unrestricted predicates and stays in the common candidate bucket. `theme!` supplies identity and specificity metadata automatically.
