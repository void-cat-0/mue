# Transitions

Use the `transition` style property or the fluent `.transition(...)` setter. It is a default presentation curve for reactive style values:

```rust
let panel = flexbox().style(style! {
    transition: 240ms ease-in-out;
});
```

Supported curves are `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`, and `spring`. An optional strength from `0` to `1` follows an ease curve to blend it from linear to its standard shape; for spring it controls bounce:

```rust
style! {
    transition: 240ms ease-in-out 0.65;
}

style! {
    transition: 480ms spring 0.35;
}
```

The default ease strength is `1`; the default spring bounce is `0.25`. `.transition(Transition::ease_in_out_with_strength(0.24, 0.65))` and `.transition(Transition::spring(0.48, 0.35))` provide the same controls in Rust. `App::frame` advances active transitions before layout. Values with compatible units interpolate; incompatible units switch at the target value.

There is no separate animation hook to import. Navigation and pages use the same `Transition` type for their screen motion.
