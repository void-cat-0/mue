# Styles and units

`style!` is a typed declaration macro. It accepts familiar kebab-case property names and lowers them to the renderer's `Style` setters. Values in braces are Rust expressions and retain their reactive type.

```rust
let card = flexbox().style(style! {
    width: 100%;
    max-width: 640px;
    padding: 16px 24px;
    gap: 12px;
    background: #eef4ff;
    transform: translateY(8px) rotate(-2deg);
    transform-origin: center top;
});
```

Static lengths use `px`, `%`, `vw`, and `vh`; `font-size` also accepts `em` and `rem`. A unitless zero is accepted. Padding, margin, inset, and border widths use the four-value order. `gap: 8px 16px` sets row and column gaps.

Responsive helpers are ordinary Rust expressions, so they stay in braces:

```rust
style! {
    min-height: {responsive::ui(144.)};
    font-size: {responsive::font(22.)};
}
```

The responsive helpers return reactive `Prop<Size>` and `Prop<FontSize>` values. They cannot be written as a static `22px` token because their value changes with the viewport.

Transforms include 2D functions and the common 3D functions `translate3d`, `translateZ`, `scale3d`, `scaleZ`, `rotateX`, `rotateY`, `perspective`, and `matrix3d`. The current renderer is a 2D canvas: 3D values are resolved and projected to that canvas plane.

Transitions accept an optional curve strength after the curve name: `transition: 240ms ease-in-out 0.65;` blends from linear at `0` to the standard ease at `1`. For `spring`, the value controls bounce, for example `transition: 480ms spring 0.35;`.
