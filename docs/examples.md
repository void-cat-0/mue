# Examples

The workspace examples cover the public API:

- `style`: declaration syntax, transforms, transitions, and component composition;
- `playground`: interactive layout, typography, graphics, input, application tokens, and style declarations;
- `theme`: application-defined tokens, explicit reactive button styles, hover, and light/dark switching;
- `animation`: reactive style transitions;
- `document` and `atlas`: typography and glyph atlas behavior;
- `nodes`, `basics`, `color`, `gestures`, `navigation`, `pages`, and `tab`: layout, input, graphics, and navigation.

Run one with `cargo run -p style-example` or `cargo run -p theme`. Shared appearance data is ordinary application code: [application tokens and context](themes.md) explains `Prop` bindings, `map`, and `provide` / `inject`.
