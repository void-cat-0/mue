# Themes and selectors

Themes use a small selector syntax. A selector names a factory, and rules are applied from general to specific:

```rust
let colors = theme! {
    canvas { background: {paper}; }
    label { color: {ink}; }
    button[primary] { background: {BLUE}; }
    button[progress < 32] { opacity: 50%; }
    button:hover { color: white; }
    button > label { font-size: 14px; }
    button label { line-height: 1.25; }
    * { transition: 150ms ease-in-out; }
};
```

`[name]` checks a boolean option or a dynamic class flag. Bracket contents can be a Rust expression against the final component's options. `:hover` remains available. A space matches any factory descendant, `>` a direct factory child, `+` the immediately preceding factory sibling, and `~` any preceding factory sibling. A comma separates selector alternatives that share one declaration block. Sibling matching uses ordered factory instances in a tuple passed to `.children(...)`.

```rust
let appearance = theme! {
    panel > button + label { color: white; }
    button, label { transition: 180ms ease 0.7; }
};
```

Factories are ordinary values and the same handle is used to create instances and name selectors:

```rust
let button = factory(build_button).defaults(ButtonOptions { primary: false });
let view = button.create("Save").class("primary", active)
    .style(style! { padding: 12px; });
```
