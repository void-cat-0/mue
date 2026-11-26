# Shared appearance

[Learn contents][learn-index]

Switch an observatory card from day to night colors while keeping the same node.

## Register a palette and bind the card

<div class="mue-demo">
<div class="mue-demo-code">

```rust,no_run
use mue::{components::{Theme, token_set}, prelude::*};
use mue::renderer::{paint::color::Color, paint::shader::solid};

let theme = Theme::new();
let day = token_set! { panel: solid(Color::from_rgba(255, 228, 173, 255)) };
let night = token_set! { panel: solid(Color::from_rgba(191, 203, 238, 255)) };
theme.register(&day);

let card = flexbox().style(style! {
    padding: 16px 24px;
    border-radius: 12px;
    background: token!(theme, panel);
}).children(text("Observatory").style(style! {
    font-size: 24px;
    color: #27314c;
}));
// After building the view, change the palette in place.
theme.apply(&night);
```

</div>
<figure class="mue-demo-preview mue-preview-pair">

![The observatory card uses the day palette][shot-observatory-day]

![The same card uses the night palette][shot-observatory-night]

</figure>
</div>

`register` installs the initial brush. `token!(theme, panel)` asks for a token whose type matches the `background` property. Build one App with this card. After its first frame, switch the token set in place.

```rust,no_run
# use mue::{components::{Theme, token_set}, renderer::{paint::color::Color, paint::shader::solid}};
# let theme = Theme::new();
# let night = token_set! { panel: solid(Color::BLACK) };
theme.apply(&night);
```

Try changing `night` and applying it again. If you read the brush with `theme.get::<Brush>("panel").get_clone()` at setup, you pass a snapshot to the builder. Keep the property itself to see the updated brush.

| Task | API |
| --- | --- |
| Read a token with an explicit type | `theme.get::<Brush>("panel")` |
| Read from the nearest theme | Install it with `provide_theme`, then use `token!(panel)` in `style!` |
| Use a dotted key | `token!("card.panel")` |
| Change several types | Apply each typed token set in turn |

A key belongs to a value type, so a brush and a font size can use the same name. Register a key before reading it. Resolve a scoped token when building the view and capture the resulting property in any deferred `map` or `computed` callback.

## Install context for a subtree

You can pass a palette or spacing struct into a component like any other Rust value. For values used deep in a subtree, call `provide` from the parent component body, then `inject` in the consumer. Inside a custom `#[node]`, provide context before `ctx.finish(...)` builds the children.

- Read with `get()` or `get_clone()` for a snapshot in ordinary setup code.
- Pass a signal or `Prop` to a style setter for a live binding. Derive a value with `map` or `computed` in the scope where it should live.
- Keep an owning node or `OwnedScope` alive while its handles are in use. Provision and injection look up values through scope ancestry. Plain tasks poll outside that context. See [Foreground tasks][tasks-guide].

Install a named theme when several descendants need typed tokens. Pass the child *builder* into the provider's scope so the consumer resolves the token while Mue builds its node.

```rust,no_run
use mue::{components::{Theme, provide_theme, token_set}, core::FontSize, prelude::*};
use std::rc::Rc;

#[component]
fn section_title() -> impl Component {
    text("Downloads").style(style! { font-size: token!(title); })
}

fn download_panel() -> impl IntoNode {
    let theme = Rc::new(Theme::new());
    theme.register(&token_set! { title: FontSize::Px(28.) });
    provide_theme(theme, section_title)
}
```

Install a second provider to give one subtree a different title size. A sibling outside that subtree resolves tokens from its own nearest provider.

### Inherit foreground paint

Use `provide_foreground` when several labels should share inherited text paint. A label with an explicit color keeps that color.

```rust,no_run
use mue::{prelude::*, renderer::{paint::color::Color, inherit::provide_foreground}};

let legend = provide_foreground(Color::from_rgba(27, 64, 54, 255), || {
    vstack().children((text("Ridge temperature"), text("Creek temperature")))
});
```

See [token, context, and scope behavior][themes-reference] for token registration, typed lookup, scope ownership, and asynchronous boundaries.


[shot-observatory-day]: ../images/observatory-day.png
[shot-observatory-night]: ../images/observatory-night.png


**Previous** [Reusable views and custom nodes][components-guide] · **Next** [Screen navigation][scenes-guide]

[learn-index]: index.md
[components-guide]: components.md
[scenes-guide]: scenes.md
[tasks-guide]: tasks.md
[themes-reference]: ../reference/themes-and-tokens.md
