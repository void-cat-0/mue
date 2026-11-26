# Layout, paint, and motion

[Learn contents][learn-index]

Read each comparison as a small change to a base view. Start with a node builder, use `style!` for CSS-like declarations, and use braces for typed Rust values. Apply each variant with `.style(variant_a)` or `.style(variant_b)` on the same builder. Names beginning with `child_` belong on the first child. The images label the variants A, B, and C in declaration order.

```rust
use mue::prelude::*;

let card = flexbox().style(style! {
    width: 100%;
    max-width: 420px;
    padding: 16px 24px;
    background: #e8eff7;
});
```

The [style reference][style-reference] lists spellings, units, and typed values.

## Box dimensions and spacing

### Size a card relative to its parent

Use a percentage for the initial width and a pixel cap when a card should stop growing. In the second sample, the cap wins over `100%`. A percentage depends on available parent space. Viewport units such as `vw` refer to the window instead.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    width: 50%; height: 42px;
};
let variant_b = style! {
    width: 100%;
    max-width: 160px; height: 42px;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Compare width, height and a width cap inside identical 280 px tracks.][shot-layout-measure]

</figure>
</div>

### Clamp size or preserve a ratio

`min-width` and `min-height` set lower bounds. `max-width` and `max-height` set upper bounds. For an image placeholder, specify one dimension and keep the other `auto` so `aspect-ratio` can resolve it.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    width: 40px; height: 20px;
    min-width: 100px; min-height: 38px;
};
let variant_b = style! {
    width: 120px; height: auto;
    aspect-ratio: 2 / 1;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The minimums enlarge the first tile. The second is twice as wide as it is tall.][shot-layout-limits]

</figure>
</div>

### Put space on the right box

Add `padding` to the row for space inside its painted edge. Add `gap` to the row for space between its children. Add `margin-left` to one child when only that child needs an offset. On a wrapping row, `gap: 8px 22px` means an 8 px row gap and a 22 px column gap.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    padding: 16px;
};
let variant_b = style! {
    gap: 22px;
};
let child_c = style! {
    margin-left: 22px;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Padding moves both chips in from the panel edge. Gap separates them. Margin moves the first chip and the remaining flow follows.][shot-layout-spacing]

</figure>
</div>

Recreate variant B by mounting this row. To try A, replace `gap: 22px` with `padding: 16px`. To try C, remove the row override and call `.style(style! { margin-left: 22px; })` on `chip("A")`.

```rust
use mue::prelude::*;

let chip = |label| flexbox()
    .style(style! {
        width: 48px; height: 30px; flex-shrink: 0;
        background: #23646c; color: #ffffff;
        align-items: center; justify-content: center;
    })
    .children(text(label).style(style! { font-size: 16px; }));
let row = flexbox()
    .style(style! {
        width: 280px; height: 64px;
        align-items: center; background: #d8e5ec;
    })
    .style(style! { gap: 22px; })
    .children((chip("A"), chip("B")));
```

### Choose what the declared width measures

With `border-box`, the declared width includes padding and border widths. With `content-box`, the same declaration sizes the content, then adds padding and borders. `border-width` participates in layout and `border-color` paints it. An `outline` draws outside the box without affecting layout.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    width: 120px; padding: 16px;
    box-sizing: border-box;
};
let variant_b = style! {
    width: 120px; padding: 16px;
    box-sizing: content-box;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Both tiles declare width 120 px with 16 px of padding on each side, but their painted edges differ.][shot-layout-box]

</figure>
</div>


## Flex layout and placement

### Read the main and cross axes

With `flex-direction: row`, `justify-content` distributes items across the width and `align-items` places them across the height. In a column those roles rotate. The `*-reverse` directions change the start edge and item order. `align-self` overrides the parent setting for one child.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
};
let variant_b = style! {
    flex-direction: column-reverse;
    justify-content: space-between;
    align-items: center;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The same space-between and center settings in a row and a column.][shot-layout-axes]

</figure>
</div>

### Wrap items before aligning lines

`align-content` distributes whole lines. Give the container spare cross-axis space and allow wrapping before changing it. Compare the vertical separation between the two lines; their placement inside each row is unchanged.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    flex-wrap: wrap;
    align-content: start;
};
let variant_b = style! {
    flex-wrap: wrap;
    align-content: space-between;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Start stacks both lines at the top. Space-between puts a line at each edge.][shot-layout-wrap]

</figure>
</div>

### Divide the space left after sizing

Set `flex-basis` for the starting main-axis size. `flex-grow` divides remaining space by weight, and `flex-shrink` lets items give up space when they no longer fit. The shorthand is grow, shrink, basis. In both samples the second child uses `flex: 1 1 0px`.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let child_a = style! {
    flex: 1 1 0px;
};
let child_b = style! {
    flex: 2 1 0px;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The first chip gets half the available width with equal growth and two thirds with double growth.][shot-layout-flex]

</figure>
</div>

### Anchor an overlay without reserving space

Keep a child in normal flow with `position: relative`. Its insets offset the painted position but it still takes up space. With `position: absolute`, leave the flow and anchor it with `top`, `right`, `bottom`, `left`, or the `inset` shorthand.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let child_a = style! {
    position: relative; left: 28px;
};
let child_b = style! {
    position: absolute; right: 12px;
    top: 8px;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Relative moves A from its layout slot and B keeps its position. Absolute anchors A to the right and B takes the first slot.][shot-layout-position]

</figure>
</div>

### Decide what can paint outside a panel

Both examples use an offset chip that crosses the edge. Set `overflow` for both axes or `overflow-x` and `overflow-y` individually. `overflow: hidden` clips the visual result. Use `scroll_area` when users should be able to move the contents.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    overflow: visible;
};
let variant_b = style! {
    overflow: hidden;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![A extends beyond the gray panel with visible overflow and stops at its edge with hidden overflow.][shot-layout-clip]

</figure>
</div>

### Align one child across the row

Set `align-items` on the row and override it on a particular child with `align-self`. `justify-items` and `justify-self` do not position children in the current flex and block layouts.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let child_a = style! {
    align-self: center;
};
let child_b = style! {
    align-self: end;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![A moves to the bottom edge while B stays centered.][shot-layout-self]

</figure>
</div>

### Choose which item can give up space

Both children ask for 180 px in a 280 px panel. In B, setting the first child to `flex-shrink: 0` forces the second child to take the whole reduction.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let child_a = style! {
    flex-shrink: 1;
};
let child_b = style! {
    flex-shrink: 0;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Equal shrink produces equal widths. Disabling shrink on the first chip protects its width.][shot-layout-shrink]

</figure>
</div>

### Keep an outer gap with space-evenly

Use `space-between` to pin the first and last child to opposite edges. `space-evenly` gives the edges the same spacing as the gap between children.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    justify-content: space-between;
};
let variant_b = style! {
    justify-content: space-evenly;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The chips touch opposite edges in A and have matching outer and inner spaces in B.][shot-layout-distribution]

</figure>
</div>

### Lay cards out in a grid

`display: grid` places children into column and row tracks. `repeat(auto-fill, minmax(160px, 1fr))` creates as many columns as fit while each stays at least 160 px wide; the leftover space is shared. `grid-column: 1 / -1` stretches a child across every column. The `simple_grid` component wraps this pattern: `simple_grid().min_child_width(Some(160.))`.

```rust
# use mue::prelude::*;
let cards = style! {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 12px;
};
let header = style! { grid-column: 1 / -1; };
```

### Remove a box from layout

Apply `display: none` to omit a box from layout. Use `display: block` for block flow and `display: flex` for flex layout. Use the [conditional children section][conditional-children-guide] when visibility is part of application state.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let child_a = style! {
    display: flex;
};
let child_b = style! {
    display: none;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![B takes the first slot after A is omitted from layout.][shot-layout-display]

</figure>
</div>


## Paint and transforms

### Change a tile without changing its size

`background` and `background-color` set the backing paint. `border-radius` rounds it (one value, or up to four corners), and `border` or `outline` draw a line inside or outside the box. `color` provides the foreground for text. `opacity` affects the node and its descendants together, so the text and paint fade together.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    background: #23646c; color: #ffffff;
};
let variant_b = style! {
    background: #23646c; color: #ffffff;
    border-radius: 18px; opacity: 48%;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Identical tiles with a solid baseline and a rounded, translucent variant.][shot-layout-paint]

</figure>
</div>

### Choose the point a transform pivots around

Keep the same layout slot in both variants. `transform` changes the drawn geometry, and `transform-origin` sets the pivot. Combine functions in a single declaration, such as `translateY(8px) rotate(12deg)`. The reference lists the supported 2D and projected 3D functions.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    transform: rotate(12deg);
    transform-origin: center;
};
let variant_b = style! {
    transform: rotate(12deg);
    transform-origin: left top;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Both tiles rotate by the same angle. The pale rectangle marks the untransformed layout slot.][shot-layout-origin]

</figure>
</div>


## Typography

### Style the label and its line box

Set family, weight, style and size on the label or an ancestor. `letter-spacing` separates glyphs. `line-height` controls leading within a paragraph and `paragraph-spacing` adds space between paragraphs. Use `text-align` within a line and `text-vertical-align` within a taller label box.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    font-size: 20px; text-align: left;
};
let variant_b = style! {
    font-size: 20px; font-family: monospace;
    letter-spacing: 2px; text-align: right;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The second label changes family, spacing and alignment inside the same text box.][shot-layout-type]

</figure>
</div>

### Fit a long title into a known width

Keep the width fixed while comparing. `white-space: nowrap` disables wrapping. When wrapping is enabled, choose the break strategy with `text-wrap-mode`. To display an ellipsis, set a line limit and use `text-overflow: ellipsis`.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
# use mue::renderer::node::TextWrap;
let variant_a = style! {
    text-wrap-mode: {TextWrap::Word};
};
let variant_b = style! {
    text-wrap-mode: {TextWrap::Word};
    text-max-lines: {Some(1)};
    text-overflow: ellipsis;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![One title wraps across two lines in the first panel and truncates on the first line in the second.][shot-layout-lines]

</figure>
</div>

### Mark completed or emphasized text

Use `font-weight: bold` or `font-style: italic` for emphasis. For a completed item, use `text-decoration: line-through`. Bind `text-underline` or `text-strikethrough` to a boolean when the mark follows state.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    font-size: 24px; font-weight: bold;
    font-style: italic;
};
let variant_b = style! {
    font-size: 24px;
    text-decoration: line-through;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![Bold italic text above the same title rendered with a strike.][shot-layout-marks]

</figure>
</div>

### Space lines inside a paragraph

Use a line-height multiplier when the spacing should scale with the text size. Use `paragraph-spacing` when the extra gap belongs only after paragraph breaks.

<div class="mue-demo">
<div class="mue-demo-code">

```rust
# use mue::prelude::*;
let variant_a = style! {
    line-height: 1; paragraph-spacing: 4px;
};
let variant_b = style! {
    line-height: 1.45; paragraph-spacing: 4px;
};
```

</div>
<figure class="mue-demo-preview mue-style-preview">

![The second panel increases the distance between the first two lines without changing the font size.][shot-layout-leading]

</figure>
</div>


## Bind style to a state change

Give a menu row a selected state. The color follows a signal because you pass the property itself. Use `selected.get()` only when you need a snapshot.

```rust
use mue::prelude::*;
use mue::renderer::color::Color;

let selected = signal(false);
let fill = selected.pick(
    Color::from_rgba(35, 84, 147, 255),
    Color::from_rgba(219, 232, 247, 255),
);
let tab = flexbox().style(style! { background: {fill}; });
selected.set(true);
```

For text decorations, use `text-decoration: underline` or `line-through`. Bind `text-underline` and `text-strikethrough` when the marks depend on booleans. Use `tab-width: {8}` to change tab advance in spaces. The reference lists alignment, display, wrapping and decoration forms. Use [property transitions][transitions-guide] to interpolate a bound value over time.


[shot-layout-measure]: ../images/layout-measure.png
[shot-layout-limits]: ../images/layout-limits.png
[shot-layout-spacing]: ../images/layout-spacing.png
[shot-layout-box]: ../images/layout-box.png
[shot-layout-axes]: ../images/layout-axes.png
[shot-layout-wrap]: ../images/layout-wrap.png
[shot-layout-flex]: ../images/layout-flex.png
[shot-layout-position]: ../images/layout-position.png
[shot-layout-clip]: ../images/layout-clip.png
[shot-layout-paint]: ../images/layout-paint.png
[shot-layout-origin]: ../images/layout-origin.png
[shot-layout-type]: ../images/layout-type.png
[shot-layout-lines]: ../images/layout-lines.png
[shot-layout-self]: ../images/layout-self.png
[shot-layout-shrink]: ../images/layout-shrink.png
[shot-layout-distribution]: ../images/layout-distribution.png
[shot-layout-marks]: ../images/layout-marks.png
[shot-layout-leading]: ../images/layout-leading.png
[shot-layout-display]: ../images/layout-display.png

## Property transitions

A style binding can change between two target values. Give the node a transition to interpolate between them on subsequent frames. For a tide marker, keep one bar and animate its width whenever the selected tide changes.

### Bind the target width

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{core::Size, prelude::*};

#[component]
fn tide_marker() -> impl Component {
    let high = signal(false);
    let width: Prop<Size> = high.pick(Size::Px(220.), Size::Px(68.));
    let bar = flexbox().style(style! {
        width: {width};
        height: 16px;
        border-radius: 8px;
        background: #258584;
        transition: 400ms ease-in-out;
    });

    vstack().gap(16.).children((
        bar,
        button("Change tide").on_activate(move || high.set(!high.get())),
    ))
}
```

</div>
<figure class="mue-demo-preview">

<div class="mue-motion">

![The teal bar grows from low to high tide, pauses, then shrinks back.][shot-tide]

</div>
<div class="mue-still">

![The expanded tide marker above the Change tide button][shot-tide-high]

</div>

</figure>
</div>

`high.pick(...)` returns a property whose value switches between two pixel lengths. Pass that property to `width` and keep the bar mounted. Try pressing **Change tide** again before 400 ms has passed. The transition tracks the new target from the current presentation rather than waiting for the old target. Changing the type of unit, for example from pixels to percent, switches to the target instead of interpolating incompatible lengths.

### Compare curves

| Declaration | What changes |
| --- | --- |
| `transition: 400ms linear;` | The marker travels at a constant rate. |
| `transition: 400ms ease-in;` | It starts slowly. |
| `transition: 400ms ease-out;` | It settles slowly. |
| `transition: 400ms ease-in-out 0.65;` | Both ends ease. Strength 0.65 blends the curve with linear. |
| `transition: 480ms spring 0.35;` | The marker can overshoot the target. |
| `transition: none;` | It snaps to the target. |

`ease` is also available. Ease strength defaults to 1 and spring bounce defaults to 0.25. Both take values between 0 and 1. For runtime choices, pass `Transition` through `transition: {curve};` or call `.transition(curve)`. The equivalent constructors include `Transition::ease_in_out_with_strength(0.4, 0.65)` and `Transition::spring(0.48, 0.35)`.

`App::frame` advances active property transitions before layout. Navigation and pages use the same `Transition` type. See [Scene transitions][scene-transitions-guide] to attach one to a stack request.

### Animate a paint change without resizing the row

Attach the transition to the row itself and bind its background. Use this for a selection marker that should not move its siblings. Compare it with the width binding above, which requests new layout as the width changes.

```rust
use mue::{prelude::*, renderer::color::Color};

#[component]
fn selection_row() -> impl Component {
    let selected = signal(false);
    let tint = selected.pick(Color::from_rgba(171, 214, 241, 255), Color::WHITE);
    flexbox().style(style! {
        padding: 10px 14px;
        background: {tint};
        transition: 180ms ease-out;
    }).children(button("Toggle selection").on_activate(move || selected.toggle()))
}
```

Press twice before the color settles. The second press sets a new target for the same binding. Keep unrelated properties static so the transition only has one changing value to follow.


[shot-tide]: ../images/tide.gif
[shot-tide-high]: ../images/tide-high.png


**Previous** [Reactive state and children][basics-guide] · **Next** [Reusable views and custom nodes][components-guide]

[learn-index]: index.md
[basics-guide]: basics.md
[transitions-guide]: style.md#property-transitions
[components-guide]: components.md
[conditional-children-guide]: basics.md#conditional-children
[scene-transitions-guide]: scenes.md#scene-transitions
[style-reference]: ../reference/style.md
