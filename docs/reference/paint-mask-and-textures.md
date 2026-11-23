# Rendering and resources

[Reference contents][guide-reference] · [Related lesson][style-guide]

- [Choose when to build geometry](#choose-when-to-build-geometry)
- [Background paint and shadows](#background-paint-and-shadows)
- [Textures](#textures)

## Choose when to build geometry

| Entry point | Use |
| --- | --- |
| `use_background(style, rect)` | Install the built-in background and shadow consumers on a terminal node. |
| `use_paint(style).build(...)` | Build CPU geometry from tracked dependencies. Draw the resulting shapes in a render hook. |
| `use_paint(style).draw(...)` | Build and submit transient geometry in the current render callback. |
| `Shapes::build(...)` | Own geometry independently of a node's style bindings. |

A node's tree rendering applies its transform to the subtree. Build geometry in the supplied layout coordinates and let `use_paint` handle effective opacity. The following node caches a rectangle and invalidates it when its layout rect changes.

```rust,no_run
use mue::{prelude::*, renderer::{color::Color, layout::use_layout, paint::use_paint}};

#[node]
fn meter_track(ctx: NodeContext) -> Node {
    ctx.finish(style! { width: 100%; height: 6px; }, |style| {
        let rect = use_layout(style).rect;
        let geometry = use_paint(style).build(move |shapes| {
            shapes.fill_rect(rect.get(), Color::from_rgba(60, 97, 145, 255));
        });
        style.on_render.append(move |_| geometry.get_clone().draw());
    })
}
```

## Background paint and shadows

`use_background` caches CPU `Shapes` through `Paint::build`, rebuilding when its
tracked rect, brush, radius or paint state changes. Cached geometry still uses
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

The paint path skips a shadow when its expanded quad is provably outside the
viewport and scissor, even with caching disabled. Uncertain transforms still
draw. Analytic shading skips the blur integral where the clip, rounded cutout,
or integration interval gives zero coverage. Native OpenGL's mask path also
skips demand when its clip lies strictly inside the knockout core. Auto does
not infer occlusion by other widgets or materials.

The pool allows at most 8 resident masks and 16 MiB of logical RGBA8 backing
storage, with a limit of 4 MiB and 2048 pixels per side per mask. Native OpenGL
keeps resizable slots; Vulkan packs masks into fixed-size backing pages, with
capability-checked, fallible page allocation. It never resizes or frees pages
on a cache miss or eviction. The first bake clears a page; later bakes write
only their region. Samples clamp to the region's texel centers. If the fixed
pages are full, fragmented, or cannot fit a shape, extra shadows stay analytic.
The sharp interior cutout remains analytic; RGBA8 quantization and filtering
can produce small visual differences from the full analytic shader.

Only stable keys are baked. Blur below one logical pixel, perspective,
excessive scale/size, unsupported backends, failed texture allocation or mask
shader initialization, and cache pressure are reasons to use the analytic
path. Recently sampled masks are protected from eviction for 100 ms. Bake
attempts are throttled to one per 16 ms after completion. These are heuristics,
not GPU-time budgets. No WebGL or Metal cache support is implied.

Before overwriting mask storage, the baker flushes queued Macroquad draws. On
Vulkan, sampling and rebaking are ordered by barriers in the existing graphics
command stream, without a queue-idle wait. `shadow::clear_shadow_mask_cache()`
invalidates keys and demand history; it shrinks valid OpenGL slots to 1×1 but
retains Vulkan backing pages until context teardown. Keep mue drawing outside
a host raw miniquad pass. `shadow::shadow_mask_cache_stats()` reports hits,
bakes, fallbacks, culled draws, pending demand and logical backing bytes (not
driver memory overhead). The built-in rounded/sector clip stack applies to
both paths.

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
