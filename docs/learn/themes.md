# Themes

[Learn contents][learn-index]

Colors are now spread across every component of Focus. In this chapter you
collect them into a **theme**: a set of named values that components look up.
Then you add a dark mode that switches the whole app at once, without
rebuilding anything.

## Name your values

A `TokenSet` maps names to values of one type. Register it on a `Theme`, and
refer to a name with `token!` inside `style!`:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{components::{Theme, token_set}, prelude::*};
use mue::renderer::paint::{color::Color, shader::solid};

let theme = Theme::new();
let light = token_set! {
    canvas: solid(Color::from_rgba(244, 239, 230, 255)),
    card: solid(Color::from_rgba(255, 250, 243, 255)),
    ink: solid(Color::from_rgba(45, 42, 38, 255)),
};
theme.register(&light);

let card = center()
    .style(style! {
        width: 280px;
        padding: 24px;
        border-radius: 20px;
        background: token!(theme, card);
    })
    .children(text("25:00").style(style! {
        font-size: 56px;
        color: token!(theme, ink);
    }));
```

</div>
<figure class="mue-demo-preview mue-focus">

![A light card with dark 25:00 text][shot-guide-theme-light]

</figure>
</div>

Each `token!(theme, name)` is a live handle to the value, not a copy.

## Switch the whole app

To change the theme, `apply` another set with the same names. Every handle
updates in place, so all the components that use the tokens repaint on the
next frame:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::{components::{Theme, token_set}, renderer::paint::{color::Color, shader::solid}};
# let theme = Theme::new();
let dark = token_set! {
    canvas: solid(Color::from_rgba(24, 22, 20, 255)),
    card: solid(Color::from_rgba(38, 35, 32, 255)),
    ink: solid(Color::from_rgba(240, 233, 222, 255)),
};
theme.apply(&dark);
```

</div>
<figure class="mue-demo-preview mue-focus-dark">

![The same card in dark colors with light text][shot-guide-theme-dark]

</figure>
</div>

`apply` only changes names the theme already knows, so register the full light
set first. A token can hold any comparable type: brushes and colors, numbers
for radii and spacing, or font sizes.

## Share the theme with every component

Passing `theme` to every component would be tedious. Install it once for the
app, or provide it to one part of the view, and look tokens up by name with
`token!("name")`:

```rust
use mue::{components::{Theme, provide_theme, token_set}, prelude::*};
use mue::renderer::paint::{color::Color, shader::solid};
use std::rc::Rc;

#[component]
fn session_label() -> impl Component {
    text("Deep work").style(style! { color: token!("ink"); font-size: 18px; })
}

fn focus_app() -> impl IntoNode {
    let theme = Rc::new(Theme::new());
    theme.register(&token_set! { ink: solid(Color::from_rgba(45, 42, 38, 255)) });
    provide_theme(theme, session_label)
}
```

`provide_theme` makes the theme available to everything built inside it.
`Theme::install(&theme)` does the same for the whole app; call it at the start
of your root view. A component can also read a token in code with
`Theme::current()` and `theme.get::<T>("name")`.

## Follow the system

`use_color_mode()` holds the user's preference (light, dark, or follow the
system) and the scheme that results. Apply the matching set whenever the scheme
changes:

```rust
use mue::{components::{ColorScheme, Theme, TokenSet, use_color_mode}, prelude::*};
use mue::renderer::paint::shader::Brush;
use std::rc::Rc;

fn follow_scheme(theme: Rc<Theme>, light: TokenSet<Brush>, dark: TokenSet<Brush>) {
    let color = use_color_mode();
    mue::core::effect::watch(color.scheme, move |scheme| {
        theme.apply(if scheme.get() == ColorScheme::Dark { &dark } else { &light });
    });
}
```

Call `color.toggle()` from a button to switch between light and dark, or set
`color.mode` to follow the system again.

## What you learned

- A `TokenSet` names values; a `Theme` holds the live handles.
- `token!` binds a style to a token, and `apply` switches every user at once.
- `provide_theme` and `Theme::install` share a theme without passing it around.
- `use_color_mode` connects the theme to light and dark mode.

The [theme reference][themes-reference] covers typed lookups, snapshots, and
nested themes.

**Previous** [Components][components-guide] · **Next** [Screens][scenes-guide]

[shot-guide-theme-light]: ../images/guide-theme-light.png
[shot-guide-theme-dark]: ../images/guide-theme-dark.png

[learn-index]: index.md
[components-guide]: components.md
[scenes-guide]: scenes.md
[themes-reference]: ../reference/themes-and-tokens.md
