# Input timing at a visibly low frame rate

This experiment compares the identity hit by one click against two generations of the same real mue Node tree. Scene management remains unimplemented. App now preplans and dispatches its input before updates; this coordinate comparison remains separate from App callback behavior.

```bash
cargo run -p input-timing --offline -- --fps 1
```

The blue A and orange B exchange positions once per UI frame. Click the target you see; the next UI frame shows two results for the same pointer-down:

- **BEFORE** queries the unchanged Node tree matching the image that was visible while you clicked, then advances the UI state.
- **AFTER** advances the state, computes/publishes the next real layout, then queries the same coordinates.

For example, if the visible left target is A, a click on it produces BEFORE=A and AFTER=B when A/B exchange places. Blank space should stay empty in both paths. This measures target identity, not merely whether either colored rectangle contains a point.

The host continues calling `next_frame()` and reading the shared input cursor every host frame. The entire experiment image is cached in a render target between UI frames, so both targets and reports visibly refresh at the selected rate. There is no thread sleep or blocked event loop. Raw pointer-down events are buffered locally until the next UI frame; mue input batches are not retained or replayed after expiring. `--fps 0.5` makes each displayed frame last two seconds. Escape closes the example. The window uses a fixed logical canvas and is not resizable.

## How the comparison uses current APIs

The Scene layout/render split is not implemented. BEFORE uses `event::hit_test::hit_test` on the last rendered tree. The example invokes `App::frame_with_update(...)` into an offscreen target and exchanges positions inside its update callback, after App input and before real Taffy layout, geometry publication and drawing. AFTER uses the same hit-test API on this newly published tree **before that target replaces the visible image**. Only then does the host display the next image. The Node tree is not mutated between UI frames, so the retained geometry, transforms and children agree with the visible image for BEFORE.

This is a comparison of hit-testing order. It does not run two Scene implementations, change App pointer dispatch, test gesture ownership, or prove that a snapshot of rects alone supports arbitrary animated/scrolling UI. Target labels and reports use macroquad text; target rectangles and their hierarchy use actual mue Nodes.

## Input samples

Standard output contains CSV samples: UI frame, logical coordinates, BEFORE identity, AFTER identity, unchanged touch source timestamp when available, time spent in this example's local input buffer, and the `native` source label (including OS-injected events). The buffer duration starts when the example receives the event; it is **not** hardware-to-display latency. Mouse callbacks do not supply a source timestamp in the current miniquad interface.

## Timestamp and design interpretation

The miniquad raw touch callback includes a `time` parameter. The vendored macroquad event record and mue `InputBatch` preserve the value they receive; this does not prove every platform backend supplies a valid native timestamp. Its clock origin belongs to the platform, so the experiment prints it without comparing it to `macroquad::get_time()`. Buffering an event does not change this field.

There are existing gaps outside this experiment: the vendored Android Java call does not supply the timestamp expected by its Rust JNI function, the iOS adapter has inconsistent touch call sites, and Windows constructs a timestamp when processing the message. Node `PointerEvent` also currently uses the UI processing clock rather than the raw touch time. This example reads the raw mue batch and does not fix those platform or UI-event APIs.

Timestamp preservation and visual hit-testing solve different problems. The timestamp identifies when the event happened, while the hit-test must decide which displayed geometry to use. A new layout does not retroactively become the image the player clicked. The same visual issue can arise with translations, scrolling, clipping or changes to child identity, not just rectangle sizes.

The experiment supports considering **input against the last displayed state → state update → App layout → Scene rendering** as the default for pointer targeting. The single-root App now implements this order using a preplanned input batch and `frame_with_update`; first-frame and resize/DPI-change batches follow the normal routing path without automatic discard or capture cancellation. `frame_paused()` drains suspended input and cancels old capture/hover; the first normal frame immediately accepts newly received events. It does not snapshot arbitrary host changes made outside the supported phase. Determining event owners, transforms and geometry across resize or reactive mutations still requires an explicit contract. Scene failure/recovery semantics are outside this experiment.
