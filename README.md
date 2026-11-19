# mue

mue is a retained, reactive UI toolkit for [Macroquad](https://macroquad.rs), written in Rust. It combines typed Rust builders with Taffy layout and cosmic-text shaping.

## Getting started

Add `mue` to your application’s dependencies using the GitHub repository.

```toml
[dependencies]
mue = { git = "https://github.com/void-cat-0/mue", branch = "main" }
```

Import `mue::{App, prelude::*}` to build UI. Use `mue::macroquad` for the host window and frame loop. See the [getting-started guide](docs/getting-started.md) for a complete example and Git dependencies.

## Crates

| Crate | Purpose |
| --- | --- |
| `mue` | Application entry point for the renderer, reactive core, components, and authoring macros. Enable `i18n` for Fluent localization. |
| `mue-core` | Renderer-independent signals, effects, scopes, properties, and foreground tasks. |
| `mue-macroquad` | Macroquad nodes, styles, input, rendering, App, and Scene runtime. |
| `mue-components` | Layout, typography, graphics, and optional application token components. |
| `mue-macros` | `#[component]`, `#[node]`, and `style!` procedural macros. |
| `mue-i18n` | Optional Fluent catalogs and reactive translations. |

## Documentation

Build the public API documentation starting at the `mue` crate.

```sh
cargo doc -p mue --all-features --open
```

The `mue::guide` module collects the application guides in Rustdoc. The same guides are available here on GitHub.

- [Getting started](docs/getting-started.md)
- [Components and custom nodes](docs/components.md)
- [Reusable component declarations](docs/factories.md)
- [Styles and units](docs/style.md)
- [Property transitions](docs/transitions.md)
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
