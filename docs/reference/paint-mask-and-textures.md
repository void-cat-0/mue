# Rendering and resources

[Reference contents][guide-reference] · [Related lesson][style-guide]

- [Choose when to build geometry](#choose-when-to-build-geometry)
- [Background paint and shadows](#background-paint-and-shadows)
- [Vector paths and SVG](#vector-paths-and-svg)
- [Offscreen layers and backdrop blur](#offscreen-layers-and-backdrop-blur)
- [Custom shaders](#custom-shaders)
- [Textures](#textures)

## Choose when to build geometry

| Entry point | Use |
| --- | --- |
| `use_background(style, rect)` | Install the built-in shadow, background, border, and outline consumers on a terminal node. |
| `use_paint(style).build(...)` | Build CPU geometry from tracked dependencies. Draw the resulting shapes in a render hook. |
| `use_paint(style).draw(...)` | Build and submit transient geometry in the current render callback. |
| `Shapes::build(...)` | Own geometry independently of a node's style bindings. |

A node's tree rendering applies its transform to the subtree. Build geometry in the supplied layout coordinates and let `use_paint` handle effective opacity. `ShapesBuilder` tessellates [`PathData`](#vector-paths-and-svg) with `fill(path, rule, shader)` and `stroke(path, stroke_style, shader)`, and draws textured quads with `draw_texture`. The following node caches a rounded track and invalidates it when its layout rect changes.

```rust,no_run
use mue::{prelude::*, renderer::{paint::color::Color, layout::use_layout, paint::use_paint, paint::path::FillRule}};

#[node]
fn meter_track(ctx: NodeContext) -> Node {
    ctx.finish(style! { width: 100%; height: 6px; }, |style| {
        let rect = use_layout(style).rect;
        let geometry = use_paint(style).build(move |shapes| {
            let track = PathData::rounded_rect(rect.get(), 3.);
            shapes.fill(&track, FillRule::NonZero, Color::from_rgba(60, 97, 145, 255));
        });
        style.on_render.append(move |_| geometry.get_clone().draw());
    })
}
```

`Layout` also publishes the resolved `border` and `padding` widths as `ReadSignal<Edges>`, so a custom node can draw inside its content box with `rect.get().deflate(border.get()).deflate(padding.get())`.

## Background paint and shadows

`use_background` caches CPU `Shapes` through `Paint::build`, rebuilding when its
tracked rect, brush, corner radii, border widths, or paint state changes. The
drop shadow, background, and border draw before the node's own render hooks; the
outline is a separate cache drawn after its children and outside the node's own
clip (`clip_radius`, scroll clips), so focus rings stay visible. Cached geometry still uses
immediate submission and batching; no per-background GPU buffers are created.
Permanently transparent solid backgrounds do not create a cache, but may still
draw shadows. The reactive cache is eager: changing dependencies can rebuild
hidden retained nodes too. Paint-building callbacks and their property producers
should be pure geometry/value computations, not recursively draw or build nodes
while the shared tessellator is borrowed.

Filled shapes have antialiased edges: rounded backgrounds, borders,
outlines, backdrop shapes, and vector path fills get a strip one device pixel
wide along their outline that fades to transparent. The strip is ordinary
geometry, so antialiased shapes still batch with everything else and need no
multisampling. Straight axis-aligned rectangles have no fringe, and strokes are
not antialiased yet.

`corner-shape` (CSS Borders 4) changes the curve inside each corner's radius
box: `round` (the default), `squircle`, `bevel` (a straight cut), `scoop`
(concave), `notch`, `square`, or `superellipse(K)` for any curve in between
(K = 1 is round, 2 is a squircle, 0 a bevel, negative values are concave). It
applies to the background, border, outline, and backdrop blur together; overflow
clipping and box shadows still use round corners.

```rust
use mue::prelude::*;

let squircle = flexbox().style(style! { border-radius: 24px; corner-shape: squircle; });
let ticket = flexbox().style(style! { border-radius: 12px; corner-shape: scoop; });
let tag = flexbox().style(style! { border-radius: 10px; corner-shape: superellipse(-0.5); });
```

### Box shadows

`box-shadow` works as in CSS: each layer is the border box moved by an
offset, grown by a spread, and blurred, with the node's corner radius (per-corner
radii use their average). Several layers stack, and the first draws on top.
Like CSS outer shadows, a shadow never shows through the node, so translucent
panels stay clean. Shadows follow the node transform and add paint around the
node without changing its layout bounds. They are drawn by `use_background`, so
custom nodes must call `use_container` or `use_background`.

```rust
use mue::{prelude::*, renderer::{BoxShadow, BoxShadows, ShadowCache, paint::color::Color}};

// In a declaration: x, y, blur, optional spread, and a color.
let card = flexbox().style(style! {
    border-radius: 12px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3), 0 1px 2px rgba(0, 0, 0, 0.25);
});

// From code: one shadow, a list, or a preset that grows with elevation.
let chip = flexbox().box_shadow(BoxShadow::new(0., 2., 6., Color::from_rgba(0, 0, 0, 90)));
let menu = flexbox().box_shadow(BoxShadows::elevation(10.));

// The blur integral can be baked into a shared mask texture (native OpenGL).
let fixed = flexbox()
    .box_shadow(BoxShadows::elevation(16.))
    .shadow_cache_policy(ShadowCache::Enabled);
```

Blurred layers are analytic by default; stable, large shadows may be baked into a
shared, bounded mask pool. `.shadow_cache(true/false)` and
`.shadow_cache_policy(ShadowCache::{Auto, Enabled, Disabled})` choose the
admission policy (Auto when unset). Each layer keeps its own cache state.

<figure class="mue-chart">

![Two identical cards with no shadow on the left and an 18 px elevation shadow on the right][shot-shadow-comparison]

<figcaption>Elevation changes the paint around the card without increasing its layout bounds. The cache policy controls reuse of that shadow, not the requested elevation.</figcaption>
</figure>

### Mask reuse and admission

Exact resident masks reuse immediately, including a new node or a return to an
old geometry key. Only new bakes wait for width, height, effective radius, blur
and sampling resolution to remain unchanged for 100 ms. Color, opacity, translation
and rotation at the same density reuse the mask; size/scale/DPI/camera/viewport
changes may require a new key.

Auto estimates conservative visible halo coverage in physical target pixels: an
affine projected quad clipped by target, viewport and raw scissor, minus the full
body rectangle. A new key needs at least 8192 halo pixels in one observation or
at least four recently drawn distinct same-key nodes whose halo sum reaches 8192.
It also needs eight observations and accumulated halo work at least the bake
texture pixel count within a 500 ms fixed window. The bookkeeping is bounded to
16 pending keys and four contributors/key, using the strongest recent coverage;
repeated calls on one node are not four contributors. These are conservative
heuristics, not a GPU-profit guarantee or a frame snapshot. Enabled bypasses these
economic checks, but not stability/backend/quality/storage limits. Small shadows
and overloaded pending sets may stay analytic indefinitely.

Zero physical coverage or a clip strictly within the knockout core does not
count demand, sample or bake. Uncertain rounded edges/corners still draw. Auto
does not infer occlusion by other widgets/materials, and native projected or
non-normalized homographies fall back rather than applying unsafe culling.
The sharp interior cutout is still evaluated per fragment, preventing filtered
mask texels from leaking into a transparent panel; this is not a full pixel
snapshot or a retained mesh. RGBA8 quantization/filtering can cause small visual
differences from the analytic shader.

The render-thread pool has at most 8 reusable slots and a 16 MiB logical storage
budget, with a 4 MiB/2048-pixel limit per mask. It uses native OpenGL only; blur
below 1 logical pixel, projective transforms, excessive scale/size, unsupported
backends, resource failure or cache pressure fall back to the analytic path.
Continuously changing parameters do not bake every frame; a slow step animation
that rests longer than the stable window can still bake. Missing/evicted nodes wait again,
recently sampled masks are protected from eviction for 100 ms, and GPU bake
attempts (including failures) are throttled to one per 16 ms after completion.
A full actively used pool leaves extra shadows analytic instead of rotating all
masks through repeated bakes. These are heuristics, not GPU-time budgets. No WebGL or Metal support is added by this option.

First bake, slot reuse and `shadow::clear_shadow_mask_cache()` flush queued work
before an offscreen pass or texture resize. They leave macroquad's camera, model,
material, viewport and scissor state intact, but cannot resume an arbitrary host
raw miniquad pass. Keep mue drawing outside host raw passes. Read
`shadow::shadow_mask_cache_stats()` for hits, bakes, fallbacks, Auto rejections,
culled draws, bounded pending keys and logical bytes;
clear on the live render thread to discard demand history, invalidate masks and
shrink valid slots to 1x1 (the 16 ms attempt throttle remains). Failed storage or
passes can reuse their existing slot after the wait; failed texture/FBO object
creation is never repaired by a resize or drawn through. Known shader initialization
failures fall back without repeating GPU work. The bounded pool and its baker live
until context teardown, not node drop.
In-flight driver storage may temporarily exceed the logical budget. Built-in rounded/sector
clip stacks are applied to both analytic and cached shadow sampling; arbitrary external
materials remain outside that stack.

`Style::clip_radius(Size::Px(12.))` clips the node's children to a rounded
rectangle. It does not clip the node's own background; use `border-radius` for
that. Rotation and scale are exact via the inverse model, and hit-testing
follows the clip. Retained text/shape use the matching retained clip material
while that clip pipeline is selected. If the matching shader is unavailable, or
an inner custom material replaces the clip, nodes fall back to immediate drawing.

`UiMaterial::with` / `with_material` use immediate geometry for text and shape
nodes; ordinary macroquad materials do not share the retained vertex layout.
Leaving the scope restores the previous pipeline and retained partner, not the
outer ordinary material's uniforms. Direct raw pipeline switches are checked by
pipeline identity; raw changes to the same material's uniforms or textures are
not mirrored into its retained partner. Temporary material fallback preserves
previously uploaded text/shape meshes. Large text uses retained quads only with a
solid foreground brush; gradients keep immediate per-vertex sampling. Text cache
keys track atlas evictions and the block's own glyph residency, not unrelated
glyph insertions. Fractional layout origins participate in baseline snapping.

`Style::clip_sector(Sector::new(center, start, end))` clips children to an
angular sector. `center` is relative to the node's layout origin, angles are
radians in local layout space, and `.feather(radians)` softens both edges. The
node transform applies to the sector shape. Hit-testing follows the hard
angular boundary; feather only affects drawing. Import `Sector` from
`mue_macroquad`.

`button(label)` is a solid gray button. `variant(...)` picks solid, subtle,
outline, ghost, or link; `palette(Palette::Teal)` the accent color;
`size(ControlSize::Sm)` the size; `primary(true)` is shorthand for solid
blue. `left_icon`, `right_icon`, `loading(...)` (a spinner, with optional
`loading_text`), `disabled(...)`, and `full_width` complete it;
`icon_button(icon)` and `button_group(...)` cover square and grouped
buttons. Colors follow the color mode and sizes are in `rem` (see
`mue_components::system`). `overlay_host` draws in the `Modal` layer; its
backdrop fades while the content rises and scales.

## Vector paths and SVG

`mue_macroquad::paint::path::PathData` is an immutable, cheaply cloned path with cached
bounds. Build one with `PathData::parse` (SVG path data with every command,
relative forms, and compact numbers), the geometry constructors `rect`,
`rounded_rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`,
`regular_polygon`, `star`, `arc`, and `sector` (pie or ring segment), or
`PathData::from(lyon_path)`. `transformed(matrix)` applies an affine matrix
and `trimmed(start, end)` keeps a fraction of the path's length. Equality is
identity, so a newly parsed path always repaints.

Shape nodes draw common shapes without path data: `ellipse()`, `circle()`,
`rectangle(radius)`, `regular_polygon(sides)`, `star(points, inner_ratio)`,
`arc(start, sweep)`, `sector(start, sweep, inner_ratio)`, `line(from, to)`,
`polyline(points)`, and `polygon(points)` (points as fractions of the box).
Their geometry follows the node's content box, strokes stay inside it at
their logical width, and every parameter takes a reactive value, so a
signal or a transition animates the shape. `.trim(start, end)` on any path
or shape node draws part of the outline: animate `end` from 0 to 1 to draw
a stroke on. `arc`, `line`, and `polyline` are stroked with the foreground
color by default; the others are filled like `path`.

The `path(data)` node fits a path into its content box with `PathFit`
(`Contain` by default, also `Cover`, `Fill`, and `None`). Its view box defaults to
the path bounds; set `.view_box(rect)` to keep a fixed coordinate space, as
icons do. Without `.fill(..)` or `.stroke(..)` the path is filled with the
foreground `color`; `.stroke_current_color()` strokes with it instead. Stroke
widths (`StrokeStyle`, `.stroke_width`) are in view-box units and scale with the
fit. When width or height is `auto`, the node measures to the view box size.

```rust
use mue::{prelude::*, renderer::math::Rect};

let check = path(PathData::parse("M4 12l5 5L20 6").unwrap())
    .view_box(Rect::new(0., 0., 24., 24.))
    .stroke_current_color()
    .stroke_style(StrokeStyle::round(2.))
    .style(style! { width: 24px; height: 24px; color: #7fd1a8; });
```

`SvgDocument::parse` reads a small SVG subset: `svg` (`viewBox`, `width`,
`height`), `g`, `path`, `rect` (with `rx`/`ry`), `circle`, `ellipse`, `line`,
`polyline`, and `polygon`; `fill`, `stroke`, `stroke-width`, `stroke-linecap`,
`stroke-linejoin`, `stroke-miterlimit`, `fill-rule`, `opacity`, `fill-opacity`,
and `stroke-opacity`, also inside `style="..."`; and `transform` lists. Text,
gradients, `use`, masks, filters, and CSS selectors are ignored: a
`url(#gradient)` paint uses its fallback color if one follows it, otherwise
nothing; `inherit` and unknown paints keep the inherited paint. The `svg(doc)`
node draws a document with the same fitting and measuring rules as `path`;
`currentColor` follows the node's foreground `color`.

## Offscreen layers and backdrop blur

`mue_macroquad::paint::offscreen::offscreen()` is a flex container whose subtree is
rendered into a window-sized render target and composited once. Its `opacity`
applies to the composited result (true group opacity: overlapping children do
not show through each other), and `filter(..)` adds whole-layer effects:

| `Filter` | Effect |
| --- | --- |
| `Blur(px)` | Gaussian-like blur; large radii run at half or quarter resolution |
| `Grayscale(amount)` | 0 keeps colors, 1 is fully gray |
| `Brightness(factor)` | Multiplies color |
| `Custom(Rc<UiMaterial>)` | Composite with your fragment shader, which samples `Texture` at `v_uv` and receives `u_opacity` |

Content keeps its screen position; ancestor transforms and clips still apply.
`filters(prop)` replaces the list reactively.

`backdrop_blur(px)` (`backdrop-filter: blur(12px)` in `style!`) blurs what was
drawn behind a node, inside the nearest offscreen ancestor, before the node's
background draws on top, following its corner radii (frosted glass). A layer
host, and therefore every `App::new` root, becomes that ancestor automatically
while at least one backdrop-blurred node exists. Give the root an opaque
background so there is something to sample.

```rust
use mue::{prelude::*, renderer::paint::offscreen::{Filter, offscreen}};

let disabled_group = offscreen().opacity(0.6).filter(Filter::Grayscale(1.));
let glass = flexbox().style(style! {
    background: rgba(255, 255, 255, 0.12);
    border-radius: 16px;
    backdrop-filter: blur(12px);
});
```

Each active layer costs a full-window render target (pooled) and one composite;
blur adds two passes. Offscreen rendering uses OpenGL; on other backends the
subtree draws directly and backdrop blur draws nothing. Content is drawn into a
transparent target with the default blending, so alpha is reconstructed when
compositing; one level of translucency is exact, stacked translucency inside
one layer is approximate.

## Custom shaders

`UiMaterial::new(fragment, uniforms, textures)` builds a material from a GLSL
fragment shader that can read `v_pos` (window logical position), `v_uv`, and
`v_color`. Every material also declares built-in uniforms, filled by
`UiMaterial::with` and `with_rect`:

| Uniform | Value |
| --- | --- |
| `float u_time` | Host time in seconds |
| `vec2 u_resolution` | Window size in logical pixels |
| `float u_dpi` | Physical pixels per logical pixel |
| `vec4 u_rect` | The drawn rectangle (`x, y, w, h`) |

`shader_box(material)` is a node that fills its border box, with its corner
radii, using the material. `u_rect` is the node's layout rect and the vertex
color is the node's foreground `color`. Set other uniforms from an `on_update`
hook or an effect.

```rust,no_run
use mue::{prelude::*, renderer::{UiMaterial, node::shader_box}};

let stripes = UiMaterial::new(
    r#"#version 100
    precision mediump float;
    varying highp vec2 v_pos;
    varying lowp vec4 v_color;
    uniform float u_time;
    void main() {
        float band = step(mod(v_pos.x * 0.8 + v_pos.y * 0.6 - u_time * 12.0, 14.0), 8.0);
        gl_FragColor = vec4(v_color.rgb, v_color.a * 0.1 * band);
    }"#,
    Vec::new(),
    Vec::new(),
)
.unwrap();
let background = shader_box(stripes).style(style! { width: 100%; height: 100%; color: #ffffff; });
```

## Textures

`SharedTexture` (`mue_macroquad::SharedTexture`) is the UI-thread handle:
`Rc<Texture2D>`, cheap to clone, and `!Send` by design. Use it for `image`,
`Portal`, shaders, and any render-thread-only sharing.

`SafeTexture` (`mue_macroquad::SafeTexture`) is the `Send + Sync` handle for
textures that may be dropped on a worker thread. It queues the underlying
`Texture2D`; `App::frame` flushes the queue on the render thread. Hosts that do
not use `App` call `flush_pending_texture_deletions()` after their draw calls.
`SafeTexture::to_shared()` clones the `Texture2D` into a `SharedTexture`
(sharing macroquad's managed slot); `try_into_inner()` and `into_shared()`
transfer only when there is a single owner.

Macroquad 0.4's `Texture2D` is `Clone` (not `Copy`); dropping the last clone on
a worker thread panics. Do not replace `SharedTexture` with `SafeTexture` for
UI-thread-only work: the `!Send` bound is a useful guard, and `SafeTexture`
adds atomic refcounting plus a deletion queue.


[components-api]: https://github.com/void-cat-0/mue/blob/main/mue-components/src/lib.rs


[shot-shadow-comparison]: ../images/shadow-comparison.png

[style-guide]: ../learn/style.md

[guide-reference]: index.md
