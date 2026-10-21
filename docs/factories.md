# Factories

`factory` is useful for local definitions. It stores a base builder, defaults, and a visual style callback:

```rust
let tile = factory(|label: &'static str| text(label))
    .defaults(TileOptions { accent: false })
    .style(|style, options| {
        style.background(if options.accent { CORAL } else { BLUE })
    });

let view = tile.create("Example")
    .options(|options| options.accent = true)
    .class("selected", selected);
```

Factories are values with stable identities, so pass the same handle to `theme!` and use `.create(...)` for each instance. This keeps the component's content and options explicit:

```rust
let button = factory(build_button).defaults(ButtonOptions { primary: false });
let colors = theme! {
    button[primary] { background: blue; }
    button { background: gray; }
};

let view = button.create("Save")
    .options(|options| options.primary = true)
    .class("selected", active);
```
