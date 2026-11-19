# mue

mue is a retained, reactive UI toolkit for [Macroquad](https://macroquad.rs), written in Rust. It combines typed Rust builders with Taffy layout and cosmic-text shaping.

## Crates

| Crate | Purpose |
| --- | --- |
| `mue-core` | Renderer-independent signals, effects, scopes, properties, and foreground tasks. |
| `mue-macroquad` | Macroquad nodes, styles, input, rendering, App, and Scene runtime. |
| `mue-components` | Layout, typography, graphics, and optional application token components. |
| `mue-macros` | `#[component]`, `#[node]`, and `style!` procedural macros. |
| `mue-i18n` | Optional Fluent catalogs and reactive translations. |

## Documentation

Build the public API documentation for the workspace crates:

```sh
cargo doc --workspace --no-deps --open
```

The crate and module pages document the public API, contracts, limitations, and examples. The guides cover workflows that span several APIs:

- [Getting started](docs/getting-started.md)
- [Components and custom nodes](docs/components.md)
- [Styles and units](docs/style.md)
- [Scenes and input](docs/scenes.md)
- [Scene transitions](docs/scene-transitions.md)
- [Application tokens](docs/themes.md)
- [Foreground tasks](docs/tasks.md)
- [Fluent localization](docs/i18n.md)
- [Examples](docs/examples.md)

## Examples

Run a workspace example with Cargo, for example:

```sh
cargo run -p basics
cargo run -p scenes
cargo run -p scene-flow
cargo run -p scene-transitions
cargo run -p song-transition
```

The examples in [examples/](examples/) are executable applications and integration references. Macroquad examples open a window and require a working graphics backend.

## Requirements

Rust 1.90 or newer. See [Cargo.toml](Cargo.toml) for workspace dependencies and example packages.

## License

MIT. See [LICENSE](LICENSE).
