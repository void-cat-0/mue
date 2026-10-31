# Playground frame timing

Run from the repository root:

```sh
cargo run -p playground --release
```

The native example automatically writes a uniquely named playground-frame-times-*.csv in its current working directory and prints the absolute path to stderr. Set MUE_TIMING_SCENARIO=1 to run the deterministic capture scenario: transition switches at frames 120/480/600/720 and page switches to Typography/Layout at 240/840, and exits after 960 logged frames, flushing the tail. Scripted signal changes are measured in example_update_ms (before App::frame), so rebuilds are not expected to appear in app_frame_ms as manual pointer callbacks do. Existing logs are never replaced. Rows are buffered (256 KiB) and flushed every 120 frames and when the future is dropped. Force-killing the process can lose the buffered tail. A write error disables logging and lets the example continue. Browser builds do not write files.

The playground dependency declarations enable the default-off frame-timing features in mue-macroquad and the vendored Macroquad. Normal builds of other packages do not enable them unless Cargo feature unification builds them together with playground. Timing uses CPU wall-clock spans and counters, not GPU timers; it does not enable telemetry or add glFinish. Probes themselves add overhead, especially on pages with many text nodes. Compare runs with the same release profile and instrumentation.

## Frame boundaries

Each row measures example updates and App::frame before next_frame suspension, then the current backend end_frame and next backend begin_frame during that suspension. The first seven columns preserve the old column names. frame_interval_ms is the interval between consecutive post-await samples. It equals example_update_ms + clear_ms + app_frame_ms + next_frame_wait_ms + bookkeeping_ms (apart from printed rounding). bookkeeping_ms includes prior CSV logging/FPS updates, metric reads, and the short gap before suspension. The first row is a partial start interval. Initial App construction and font loading before the loop are not included in CSV frame rows. With MUE_TIMING_SCENARIO=1, native startup diagnostics on stderr report fonts_ms, app_build_ms, mounted nodes, live signal/effect counts, nested image decode/upload and transition binding, plus elapsed time from run() through the first next_frame. These startup spans include no window/context creation or process loading; image and transition spans are nested within app_build_ms.

page and transition_before are sampled before App::frame; page_after and transition_after are sampled after input callbacks. A transition change is thus recorded in the row that handles it, even when the rendered content still belongs to the previous selection. Window size, backend, build profile and interaction scenario should be kept consistent when comparing logs.

## Inclusive time columns

Times are accumulated milliseconds. Stages are nested and cannot all be summed:

- App top-level stages: transition_ms (viewport + animation advancement and batch flush), timers_ms (callbacks and batch flush), font_resolve_ms, layout_ms (all stabilization passes), render_ms (including text pass boundaries and batch flush), pointer_ms and wheel_ms (including callbacks and resulting reactive changes).
- Layout subparts: taffy_ms includes measure callbacks; text_measure_ms measures text callbacks, including their shaping. layout_sync_ms measures synchronization before the enclosing batch flush. layout_ms includes that flush. Font re-resolution between passes is measured separately in font_resolve_ms.
- Text subparts: text_content_ms measures updates to the rich-text buffer caused by changed properties; text_render_shape_ms measures the shaping call during render (which may use existing caches); text_glyph_draw_ms includes glyph lookup/rasterization, geometry creation/submission and pending atlas uploads. It is CPU time, not GPU execution time.
- Rebuild diagnostics: keyed_update_ms includes keyed_build_ms and keyed_drop_ms; pages_build_ms measures Pages mounting and creation of its retained pages. destination_*_ms measures construction of each playground page, nested under Pages/keyed creation. Recursive keyed operations can nest within the same stage, so keyed totals can include overlapping intervals; use the per-destination and count columns to interpret them. theme_resolve_ms measures selector resolution callbacks and excludes property projection/transition binding work after those callbacks.
- Backend: backend_end_frame_ms includes backend_render_passes_ms, backend_ui_draw_ms, backend_draw_submit_ms, backend_commit_ms, scene update and cleanup. backend_begin_frame_ms is the next frame's begin. These are nested in next_frame_wait_ms. Submissions flushed by application code before suspension remain in app_frame_ms, not these backend subparts.
- next_frame_unattributed_ms = next_frame_wait_ms - backend_end_frame_ms - backend_begin_frame_ms. It includes external event-loop/presentation waits and uninstrumented work. It is not a measurement of GPU time or VSync time. No additional GPU synchronization is performed.

## Counts

layout_pass_count counts stabilization iterations. measure_call_count counts Taffy content measure callbacks. text_measure_call_count, text_content_update_count and text_render_call_count distinguish measurement, buffer content updates and rendering. glyph_count counts attempted layout glyphs (including whitespace), not draw calls. node_mounted_count counts newly registered nodes; keyed_node_created_count / keyed_node_removed_count count cache additions/removals, not recursive descendant totals; pages_created_count counts retained page wrappers.

atlas_* columns are per-row deltas from the cumulative glyph atlas counters (rasterized, evicted, uploads, uploaded_pixels and rejection categories), except atlas_pages which is the current number of pages. They cover example/App work before suspension, not GPU execution. A stable page should normally stop generating new rasters/uploads after warming up. Repeated content updates or uploads on a stationary page provide a place to investigate rather than proving a cause.

For the reported issues, capture idle Layout, idle Typography, and several Transition changes. Compare the transition-before/after values, node counts and keyed/page/destination times for long frames. On Typography compare text content/measure counts, text shaping/draw time, atlas deltas and backend/unattributed wait.

## Finer construction and resource breakdown (127 columns)

The original 64 columns retain their order. Appended detail stages cover prepare, seal, mount, transition binding, inherited paint binding, child mounting, node registration, complete node disposal (including remaining field destruction), scope disposal, layout creation, hook construction, theme projection construction, image resource loading, material creation, material Drop flush and sample cached-shape creation.

For each detail stage, detail_NAME_ms sums every invocation's inclusive duration. Recursive parent and child invocations overlap and this sum can exceed the frame. detail_NAME_outer_ms accumulates only the outermost interval of that same stage, avoiding same-stage double counting. detail_NAME_calls counts all invocations. Different stages still overlap, including their outer values: for example, mount includes child construction and prepare can contain side-branch mounts. These are diagnostics, not additive parts of a pie chart. Snapshot reset must happen with all detail spans closed. Theme projection measures creation of bindings, not their later reactive recomputations. Custom Node::build_with_style bypasses Prepare and Seal, but still records Mount and its subparts.

resource_image encloses encoded texture loading on Graphics and Map. app_image_decode_ms and app_image_upload_ms split actual Texture2D::from_file_with_format calls in application work into decoding/to_rgba8 and from_rgba8 texture creation/upload. app_image_load_count and app_image_decoded_bytes record successful decoded image loads and RGBA8 bytes. These preserve existing uncached loading behavior. backend_image_* measures any such loads during the suspension; app metrics are captured separately before suspension, not discarded. Direct from_image/from_rgba8 operations such as glyph atlas uploads are not counted as encoded image loads. They retain atlas counters from earlier columns. material_flush measures only PageMaterial::drop's explicit flush, not the later implicit Material field destructor; that destructor is included in enclosing node disposal.

Backend end-frame subparts now also include scene_update, projection, gpu_query (CPU duration of telemetry::end_gpu_query), input_reset, texture_gc and touch_cleanup. backend_end_unattributed_ms subtracts all ten non-overlapping subparts from backend_end_frame_ms. It retains timer overhead and uninstrumented screenshot code when that build option is enabled. This residual is not GPU time. No telemetry or extra glFinish is enabled.

## Hotspot breakdown (235 columns)

The first 127 columns remain unchanged. The 108 appended columns include 24 backend/resource times and 13 counters twice, with app_ covering work before suspension and backend_ covering suspension work; eight core cleanup stages with total/outer/calls; and two Tween stages with four workload counters. Name/value pairs for the new backend fields generate both the header and row in one order.

- texture_from_rgba8 includes all callers of from_rgba8, including direct texture construction. texture_new also includes empty_rgba8 storage allocation without initial pixels; from_rgba8 counters exclude this empty allocation. Glyph pages use empty_rgba8 and initialize only padded dirty regions, so a decrease in from_rgba8 counters does not count the later partial uploads. texture_store, texture_filter, and texture_batcher_add describe from_rgba8 setup. Batcher add measures queue insertion. texture_readback includes image allocation and the existing texture_read_pixels call, with alloc/gpu subparts; gpu means CPU wall time spent in the existing driver call. No readback is added by profiling. texture_atlas_build/cache measure the optional atlas path. Counters record attempts and input/allocation bytes, not GPU traffic. The older image_upload column still covers only encoded-image loading and includes from_rgba8.
- scene_node_logic includes ready/update/fixed callbacks and time bookkeeping; scene_node_draw contains scene draw callbacks. scene_camera_set includes push/set and their implicit flush; scene_camera_pop includes pop and its flush. scene_bookkeeping covers pending dense-node changes. These separate scene callbacks from rendering previously queued by the application.
- quad_draw measures every QuadGl::draw call, including implicit camera flushes and empty calls. It includes buffer creation, pass boundaries, vertex/index uploads, state/bindings, uniforms, the backend draw call, screen snapshots, and existing telemetry capture. Counters distinguish draw invocations from actual backend draw calls and record vertices, indices, upload payload bytes, buffer creation, render passes and snapshot requests. These spans overlap camera/end-frame parents; never add them to those parents. CPU driver time is not GPU execution time.
- core_NAME_ms/outer_ms/calls split scope detach, recursive children, cleanup callbacks, signal disposal, final signal destruction, effect disposal, final effect destruction, and final context destruction. total sums all recursive invocations, outer removes only same-stage recursive overlap. signal_value_drop includes the complete SignalInner; effect_destroy includes UpdateContext and captured callback resources; scope_context_drop includes inert vectors and context. Cleanup callback probes also cover callbacks run on effect reruns. Do not sum different core stages or subtract them as disjoint costs.
- detail_tween_drop and detail_tween_registry_retain have inclusive/outer/calls. Drop covers the destructor body, preceding automatic field destruction. tween_drop_count counts attempts; entries_scanned is registry length immediately before a successful retain (predicate work); entries_removed is the actual length decrease; retain_skipped_count distinguishes TLS/borrow failure from zero work.

The analyzer adds a focused Hotspot drill-down for mode-change samples and settled Layout/Typography groups, using only available columns. New timings cannot be reconstructed from old CSVs; old-capture output is a baseline, not a new measurement.

```sh
MUE_TIMING_SCENARIO=1 cargo run -p playground --release
python3 examples/playground/analyze_timings.py playground-frame-times-NEW.csv --output target/playground-timing-report-NEW.md
```

Keep window dimensions, graphics backend, driver, machine load and build profile fixed for comparison. Scripted mode is opt-in; normal interactive runs keep their existing controls.

## Offline statistics

The standard-library Python script reads columns by name and supports earlier seven-/64-column logs and newly appended numeric fields:

```sh
python3 examples/playground/analyze_timings.py playground-frame-times-1793536945616163242-58096.csv --output timing-report.md
```

Replace the input with the newly captured CSV, or provide several paths to compare files. Output goes to stdout if --output is omitted; files use exclusive creation unless --force is explicitly provided. Input files can never be replaced. Defaults: --warmup-frames 6, --settle-frames 35, --slow-ms 50. Statistics include n, mean, p50/p95/p99, maximum and per-column totals; percentiles use linear interpolation at (n-1)*p. Reports distinguish initialization, page/mode changes, their subsequent settling frames and settled page/mode groups. Slow settled frames are retained; no timing cutoff improves the reported steady-state statistics. Missing frame indices and excluded short final records are reported. Changing the definition of settled populations or percentiles can produce small differences from earlier ad-hoc summaries.

