# Reusable component declarations

> The former factory API has been removed without a compatibility layer. Use declarations and a top-level theme as described here; the [finalized design](theme-refactor.md) and [implementation specification](theme-refactor-implementation.md) explain the migration. This page keeps its existing URL for links.

Declare a reusable component directly. Parameters and defaults belong to the function, and the theme allowlist identifies the values that selectors may read:

```rust
use std::{collections::HashSet, rc::Rc};
use mue_components::prelude::*;
use mue_macroquad::{App, theme::{Themeable, class, theme}};

#[component]
#[theme(accent)]
fn tile(caption: Rc<str>, #[default] accent: bool) -> impl Component {
    let _ = accent;
    center().children(text(caption))
}

let colors = theme! {
    Tile { background: blue; }
    Tile[$accent] { background: coral; }
    Tile.selected { opacity: 85%; }
};
let app = App::with_theme(colors, || {
    tile("Example")
        .accent(true)
        .classes(HashSet::from([class!("selected")]))
});
```

Call `tile(...)` for each view. Set optional parameters with generated setters such as `.accent(true)`; a setter also accepts a reactive value. `Tile` is the generated alias for the concrete builder type, with the same visibility as the declaration. There is no separate value-level handle to clone, pass to the theme, or use to create instances.

Theme rules supply visual declarations, while ordinary component code supplies content and behavior. A plain wrapper function can return a configured builder for reuse. Add `#[component] #[theme(...)]` only when that wrapper needs its own theme identity and options. Only explicitly listed value parameters enter theme metadata; captions, children, callbacks, and resources should remain ordinary parameters.

Classes use `.classes(...)` with a complete `HashSet<ClassName>`, including a signal or derived set. A later call replaces the entire previous set. Use `class!("selected")` for compile-time validation, then match `Tile.selected`. Options and classes are separate: `Tile[$accent]` reads the parameter, and `Tile.selected` reads the class set.

Install themes through `App::with_theme` at the root and combine modules with `Theme::compose`. Existing nodes update their styles when the top-level theme or reactive options/classes change; local component state is retained. See [themes and selectors](themes.md) and [component/node authoring](components.md).
