# Style declarations

[Reference contents][guide-reference]

`style!` turns semicolon-separated `property: value` declarations into a `Style`. Attach the result with `.style(...)`. The tables below cover the names accepted by the macro. The Rust builder API also exposes settings such as shadows and hooks.

| Section | Look up |
| --- | --- |
| [Box sizes and edges](#box-sizes-and-edges) | Lengths, shorthands, axes, and typed inputs |
| [Layout modes](#layout-modes) | Display modes, alignment, clipping, and flex shorthand |
| [Paint](#paint) | Color forms, brushes, corner radius, and opacity |
| [Text](#text) | Fonts, line metrics, alignment, and truncation |
| [Transforms](#transforms) | Operations, units, and pivots |
| [Property transitions](#property-transitions) | Durations, curves, and interpolation |
| [Scoped and explicit tokens](#scoped-and-explicit-tokens) | Typed token lookups and Rust expressions |

## Box sizes and edges

A length literal accepts `px`, `vw`, and `vh`, or a unitless `0`. The table lists the additional forms. A braced value, for example `width: {panel_width}`, passes a Rust value or reactive property to the setter. Use `Size::Percent(50.)` for `50%`. Viewport units resolve against the logical window dimensions, while percentages resolve according to the property and the relevant box.

| Declaration | Additional literal forms | Meaning |
| --- | --- | --- |
| `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height` | `%`, `auto` | Preferred size, lower limit, or upper limit |
| `flex-basis` | `%`, `auto` | Initial size on the main axis before flex grow or shrink |
| `padding`, `padding-{top,right,bottom,left}` | `%` | Space inside the box |
| `margin`, `margin-{top,right,bottom,left}` | `%`, `auto`, signed lengths | Space around the box |
| `inset`, `top`, `right`, `bottom`, `left` | `%`, `auto`, signed lengths | Relative offsets or absolute anchoring edges |
| `border-width`, `border-{top,right,bottom,left}-width` | — | Border size for layout. Use paint or a custom node for a visible outline. |
| `gap`, `row-gap`, `column-gap` | `%` | Distance between flex rows or columns |
| `aspect-ratio` | `auto`, positive number or ratio such as `16 / 9`, `{Option<f32>}` | Preferred width-to-height ratio |

For edge shorthands, one length sets all edges. Two set vertical and horizontal edges. Three set top, horizontal, and bottom. Four set top, right, bottom, and left. A single braced argument to an edge shorthand passes the whole value to the corresponding Rust setter. `gap: 8px 16px` sets row gap to 8 px and column gap to 16 px.

```rust
use mue::{core::Size, prelude::*};

let compact = signal(false);
let width: Prop<Size> = compact.pick(Size::Percent(100.), Size::Px(520.));
let panel = flexbox().style(style! {
    width: {width}; max-width: 100%; padding: 10px 16px 14px;
    border-width: 2px; row-gap: 8px; column-gap: 16px;
    aspect-ratio: 16 / 9;
});
```

## Layout modes

| Declaration | Accepted keywords |
| --- | --- |
| `display` | `flex`, `block`, `none` |
| `box-sizing` | `border-box`, `content-box` |
| `position` | `relative`, `absolute` |
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse` |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` |
| `align-items`, `justify-items` | `start`, `end`, `flex-start`, `flex-end`, `center`, `baseline`, `stretch`, `normal` |
| `align-self`, `justify-self` | Same as the item alignment keywords plus `auto` |
| `align-content`, `justify-content` | `start`, `end`, `flex-start`, `flex-end`, `center`, `stretch`, `space-between`, `space-around`, `space-evenly`, `normal` |
| `overflow`, `overflow-x`, `overflow-y` | `visible`, `hidden`, `clip`, `scroll` |
| `flex-grow`, `flex-shrink` | Nonnegative number or `{f32}` binding |

`border-box` includes padding and borders in the declared size. `content-box` adds those edges to it. An absolute child leaves the parent flow. A relative child keeps its slot. `overflow` takes one keyword for both axes or two in x/y order. For a scroll gesture and scrollbar, use `scroll_area`.

On a flex container, `justify-content` uses the main axis and `align-items` uses the cross axis. `align-content` distributes flex lines when the container wraps and has room on the cross axis. `align-self` overrides item alignment for one child. The current block and flex modes accept `justify-items` and `justify-self` but do not use them for placement. `display: none` omits a box from layout. For retained conditional children, see the [reactive runtime reference][state-reference].

| `flex` shorthand | Grow | Shrink | Basis |
| --- | ---: | ---: | --- |
| `none` | 0 | 0 | `auto` |
| `auto` | 1 | 1 | `auto` |
| `initial` | 0 | 1 | `auto` |
| `80px` | 1 | 1 | `80px` |
| `2` | 2 | 1 | `0%` |
| `2 0` | 2 | 0 | `0%` |
| `2 1 80px` | 2 | 1 | `80px` |

A unitless first number is a grow factor. Supply a basis with its unit, even for `0px`. Bind separate `flex-grow`, `flex-shrink`, and `flex-basis` declarations when the three values come from signals.

## Paint

| Declaration | Literal and Rust forms |
| --- | --- |
| `background`, `background-color`, `color` | A color literal or a Rust value convertible to `Prop<Brush>` |
| `border-radius` | One nonnegative length in `px`, `vw`, or `vh`, or `{Size}` |
| `opacity` | A fraction or percentage clamped into `0..=1` by the literal parser, or `{f32}` |

`background-color` aliases `background`. `color` sets foreground paint. Opacity applies to descendants as well. Hex forms are `#rgb`, `#rgba`, `#rrggbb`, and `#rrggbbaa`. Quotes can enclose a hex color. `rgb(30, 50, 90)` uses 0–255 channels. Percent channels and alpha work with forms such as `rgb(10% 20% 35% / 60%)` and `rgba(30, 50, 90, 0.6)`.

Named colors are `transparent`, `black`, `white`, `red`, `lime`, `blue`, `yellow`, `cyan`/`aqua`, `magenta`/`fuchsia`, `gray`/`grey`, `silver`, `maroon`, `olive`, `green`, `purple`, `teal`, `navy`, `orange`, and `rebeccapurple`. Use a braced `Brush` for a paint that is not a literal solid color. See the [rendering reference][paint-reference] for brushes, shadows, and resource contracts.

## Text

| Declaration | Literal and Rust forms |
| --- | --- |
| `font-size` | Length, including `%`, `em`, `rem`, or `{FontSize}` |
| `font-family` | Quoted name, `serif`, `sans-serif`, `monospace`, `cursive`, `fantasy`, or `{FamilyOwned}` |
| `font-weight` | `normal` (400), `bold` (700), integer 1–1000, or `{Weight}` |
| `font-style`, `font-italic` | `font-style` accepts `normal` and `italic`. Both can take a braced `bool`. |
| `line-height` | Length, nonnegative multiplier, percent, `em`, `normal` (1.2), or `{Size}` for a length |
| `letter-spacing` | Signed length in `px`, `vw`, or `vh` |
| `paragraph-spacing` | Nonnegative length, including `%` |
| `text-align` | `left`, `right`, `center`, `justify`, or `{Option<TextAlign>}` |
| `white-space`, `text-wrap` | `normal` and `pre-wrap` enable wrapping. `nowrap` and `pre` disable it. Use `text-wrap: {bool_property}` for a Rust binding. |
| `text-wrap-mode`, `text-vertical-align` | Braced `TextWrap` and `TextVerticalAlign` from `mue::renderer::node` |
| `text-max-lines` | Braced `Option<usize>`. Use `None` to remove the cap. |
| `text-overflow` | `clip`, `ellipsis`, or `{TextOverflow}` |
| `tab-width` | Nonnegative `u16` in spaces, supplied as a literal or Rust value |
| `text-decoration`, `text-decoration-line` | `none`, `underline`, `line-through`, or both marks in one declaration |
| `text-underline`, `text-strikethrough` | Braced `bool` for independent marks |

Use a known width to test wrapping and a line cap. `white-space` selects wrapping only. It does not implement browser whitespace collapsing modes. A numeric `line-height` multiplies the current font size. For example, `1.3`, `130%`, and `1.3em` have the same effect.

```rust
use mue::{prelude::*, renderer::node::{TextVerticalAlign, TextWrap}};

let label = text("Publish installer and release notes").style(style! {
    width: 160px; height: 60px; font-size: 16px; line-height: 130%;
    text-wrap-mode: {TextWrap::Word}; text-max-lines: {Some(1)};
    text-overflow: ellipsis; text-vertical-align: {TextVerticalAlign::Center};
});
```

## Transforms

`transform` accepts `none`, a braced `Transform`, or a sequence of the following functions. `transform-origin` sets the pivot and accepts one or two lengths or keywords (`left`, `top`, `center`, `right`, `bottom`). A missing axis defaults to center. Translate and origin percentages use the node box. Transforms affect rendering and hit testing while keeping the layout slot. The renderer projects 3D operations onto its 2D canvas.

| Functions | Arguments |
| --- | --- |
| `translate(x[, y])`, `translateX(x)`, `translateY(y)` | Signed lengths including `%`. Omitted y is zero. |
| `translateZ(z)`, `translate3d(x, y, z)` | z is a unitless pixel distance or `px`. x and y use the 2D length forms. |
| `scale(x[, y])`, `scaleX(x)`, `scaleY(y)`, `scaleZ(z)`, `scale3d(x, y, z)` | Unitless factors. One argument to `scale` applies to both axes. |
| `rotate`, `rotateX`, `rotateY`, `rotateZ` | One angle in `deg`, `rad`, `turn`, or `grad`, or unitless zero |
| `skew(x[, y])`, `skewX(x)`, `skewY(y)` | Angles. Omitted y is zero. |
| `perspective(distance)` | Positive unitless pixel distance or `px` |
| `matrix(a, b, c, d, e, f)`, `matrix3d(...)` | Six or sixteen unitless numbers in matrix column order |

```rust
use mue::prelude::*;
let preview = flexbox().style(style! {
    transform: translateX(10%) rotate(-0.02turn) scale(1.1);
    transform-origin: left center;
});
```

## Property transitions

Use `transition: none` for an instant change, a braced `Transition` for a Rust value, or `[all] duration [curve [strength]]`. Durations are nonnegative and use `ms` or `s`. The optional `all` spells out the node-wide transition. Use `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`, or `spring`. Omit the curve for `ease`. Strength ranges from 0 to 1. For ease curves the default is 1, and 0 gives linear motion. For `spring` it controls bounce, with a default of 0.25. `linear` takes no strength.

The initial binding starts at its initial value. Later writes retarget from the current presentation. Only compatible value pairs interpolate. For example, changing `Size::Percent(30.)` to `Size::Percent(70.)` can animate. Switching from `Percent` to `Px` snaps to the target. This declaration affects style bindings. For stack changes, configure the effect on a Scene request. See the [Scene transition contract][scene-transitions-reference].

## Scoped and explicit tokens

Theme tokens use the `token!(...)` value form. It reads the nearest installed `Theme` and takes the value type from the property:

```rust,no_run
# use mue::prelude::*;
# let selected = signal(false);
let style = style! {
    font-size: token!(title);
    gap: token!(gap);
    background: token!(panel);
    color: {selected.pick(token!(text), token!(muted))};
};
```

Bare identifiers are stringified and string literals pass through, so `token!("tile.accent")` covers dotted keys. `token!(theme, key)` uses an explicit theme handle. Inside braces, `token!(...)` calls are rewritten with the property value type, which lets `pick` and `computed` combine tokens; an expression that changes the type, such as `map` to `Transform`, still needs `theme.get::<T>` or a local `Prop`. `transform` and `transform-origin` do not accept the token form.

[guide-reference]: index.md
[style-guide]: ../learn/style.md
[state-reference]: state-children-and-scope.md
[paint-reference]: paint-mask-and-textures.md
[scene-transitions-reference]: scene.md#scene-transitions

Return to the [illustrated style chapter][style-guide] for rendered comparisons.
