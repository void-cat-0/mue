# Layout, paint, and motion

[Learn contents][learn-index]

So far Focus is a few labels in a column. In this chapter you give it a real
look: a card with a progress ring, a row of statistics, soft shadows, and
animations that react to the timer. Along the way you learn how mue lays out
boxes, how it paints them, and how values animate.

## Boxes and stacks

Every builder is a **box**. A box has a size, padding inside its edge, a
margin outside it, and children that it arranges. mue uses flexbox, the same
layout model as the web:

- `vstack()` lays its children out top to bottom, `hstack()` left to right.
- `gap` is the space between children; `padding` is the space inside the box.
- `align-items` positions children across the stack (for a `vstack`, left to
  right); `justify-content` distributes them along it.
- `flex-grow: 1` lets a child take the space that is left.

Here is the layout of the Focus card, still without paint:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::prelude::*;

let stat = |label: &'static str, value: &'static str| {
    vstack().style(style! { flex-grow: 1; align-items: center; gap: 2px; }).children((
        text(value).style(style! { font-size: 20px; font-weight: 700; color: #2d2a26; }),
        text(label).style(style! { font-size: 13px; color: #8a8178; }),
    ))
};

let card = vstack()
    .style(style! {
        width: 320px;
        padding: 24px;
        gap: 20px;
        align-items: center;
    })
    .children((
        text("25:00").style(style! { font-size: 56px; color: #2d2a26; }),
        hstack().style(style! { width: 100%; }).children((
            stat("today", "75 min"),
            stat("sessions", "3"),
            stat("streak", "4 days"),
        )),
    ));
```

</div>
<figure class="mue-demo-preview mue-focus">

![A large 25:00 above three evenly spaced statistics][shot-guide-layout]

</figure>
</div>

The three statistics share the row equally because each has `flex-grow: 1`.
Give the first one `flex-grow: 2` and it takes half the row.

Sizes can be pixels (`320px`), percentages of the parent (`100%`), or a share
of the window (`50vw`, `30vh`). Add `min-width`, `max-width`, and the height
versions to keep a box within limits as the window changes. `flex-wrap: wrap`
moves children to a new line when they do not fit, and `position: absolute`
takes a child out of the flow so you can place it with `left`, `top`, `right`,
and `bottom`. For two-dimensional layouts, `grid()` supports CSS grid tracks.
The [style reference][style-reference] lists every property.

## Paint the card

Painting does not change layout. Add a background, rounded corners, a border,
and a shadow to the card:

```rust
use mue::prelude::*;

let card = vstack().style(style! {
    width: 320px;
    padding: 24px;
    gap: 20px;
    align-items: center;
    background: #fffaf3;
    border-radius: 24px;
    corner-shape: squircle;
    border: 1px solid #eadfce;
    box-shadow: 0 12px 32px rgba(70, 50, 20, 0.18), 0 2px 6px rgba(70, 50, 20, 0.12);
});
```

- `border-radius` rounds the corners; give one to four values for different
  corners. `corner-shape: squircle` makes the curve smoother, like app icons;
  `bevel`, `scoop`, and `notch` are also available.
- `border` draws inside the box, `outline` (with `outline-offset`) outside it.
- `box-shadow` works as in CSS: x and y offset, blur, an optional spread, and a
  color. List several shadows separated by commas; the first one is on top.
- `background` also takes gradients and images, written as a braced Rust value
  such as `background: {linear_gradient(...)};`.

Edges are antialiased, so curves stay smooth at any size.

## Draw the progress ring

A ring that empties as the session runs makes the time visible at a glance.
The `path()` node draws vector shapes. Here the path follows the progress, so
the ring redraws whenever the remaining time changes:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::prelude::*;
use std::rc::Rc;

/// An arc of `progress` (0 to 1) around a 100 × 100 box, starting at the top.
fn ring_arc(progress: f32) -> PathData {
    let angle = progress.clamp(0.001, 0.999) * std::f32::consts::TAU;
    let (x, y) = (50. + 44. * angle.sin(), 50. - 44. * angle.cos());
    let large = u8::from(progress > 0.5);
    PathData::parse(&format!("M50 6 A44 44 0 {large} 1 {x} {y}")).unwrap()
}

#[component]
fn progress_ring(remaining: u32, total: u32) -> impl Component {
    let label = remaining.map(|s| Rc::<str>::from(format!("{:02}:{:02}", s / 60, s % 60)));
    let arc = computed(move |_| ring_arc(remaining.get() as f32 / total.get() as f32));
    flexbox()
        .style(style! { width: 200px; height: 200px; })
        .children((
            path(PathData::circle(mue::renderer::math::vec2(50., 50.), 44.))
                .stroke_current_color()
                .stroke_style(StrokeStyle::round(8.))
                .style(style! { position: absolute; width: 100%; height: 100%; color: #efe4d4; }),
            path(arc)
                .stroke_current_color()
                .stroke_style(StrokeStyle::round(8.))
                .style(style! { position: absolute; width: 100%; height: 100%; color: #e07a4f; }),
            center()
                .style(style! { width: 100%; height: 100%; })
                .children(text(label).style(style! { font-size: 40px; color: #2d2a26; })),
        ))
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![An orange ring two thirds full around the time 16:40][shot-guide-ring]

</figure>
</div>

A path keeps its own coordinate space (here 100 × 100) and is scaled to its
box. `stroke_current_color()` paints with the `color` style, so you can change
the ring's color like any text color. `PathData` also has constructors for
rectangles, rounded rectangles, ellipses, and polygons, and parses SVG path
data; the `svg()` node draws whole SVG documents.

## Text

Font settings (size, family, weight, line height) are inherited, so you can set
them once on a container. Text color is not: set `color` on each text, or give a
whole part of the view a default with
`mue::renderer::inherit::provide_foreground(color, || view)`.

```rust
use mue::prelude::*;

let note = text("Short break in 5 minutes. Stretch, drink water, look away from the screen.")
    .style(style! {
        width: 260px;
        font-size: 15px;
        line-height: 1.5;
        color: #6b6259;
        text-align: center;
    });
```

Text wraps at the width of its box. `text-max-lines` and `text-overflow`
shorten long text with an ellipsis, and `font-weight`, `font-family`,
`letter-spacing`, and the decoration properties work as in CSS.

## Property transitions

A value set with `style!` can animate whenever it changes. Add a `transition`
to the property's box and bind the property to a signal:

```rust
use mue::prelude::*;
use mue::renderer::{
    paint::{color::Color, shader::Brush},
    style::{Transform, TransformOp},
};

#[component]
fn breathing_dot(#[model] running: bool) -> impl Component {
    let fill = running.map(|on| {
        Brush::Solid(if on { Color::from_rgba(224, 122, 79, 255) } else { Color::from_rgba(200, 190, 178, 255) })
    });
    let scale = running.map(|on| {
        let s = if on { 1.15 } else { 1. };
        Transform(vec![TransformOp::Scale(s, s)])
    });
    flexbox().style(style! {
        width: 16px;
        height: 16px;
        border-radius: 8px;
        background: {fill};
        transform: {scale};
        transition: 400ms ease-out;
    })
}
```

When `running` flips, the dot grows and changes color over 400 ms instead of
jumping. A transition always starts from the current, possibly half-finished,
value, so flipping quickly never makes it jump. Besides `ease-out`, you can use
`linear`, `ease-in`, `ease-in-out`, and springs (`transition: 500ms spring;`).

## Enter and leave

Some content should animate as it appears and disappears, such as a "Time for
a break" banner. `presence` mounts its content when a condition becomes true,
animates it in, and animates it out before removing it:

```rust
use mue::{prelude::*, components::motion::presence::PresenceAnimation};

#[component]
fn break_banner(#[model] finished: bool) -> impl Component {
    presence(finished, || {
        center()
            .style(style! {
                padding: 12px 20px;
                border-radius: 12px;
                background: #e07a4f;
            })
            .children(text("Time for a break").style(style! { color: #ffffff; }))
    })
    .animation(PresenceAnimation::fade().slide(0., 12.))
}
```

Unlike `show_if`, the content is rebuilt each time it appears, so it starts
fresh. For lists, `transition_group` animates rows that are added, removed, or
reordered; see the [layers reference][layers-reference].

## What you learned

- Boxes are laid out with flexbox: stacks, gaps, padding, alignment, and
  growth.
- Painting (backgrounds, borders, corner shapes, shadows) never changes
  layout.
- `path()` draws vector shapes, and a reactive `PathData` redraws them.
- `transition` animates property changes; `presence` animates content in and
  out.

**Previous** [State and lists][basics-guide] · **Next** [Components][components-guide]

[shot-guide-layout]: ../images/guide-layout.png
[shot-guide-ring]: ../images/guide-ring.png

[learn-index]: index.md
[basics-guide]: basics.md
[components-guide]: components.md
[style-reference]: ../reference/style.md
[layers-reference]: ../reference/layers-and-overlays.md
