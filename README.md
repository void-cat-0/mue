# mue

mue is a retained, reactive UI toolkit for [Macroquad](https://macroquad.rs), written in Rust. It combines typed Rust builders with Taffy layout and cosmic-text shaping.

## Getting started

Add `mue` to your application’s dependencies using the GitHub repository.

```toml
[dependencies]
mue = { git = "https://github.com/void-cat-0/mue", branch = "main" }
```

Import `mue::{App, prelude::*}` to build UI. Use `mue::macroquad` for the host window and frame loop. See the [getting-started guide](docs/learn/first-step.md) for a complete example and Git dependencies.

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

| Section | Contents |
| --- | --- |
| [Learn](docs/learn/index.md) | Nine chapters in order, from the first window to localization. |
| [Reference](docs/reference/index.md) | API contracts, accepted syntax, and workspace examples. |
| [Example app](docs/example-app.md) | Build a release checklist with multiple screens and file saving. |

## Examples

Run an example from the standalone examples package with Cargo, for example:

```sh
cargo run --manifest-path examples/Cargo.toml --example basics
cargo run --manifest-path examples/Cargo.toml --example scenes
cargo run --manifest-path examples/Cargo.toml --example scene-flow
cargo run --manifest-path examples/Cargo.toml --example scene-transitions
cargo run --manifest-path examples/Cargo.toml --example song-transition
```

The examples in [examples/](examples/) are executable applications and integration references. Macroquad examples open a window and require a working graphics backend.

## Requirements

Rust 1.90 or newer. See [Cargo.toml](Cargo.toml) for workspace dependencies and [examples/Cargo.toml](examples/Cargo.toml) for the standalone examples package.

## License

MIT. See [LICENSE](LICENSE).
