# Components

The component crate owns reusable behavior:

- layout: `stack`, `wrap`, `center`, `container`, `scroll_area`, `pages`, and navigation;
- graphics: `image`, `shape`, and `circle`;
- typography: `text`, `document`, `markdown`, headings, links, lists, code, and quotations.

`flexbox`, `text`, `shape`, and the atlas-backed text implementation are renderer primitives under `mue-macroquad::node`. `mue-macroquad` does not import `mue-components`.

Every builder is styleable. Components with behavior expose typed options, while custom factories can add their own options and dynamic class flags.
