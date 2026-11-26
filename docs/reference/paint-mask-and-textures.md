# Rendering and resources

[Reference contents][guide-reference] · [Related lesson][style-guide]

- [Choose when to build geometry](#choose-when-to-build-geometry)
- [Background paint and shadows](#background-paint-and-shadows)
- [Vector paths and SVG](#vector-paths-and-svg)
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
use mue::{prelude::*, renderer::{color::Color, layout::use_layout, paint::use_paint, path::FillRule}};

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

### Shadow declarations

`Style::shadow(Shadow::new(elevation, radius))` draws a drop shadow behind the
background; use the same radius as `border-radius`. The shadow follows the
node transform (rotation and scale). Shadows are drawn by `use_background`, so
custom nodes must call `use_container` or `use_background`. Shadow geometry is a
stack-built quad with fixed indices, submitted on the immediate path without
temporary `Shapes` or geometry `Vec`s. Material uniform snapshots and batch-buffer
growth may still allocate. Shadow-mask admission defaults to `ShadowCache::Auto`:

```rust
use mue::{prelude::*, renderer::{Shadow, ShadowCache}};

let auto = flexbox().shadow(Shadow::new(18., 16.)); // Auto when unset

let disabled = flexbox()
    .shadow(Shadow::new(18., 16.))
    .shadow_cache(false); // always analytic (Disabled)

let enabled = flexbox()
    .shadow(Shadow::new(18., 16.))
    .shadow_cache_policy(ShadowCache::Enabled); // bypass the Auto admission heuristic
```

The existing `.shadow_cache(true/false)` API still accepts bool, `Prop<bool>` and
bool signals; true means Enabled, false means Disabled. Use `.shadow_cache_policy`
with `ShadowCache::{Auto, Enabled, Disabled}` or a reactive policy signal. These
setters write the same property; the last setter wins. Declared/take accessors now
return `Prop<ShadowCache>` rather than bool. The fluent API does not add a CSS
`shadow-cache` or `box-shadow` declaration to `style!`.

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

`button(label)` defaults to the secondary variant. Use `primary(true)` or
`variant(ButtonVariant::Ghost)` for other weights, `disabled(...)` and
`loading(...)` with reactive values, and `on_activate(...)` for the action.
The current overlay host is a single layer: stacking, enter/exit animation, and
back priority are not implemented yet. IconButton and keyboard focus are also
pending. Button colors are fixed for now; replacing them with application tokens
is a follow-up.

## Vector paths and SVG

`mue_macroquad::path::PathData` is an immutable, cheaply cloned path with cached
bounds. Build one with `PathData::parse` (SVG path data with every command,
relative forms, and compact numbers), the geometry constructors `rect`,
`rounded_rect`, `circle`, `ellipse`, `line`, `polyline`, and `polygon`, or
`PathData::from(lyon_path)`. `transformed(matrix)` applies an affine matrix.
Equality is identity, so a newly parsed path always repaints.

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
