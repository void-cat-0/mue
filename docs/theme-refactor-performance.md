# Theme Refactor CPU Baseline

Measured 2026-10-31 in the working tree based on revision 18053395ea342aad32d41242a259130a06671fe2, including the uncommitted refactor and instrumentation changes. This is a reproducible snapshot, not a performance target or a before/after comparison. Timing and counters are collected in the instrumented unit-test binary, not in a normal application build.

## Environment

- Host: x86_64 Linux under WSL2, kernel 6.18.54.1-microsoft-standard-WSL2; Microsoft hypervisor.
- CPU presented to WSL: Intel Core Ultra 9 285H, 8 online CPUs.
- Compiler: rustc 1.99.0 (b940084d7 2026-09-28), LLVM 23.1.1.
- Profile and harness: Cargo release profile, one ignored unit test forced to one test thread. No window was created.
- Command: cargo test -p mue-macroquad --lib --release node::build::perf -- --ignored --nocapture --test-threads=1
- Build cases: 32 warmups, 250 separately timed node mount + headless Taffy layout + node drop samples.
- Update cases: 64 warmups and 2,048 separately timed input changes. Each sample includes synchronous reactive update and headless layout computation.
- Percentiles use the nearest lower observation at indices floor((n - 1) * 0.50) and floor((n - 1) * 0.95); values are nanoseconds.
- A test-only global allocator forwards to System and uses atomic counters for successful alloc/realloc/dealloc calls and bytes. Theme pipeline counters use test-only thread-local stats. Instrumentation adds overhead; only compare runs using the same instrumented harness.

## Results

| Scenario | p50 | p95 | Resolver runs | Projection runs | Transition binds | Width notifications | Height notifications | Live delta scopes / signals / effects |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| No theme, build + layout | 1,938 ns | 2,115 ns | 0 | 0 | 250 | 250 | 250 | +1 / +5 / +6 |
| Empty static theme, build + layout | 36,996 ns | 42,927 ns | 250 | 16,500 | 250 | 250 | 250 | +2 / +163 / +124 |
| Static theme, no matching selector | 38,047 ns | 50,713 ns | 250 | 16,500 | 250 | 250 | 250 | +2 / +163 / +124 |
| Static theme, selector hit | 39,110 ns | 60,613 ns | 250 | 16,500 | 250 | 250 | 250 | +2 / +163 / +124 |
| Class and hover toggle | 17,824 ns/update | 20,632 ns/update | 4,096 | 270,336 | 0 | 2,048 | 2,048 | +4 / +165 / +124 |
| Option changes, same visual result | 7,717 ns/update | 8,176 ns/update | 2,048 | 135,168 | 0 | 0 | 0 | +3 / +164 / +124 |
| Dynamic theme rule add/remove | 8,660 ns/update | 9,324 ns/update | 2,048 | 135,168 | 0 | 0 | 2,048 | +3 / +164 / +124 |
| Dynamic rule with owned derived declaration add/remove | 8,859 ns/update | 9,494 ns/update | 2,048 | 135,168 | 0 | 0 | 2,048 | +3 / +165 / +124 |

Allocation traffic during measured operations; build rows include per-sample node drop, update rows cover only timed update+layout. Reallocations count as a successful allocation plus release of the prior layout, including in-place resize.

| Scenario | Alloc calls | Free calls | Allocated bytes | Freed bytes | Net blocks / bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| No theme, build + layout (250 samples) | 5,750 | 5,750 | 496,000 | 496,000 | 0 / 0 |
| Empty static theme, build + layout | 117,500 | 117,500 | 6,378,750 | 6,378,750 | 0 / 0 |
| Static theme, no matching selector | 117,500 | 117,500 | 6,378,750 | 6,378,750 | 0 / 0 |
| Static theme, selector hit | 117,750 | 117,750 | 6,410,750 | 6,410,750 | 0 / 0 |
| Class and hover toggle (2,048 samples) | 261,120 | 261,120 | 19,929,088 | 19,929,088 | 0 / 0 |
| Option changes, same visual result | 118,784 | 118,784 | 8,374,272 | 8,374,272 | 0 / 0 |
| Dynamic theme rule add/remove | 126,976 | 126,976 | 9,502,720 | 9,502,720 | 0 / 0 |
| Dynamic rule with owned derived declaration add/remove | 130,048 | 130,048 | 9,261,056 | 9,261,056 | 0 / 0 |

Post-drop cleanup allocator deltas are sampled separately from setup and measured operations. Negative entries are frees of scenario setup retained until after measurement and allocator/cache objects reclaimed during cleanup; they are not negative allocations.

| Scenario cleanup | Alloc calls | Free calls | Allocated bytes | Freed bytes | Net blocks / bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| No theme build | 0 | 1 | 0 | 72 | -1 / -72 |
| Empty static theme build | 0 | 3 | 0 | 1,112 | -3 / -1,112 |
| Static theme, no match build | 0 | 6 | 0 | 1,232 | -6 / -1,232 |
| Static theme hit build | 0 | 6 | 0 | 1,232 | -6 / -1,232 |
| Class and hover update | 0 | 458 | 0 | 22,136 | -458 / -22,136 |
| Option update | 0 | 449 | 0 | 21,812 | -449 / -21,812 |
| Dynamic rule update | 0 | 445 | 0 | 21,627 | -445 / -21,627 |
| Dynamic derived declaration update | 0 | 447 | 0 | 21,683 | -447 / -21,683 |

Exact test counters are resolver invocations, aggregate visual projection executions (including color slots), and transition bind_style invocations. Separate benchmark callbacks count selector predicates, rule declaration bodies, width/height consumer executions, and empty cached paint-shape builds. Class/hover has 4,096 resolver runs because both class and hover each invalidate the resolver per update; batched multi-handle writes flush together. Options update the resolver but leave final visual projections equal, so width/height notifications stay zero. Each build sample performs one prepare, one mount, and one explicit headless layout call; each update performs zero prepare/mount and one explicit layout call.

Build live deltas are maximum observed arena counts sampled after each mount relative to the pre-build baseline. Update live deltas are snapshots before and after the measured update sequence; the test asserts the two snapshots are equal, and does not measure an intra-update high-water mark. resource_counts reports live scopes, signals and effects, including the runtime's permanent null signal in the baseline. Every scenario's reactive arena counts return to baseline after node/owner cleanup. The allocator cleanup table is a separate raw process-global delta and can include allocator caches or objects retained for measurement; it does not attribute each byte to a specific framework object.

## Scope And Limits

- Timing includes framework node construction, style binding, an empty cached paint-shape build, and a Taffy compute_layout pass for build cases. Update timing includes synchronous reactive work and a headless layout pass. No application window was opened.
- GPU drawing/rasterization, real frame scheduling, pointer hit-testing, and transition interpolation/advance are not measured. advance(1.0) runs after node drop only to compact the transition driver's weak registry before final cleanup accounting; benchmark nodes use instant default transitions.
- The test-only counting allocator is global to the unit-test binary. The ignored benchmark uses one test thread and does not overlap other tests, but atomics add overhead to every allocation. Successful realloc is counted as one alloc and one free even for an in-place resize. Byte totals are allocator API sizes, not RSS or unique physical memory.
- Exact resolver/projection/transition counts are instrumented test-build counts and also add counter overhead. They are not available in normal application builds.
- The dynamic derived-declaration scenario creates a computed height signal in each active theme generation. Resource counts remain fixed while toggling, measured operations have zero net allocation traffic, and cleanup returns reactive arenas to baseline.
- Results are specific to this compiler, CPU virtualization, checkout, and instrumented release test profile. There is no performance threshold or comparable pre-refactor measurement in this run.

## Resolver Identity Index

Measured 2026-11-02 in the working tree based on revision 6caa01e with the ignored `resolver_identity_scaling` release test on the same Linux/WSL2 host/compiler listed above. Both resolver paths run in one newly built instrumented binary; this is not a timing comparison against the older October binary. It compares indexed `Theme::resolve` with the exhaustive `resolve_linear` reference over the same prebuilt theme and components, 16 warmups and 256 alternating samples per path. These are resolver-only timings: no node mount/layout/projection or GPU work. `Theme::new` index construction is reported separately (16 warmups, 64 samples); input `Rule` cloning is outside the timer/counters, Theme drop is allocation-counted but untimed. The tests assert sparse visual output and exactly-once rule bodies for both paths.

| Workload | p50 indexed | p50 linear | Indexed allocations/op | Linear allocations/op |
| --- | ---: | ---: | ---: | ---: |
| Sparse, 16 rules / 1 branch / 1 target | 373 ns | 431 ns | 1 | 1 |
| Sparse, 256 rules / 4 branches / 1 target | 3,264 ns | 6,331 ns | 3 | 3 |
| Sparse, 4,096 rules / 1 branch / 1 target | 13,038 ns | 37,146 ns | 5 | 5 |
| Sparse, 4,096 rules / 4 branches / 8 targets | 486,369 ns | 523,622 ns | 12 | 11 |
| Universal, 256 rules / 4 branches / 8 targets | 40,671 ns | 40,609 ns | 8 | 8 |
| Class-only, 256 rules / 4 branches / 8 targets | 52,933 ns | 52,488 ns | 8 | 8 |
| Mixed identity + unkeyed, 256 rules / 4 branches / 8 targets | 43,720 ns | 43,405 ns | 8 | 8 |
| Dense repeated identity, 256 rules / 4 branches / 8 targets | 50,643 ns | 50,419 ns | 8 | 8 |

Sparse single-target lookup improves 1.16x at 16 rules, 1.94x at 256 rules/four branches, and 2.85x at 4,096 rules. At eight targets, the 4,096-rule sparse case improves only 1.08x; most matching predicates still run, so candidate union/sort cost narrows the gain. Universal, class-only, custom unkeyed, mixed, and dense workloads are effectively neutral within this single run. Do not treat small differences as stable regressions or gains.

Index construction cost is paid when each Theme is created: p50 `Theme::new` was 1.33 us for 16 rules / 1 branch, 29.5 us for 256 rules / 4 branches, and 402 us for 4,096 rules / 4 branches distributed across 64 identity keys. This is a real tradeoff for dynamically reconstructing large themes; rule-list changes should be composed once and reused when possible. These measurements show a useful win for large sparse identity-heavy themes, not a universal speedup.

The original eight pipeline scenarios were also rerun successfully on 2026-11-02: no-theme resolver/projection counts remained zero, update prepare/mount counts stayed zero, and reactive arenas returned to baseline after cleanup. Absolute p50 times were higher even for the untouched no-theme path (3,549 ns versus the historical 1,938 ns); the older table remains a historical snapshot, and cross-date timings are not attributed to indexing. Dynamic theme construction now has additional index allocations; resolver-only allocation rows above intentionally exclude construction.

## On-demand Property Projections

Measured across 2026-11-02/03 on the Linux/WSL2 host/compiler above. The before run used revision f0be4d7; the after run used that revision plus the lazy projection working-tree changes. The original eight pipeline scenarios and sample counts are unchanged. These are sequential instrumented release runs on the same host, not alternating old/new binaries; treat timing as preliminary and resource/allocation counters as the stronger evidence.

| Pipeline scenario | Before p50 / p95 (ns) | After p50 / p95 (ns) |
| --- | ---: | ---: |
| No theme, build + layout + drop | 3,532 / 3,671 | 3,536 / 3,857 |
| Empty static theme, build + layout + drop | 67,786 / 110,453 | 56,597 / 91,540 |
| Theme with no match, build + layout + drop | 67,102 / 83,972 | 53,376 / 78,104 |
| Static hit, build + layout + drop | 70,365 / 91,409 | 53,908 / 81,536 |
| Class / hover toggle | 32,142 / 39,314 | 23,970 / 44,817 |
| Option changes, same visual result | 14,095 / 15,163 | 9,946 / 12,859 |
| Dynamic rule add / remove | 15,682 / 17,494 | 11,692 / 13,077 |
| Owned derived declaration add / remove | 15,866 / 18,414 | 12,166 / 13,447 |

The representative layout+paint leaf now consumes 47 theme properties instead of 66 (11,750 versus 16,500 projections over 250 builds). Its live signals/effects decrease from 163/124 to 130/98, with scopes unchanged. Empty-theme allocations decrease from 470 to 398 calls per build; bytes decrease from 25,523 to 24,296. Class/hover update projections decrease from 270,336 to 192,512; the other update scenarios decrease from 135,168 to 96,256. Resolver runs, consumer notification counts, and prepare/mount counts keep their prior behavior. Every measured case returns reactive arenas to baseline on cleanup. Class/hover p95 rose in this run despite improved p50, so this does not establish a tail-latency improvement.

The new ignored theme_projection_demand test separately asserts that seal creates one shared resolver but zero property projections. First width consumption creates one signal/effect; 64 repeated declared/take pairs create no resources, execute no projection, and allocate zero bytes. Adding width to an initially empty dynamic theme and changing an unused text declaration both preserve the demanded binding and resource counts. These one-shot phase timings are diagnostics, not percentiles or performance claims.

A separate mount-only comparison uses 32 warmups and 250 samples (no Taffy compute pass): a width-only custom leaf has 7 projections, 24 signals and 17 effects including mandatory node opacity/font/transform/transition state; the full layout+paint leaf has 47 projections, 130 signals and 98 effects. Their p50/p95 were 9,719/10,590 ns and 57,146/93,029 ns respectively. This compares different consumers under the new implementation, not old versus new performance.

Limits: consumed properties still subscribe to the shared resolver, so an unrelated rule change may recompute demanded slots; unused declarations can still execute rule bodies. Per-property caches and owned fallback metadata consume heap memory, and a node consuming every field has less opportunity to benefit. Public composition of an already sealed Style materializes its fields before merging to preserve precedence; the ordinary builder return chain composes raw declarations before seal. Lazy creation does not infer a closed property set from initial matches or treat a static Theme as dependency-free.

Reproduce the additional demand case alone with cargo test -p mue-macroquad --lib --release theme_projection_demand -- --ignored --nocapture --test-threads=1. Use the original cpu_performance_baseline filter for the eight before/after pipeline cases. Both use the process-wide counting allocator and must run without other tests concurrently.

## Static Instant Fast Path

Measured 2026-11-03 on the same Linux/WSL2 host/compiler and instrumented release test binary. The ignored test compares an explicit static Instant curve with an external dynamic curve whose current value is Instant, using identical Theme::default() builds, 32 warmups and 250 samples per path, with the static case measured first and the dynamic case second. Each sample mounts, performs a headless Taffy layout and drops the node. The external signal exists before either case and stays alive through measured node cleanup.

| Explicit transition | p50 / p95 | Live signals / effects above baseline | Allocations / bytes per build |
| --- | ---: | ---: | ---: |
| Static Instant | 30.3 / 38.0 us | 61 / 62 | 221 / 13,791 |
| Dynamic curve currently Instant | 46.2 / 61.2 us | 129 / 97 | 398 / 24,372 |

For these fixtures the static path removes 68 live signals, 35 effects, 177 allocations and 10,581 allocated bytes per build; measured p50 is about 34% lower in this single run. The curves have the same visible value, but only the dynamic curve retains the ability to switch to an animation later. A Theme's default transition is itself a dynamic projection and is not optimized based on its current value; callers must explicitly provide the static Instant value to use the bypass. Regression tests verify that a dynamic curve initially Instant can change to Linear and animate, and that static matrix transition bypasses before the matrix-column conversion. This is an isolated build fixture, not a pre/post implementation comparison or a cross-workload speed guarantee; ordering and host scheduling can affect timings. Reproduce with `cargo test -p mue-macroquad --lib --release instant_transition_baseline -- --ignored --nocapture --test-threads=1`. The existing eight-case pipeline baseline was captured before the change; its default theme curve is dynamic and does not qualify for this bypass.

## Taffy Aggregate Folding

Measured 2026-11-03 on the same Linux/WSL2 host/compiler with the existing release cpu_performance_baseline harness (32 warmups, 250 samples). The baseline was captured immediately before folding size, min/max-size, gap, overflow, margin, padding, border and inset computed values into the final Taffy style computation. Per-field theme/transition Props and the final equality-aware Taffy Style computation are retained.

| Scenario | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| No theme, build + layout + drop | 2.09 / 2.16 us | 3.54 / 3.66 us |
| Empty theme, build + layout + drop | 31.46 / 36.12 us | 52.45 / 69.68 us |
| Theme no match | 31.83 / 48.29 us | 52.80 / 78.47 us |
| Theme hit | 31.90 / 36.99 us | 52.32 / 66.25 us |
| Class / hover toggle | 15.09 / 17.06 us | 25.01 / 26.54 us |
| Option changes, same result | 5.99 / 6.46 us | 10.73 / 12.77 us |
| Dynamic rule add / remove | 7.11 / 7.66 us | 12.38 / 16.39 us |
| Owned derived declaration update | 7.59 / 8.08 us | 12.97 / 15.76 us |

These sequential pipeline runs do not establish a speed improvement: the no-theme fast path, which does not create these intermediates, also slowed substantially between runs. The cause was not isolated; do not attribute all timing deltas to either the optimization or the environment. Resource counts are the confirmed benefit: full themed leaf signals/effects decrease from 130/98 to 121/89, with scopes and 47 property projections unchanged. Empty-theme allocations decrease from 398 to 364 per build; allocated bytes decrease from 24,296 to 22,154. The raw static no-theme case keeps 23 allocation calls but bytes increase from 1,984 to 2,088 as the final closure captures individual fields. Some update cases also allocate more bytes despite fewer calls. Measured intervals retain zero net blocks/bytes and cleanup restores reactive arenas.

A separate raw-dynamic Style resource regression asserts build_taffy() adds at most one signal/effect above its property input signals. The prior implementation added one final Taffy Style computed plus intermediate aggregate computeds for dynamic sizes, overflow and four rectangles; the optimized implementation only creates the final computed. Five regressions cover batched notifications, Px/Vw/Vh equivalence suppression, dynamic theme add/remove fallback, synchronized four-side transitions, and resource cleanup. All eight original pipeline cases still pass and clean reactive resources to baseline.

Reproduction: cargo test -p mue-macroquad --lib --release cpu_performance_baseline -- --ignored --nocapture --test-threads=1. For focused semantic/resource assertions run cargo test -p mue-macroquad --lib style::layout_tests.

### Controlled follow-up: the apparent slowdown did not reproduce

The exact unchanged folded binary was rerun on CPU 0: empty-theme build p50 was 30.8–32.4 us and no-theme p50 was 2.09–2.18 us, versus the earlier 52.45/3.54 us. CPU 4 gave 36.7/2.47 us. Timing therefore changes substantially without rebuilding. The physical cause (frequency, host scheduling, thermal state) was not measured; the earlier roughly 1.7x change is not evidence of an aggregation regression.

The ignored layout_aggregation_compare compiles the legacy build_taffy from f0be4d7 alongside the folded method, both using current projections/transitions and identical mount/paint/layout consumers. Execution was pinned to CPU 0 with taskset, alternating arm order per sample: three rounds, 64 warmups, 2,000 build/layout/drop samples and 2,048 mounted updates per arm. Exact stored Taffy Style and computed size are compared each step. Ranges below are the three per-round p50 values, not pooled percentiles.

| Workload | Legacy p50 range | Folded p50 range | Paired difference |
| --- | ---: | ---: | ---: |
| Static no-theme build | 2.055–2.226 us | 2.068–2.241 us | 0.5–0.7% slower |
| Empty theme build, dynamic default curve | 31.03–34.30 us | 28.63–31.61 us | about 8% faster |
| Theme build, explicit static Instant | 19.45–22.18 us | 17.14–19.49 us | about 12% faster |
| Shared raw dynamic fields build | 3.922–4.386 us | 2.595–2.933 us | about 33% faster |
| Theme updates, dynamic default curve | 10.80–12.33 us | 10.09–11.71 us | about 5–7% faster |
| Theme updates, explicit static Instant | 6.498–8.033 us | 5.935–7.335 us | about 9% faster |
| Raw dynamic fields updates | 1.696–2.075 us | 0.918–1.137 us | about 45% faster |

Actual final Taffy compute/write counts match both paths: one each per build and raw/static-Instant update. Default dynamic transition updates change six presentation values separately: BOTH paths execute six final computations and six set_style writes per input (12,288 over 2,048 inputs). Previous one-notification assertions cover batched raw inputs/static Instant, not every theme update. This existing presentation-flush behavior is a separate optimization opportunity, not a folding regression.

The static-path difference accompanies the confirmed 104-byte larger closure allocation, with unchanged allocation count. No production rollback was indicated. Test-only reference and compute/write counters remain for reproduction; none are enabled in ordinary builds. The original cross-run table is retained as historical evidence; this paired comparison supersedes its timing conclusion. Results remain specific to the instrumented fixture and WSL host; no CPU frequency or thermal cause is claimed.

Build with cargo test -p mue-macroquad --lib --release layout_aggregation_compare --no-run, then run the emitted binary with taskset -c 0 and arguments layout_aggregation_compare --ignored --nocapture --test-threads=1. Affinity applies only to that process.

## Transition Presentation Coalescing

Measured 2026-11-03 on the same Linux/WSL2 host/compiler, pinned to CPU 0. The ignored transition_publication_compare runs the pre-change watch-to-signal implementation and the new computed presentation implementation in the same instrumented release binary. Both arms use the current folded Taffy consumer and notification-revision core. There are three rounds, 64 warmups and 2,048 samples per arm, alternating arm order per sample. Builds include mount, headless layout, output capture and drop; mounted updates include one prebuilt Theme replacement, headless layout and output capture. Exact Taffy Style and computed size must agree after every update. Timings below are ranges of per-round percentiles, not pooled percentiles.

| Scenario | Legacy p50 / p95 range | Computed p50 / p95 range | Paired p50 difference |
| --- | ---: | ---: | ---: |
| Empty-theme build/layout/drop | 38.23–42.69 / 48.43–59.34 us | 41.96–46.87 / 53.20–65.03 us | about 10% slower |
| Theme update, dynamic default transition | 13.68–14.92 / 15.05–17.32 us | 10.77–11.81 / 12.24–13.59 us | about 21% faster |
| Theme update, explicit static Instant control | 7.79–8.61 / 8.40–9.76 us | 7.86–8.70 / 8.44–9.93 us | -1.7% to +1.0%; effectively neutral |

The dynamic-default fixture changes width, height and four padding sides. Legacy execution produced 12,288 final layout computations and set_style writes per 2,048 updates in each final round; the computed path produced exactly 2,048. Legacy intermediate counts depend on HashSet subscriber ordering (earlier diagnostic runs produced fewer), so the reference has no exactly-six semantic guarantee. The new path has a strict one-per-update assertion. Both paths notify each width/height consumer once, perform zero prepare/mount work on updates, retain fixed live arenas during updates and return all reactive resources to baseline on drop. Static Instant remains one computation/write per input in both arms. This supersedes the previous section's statement that default transition updates still have six writes in the current production path.

Builds retain the same live delta: 2 scopes, 121 signals, 89 effects. Per-build allocations decrease from 364 to 360, while allocated bytes increase from 22,154 to 23,197 (+1,043 bytes). These build timings and byte costs are a measured tradeoff, not a claim of universal speedup. Dynamic updates allocate 64 times / 5,088 bytes per operation versus 68 / 7,640 in the final legacy runs (zero net blocks/bytes for both). Static control traffic matches at 22 allocations / 3,472 bytes per operation. Allocation instrumentation and scheduler order affect traffic; do not compare absolute timings with prior sections' cross-date runs.

Presentation is now one equality-aware computed with explicit source and frame dependencies, allowing the scheduler to pull all changed fields before the final layout computation. A private u64 notification revision in SignalInner increments only on notification; a doc(hidden), untracked reader distinguishes source updates from frame ticks without an extra per-property effect. This is required for non-reflexive targets such as NaN and preserves silent-write behavior. The per-signal revision adds storage to the shared core signal arena; both benchmark arms include that core change, so the table does not isolate its overhead against the old core. Scope ownership, dynamic curve sampling on retarget, interruption from the current presented value, matrix interpolation and batched frame publication remain covered by tests. GPU drawing and real frame scheduling were not measured.

The original eight-scenario release baseline was also rerun: no-theme resolver/projection counts remain zero, updates do not rerun prepare/mount, and every scenario cleans its reactive arenas back to baseline. Regression tests additionally cover immediate multi-property theme replacement/removal, batched animated properties, interrupted animation/curve snapshots, synchronous reads inside batch, update metadata, NaN completion and revision semantics.

Reproduce by building with cargo test --manifest-path /root/mue/Cargo.toml -p mue-macroquad --lib --release transition_publication_compare --no-run, then run the emitted binary with taskset -c 0 and arguments transition_publication_compare --ignored --nocapture --test-threads=1. The legacy binding and selection guard are cfg(test) only.

## Check Dependency Traversal Without Snapshots

Measured 2026-11-03 on the same Linux/WSL2 host/compiler, release profile, CPU 0. The core check_traversal_compare benchmark compiles the prior dependencies.to_vec() traversal and the new indexed traversal in one binary. Each arm updates a source feeding one equality-aware parent and 1/2/8/47 derived inputs, then an aggregate observer. The unchanged fixture writes even values so the parent stays equal; the changed control propagates real changes. Three rounds use 64 warmups and 2,048 samples per arm, alternating order. Benchmarks run alone; a preliminary overlapping run was discarded for timing. Every sample checks the final value and equal callback counts; cleanup restores live resources.

| Unchanged fixture dependency width | Snapshot p50 range | Indexed p50 range | Snapshot allocations / 2,048 updates | Indexed allocations |
| --- | ---: | ---: | ---: | ---: |
| 1 | 187–188 ns | 163 ns | 8,192 | 4,096 |
| 2 | 224–225 ns | 191 ns | 10,240 | 4,096 |
| 8 | 542–558 ns | 441–452 ns | 26,624 | 8,192 |
| 47 | 2,108–2,116 ns | 1,629–1,653 ns | 110,592 | 12,288 |

The width-47 unchanged case is about 22% faster and removes 98,304 temporary allocations (89% of total calls). Allocated bytes fall from 3,588,096 to 2,048,000. A test-only per-operation allocator confirms each snapshot traversal allocates once per Check visit; indexed traversal has zero allocation for the traversal itself. The remaining allocations in this graph come from dirty callback execution/signal updates, not dependency scanning. Remaining allocations belong to signal value writes and pending scheduling, not dependency snapshots. Changed controls are effectively neutral: width 47 p50 is 7,266–7,516 ns versus 7,248–7,499 ns, with 112,640 allocations and 2,850,816 bytes in both arms. This is a focused scheduler workload, not a whole application frame-rate claim.

The Check path copies one SignalId and dependency revision under a short shared borrow, releases all arena borrows before recursive updates, and then rechecks the effect. If dependency structure changed it restarts from zero; disposed effects return and Clean/Dirty states are respected. A u64 dependency revision is added to EffectInner (8 bytes before layout/padding effects). Taking/replacing a dynamic tracker and removing dependencies during signal disposal advance the revision. Live signal/effect counts do not change. Tests cover upstream disposal of multiple earlier dependencies without skipping a remaining parent, disposal of the checking effect, and nested reruns replacing dependencies.

The existing empty_resolver_compare also passes against the final scheduler. For its no-match 16-node/512-update fixture, both resolver notification policies now perform 54,784 allocation/free calls and allocate 28,307,456 bytes, with zero net traffic. The old historical suppressed policy used 767,488 calls and 36,630,528 bytes; removal of dependency snapshots eliminates that allocation penalty. This count comparison spans builds, so absolute timing gains are established only by the same-binary core traversal comparison above. Property projections remain 385,024 for always-notify versus zero for empty-to-empty suppression; both resolve 8,192 times. Matching-rule control allocations remain equal between policies, values and callbacks pass, and all resource arenas return to baseline.

Final workspace tests (including cross-crate and doctests), all-targets check, release traversal comparison, release empty resolver comparison and the original eight-scenario release pipeline baseline passed. No new graphical Playground profile was captured in this scheduler-only change. Test allocator and snapshot reference compile only in the mue-core unit-test binary; ordinary builds do not carry the selector or allocation counters.

Reproduce with cargo test --manifest-path /root/mue/Cargo.toml -p mue-core --lib --release check_traversal_compare --no-run, then taskset -c 0 EMITTED_BINARY check_traversal_compare --ignored --nocapture --test-threads=1. Run the theme resolver comparison separately, never concurrently on the same pinned CPU.

## Empty Resolver Notifications

Measured 2026-11-03 on the Linux/WSL2 host above, pinned to CPU 0. The ignored empty_resolver_compare uses 16 mounted leaves, 32 warmups and 512 updates per arm, three rounds and alternating order. Both policies run in one instrumented release binary, with final Taffy widths checked after every update. The reference always publishes each resolver result; the optimized path suppresses notifications only when both snapshots have no visual declarations.

| Rule workload | Reference p50 / p95 range | Optimized p50 / p95 range | Resolver runs per arm | Property projections before -> after |
| --- | ---: | ---: | ---: | ---: |
| Different target type, no match | 99.95–106.94 / 113.72–143.11 us | 65.31–68.72 / 74.73–97.56 us | 8,192 | 385,024 -> 0 |
| Matching rule changes width | 119.31–130.75 / 146.46–169.72 us | 119.36–129.60 / 143.19–172.88 us | 8,192 | 385,024 -> 385,024 |

The no-match fixture improves p50 by about 35%; the matched control is effectively neutral. Every resolver still runs, tracks the current Theme/classes and selector/body dependencies, executes matched rule bodies and retains a fresh generation owner. The cached snapshot is always replaced, even when notification is suppressed, so empty rule bodies that allocate resources keep the correct lifetime. All transitions involving nonempty declarations notify, including visually identical values backed by newly generated signals. Later rule addition and removal still reach stable property bindings; old generation resources are released. Live arenas stay fixed during measured updates and return to baseline after cleanup.

Tradeoff: the no-match reference performs 382,464 allocation/free calls and allocates 33,550,336 bytes per 512 updates; suppression performs 767,488 calls and 36,630,528 bytes. These are zero-net traffic, not retained memory. Downstream Check-state dependency walks clone dependency lists even when projections do not run, so fewer computations currently cost more allocation calls. This was the historical allocation tradeoff; the later Check Dependency Traversal Without Snapshots section records its removal and supersedes this statement for the current implementation.

Playground background/panel/foreground helpers now share three thread-local reactive brush projections, rather than allocating a mapping on each call. dark_theme() also returns its existing read-only handle directly. Tests verify fixed projection counts, dark/light colors and repeated toggles. A temporary release Playground probe ran 12 theme toggles and discarded the first two: source-update time ranged 6.98–9.63 ms, p50 7.79 ms; subsequent App::frame ranged 0.46–0.65 ms. This sample preceded the final removal of the dark_theme identity mapping. An earlier one-off toggle was 13.4 ms; it is not a controlled before/after comparison. All ten pages were still built and retained. The profile hook has been removed.

Reproduce with cargo test --manifest-path /root/mue/Cargo.toml -p mue-macroquad --lib --release empty_resolver_compare --no-run, then run the emitted binary with taskset -c 0 and arguments empty_resolver_compare --ignored --nocapture --test-threads=1. The always-notify switch exists only under cfg(test).

## Deferred Transition Frame Resources

Measured 2026-11-03 on the same Linux/WSL2 host/compiler, pinned to CPU 0. The ignored deferred_tween_compare builds the old immediate-frame-allocation and current lazy-frame-allocation paths in one instrumented release binary. Each arm constructs, lays out and drops the same themed leaf with the default Theme transition (Instant). Three rounds use 32 warmups and 1,000 samples per arm, alternating order per sample. A separate idle-advance loop measures registry traversal with one mounted leaf and no active animation.

| Metric | Immediate frame allocation | Deferred frame allocation |
| --- | ---: | ---: |
| Build/layout/drop p50 range | 34.66–36.19 us | 29.90–31.28 us |
| Build/layout/drop p95 range | 41.82–43.14 us | 35.73–38.17 us |
| Live signals / effects | 121 / 89 | 88 / 89 |
| Allocations / build | 362 | 296 |
| Allocated bytes / build | 23,301 | 18,833 |
| Idle advance(1/60) p50 range | 362–364 ns | 25–27 ns |
| Idle advance(1/60) p95 range | 401–406 ns | 26–28 ns |

This representative node reduces 33 frame signals, 66 allocations (18.2%) and 4,468 allocated bytes (19.2%); measured build/layout/drop p50 improves by about 14%. The idle registry cost is reduced because bindings which have never animated do not register a driver. Effects are unchanged: the source-to-presentation computed remains subscribed so a dynamic curve can animate a later source update. Once a binding has animated, its frame signal and driver are retained and reused until the owning scope drops; the optimization applies to bindings which never actually animate.

The real Playground was sampled in a release build with an opt-in temporary per-page resource probe, then that probe was removed. It eagerly builds and retains all ten pages, as required by the existing pages() contract. Baseline total: 60,308 signals / 44,913 effects; after: 44,840 signals / 44,913 effects, a 25.6% signal reduction with all pages still present. Signals by page (before -> after): Layout 7,922 -> 5,887; Typography 8,904 -> 6,630; Graphics 3,926 -> 2,917; Motion 7,140 -> 5,311; Scrolling 14,124 -> 10,497; Input 2,119 -> 1,582; Map gestures 2,181 -> 1,618; Theme 2,301 -> 1,709; Style declarations 4,196 -> 3,126; Font licenses 1,321 -> 992. The baseline build sample was 43.1 ms and later optimized samples were 53.1–61.2 ms; these sequential runs were noisy and are not used as a paired performance claim; the controlled leaf benchmark above is the timing comparison. Graphics texture and Font licenses text work remain included in page times.

A one-off release interaction sample recorded selection source-update time about 0.1–0.2 ms, theme toggle 13.4 ms, zoom source-update about 0.1 ms, and subsequent App::frame about 1.3–8.3 ms. These are one scripted sequence, not percentiles or an A/B comparison; theme recomputation and page/font/layout work dominate over the selection setter itself. No page is now built lazily, no page state or transition semantics change, and dynamic transitions continue to allocate on first actual animation.

Reproduce the resource/performance comparison with cargo test --manifest-path /root/mue/Cargo.toml -p mue-macroquad --lib --release deferred_tween_compare --no-run, then run the emitted binary using taskset -c 0 and arguments deferred_tween_compare --ignored --nocapture --test-threads=1. The eager reference and selector are cfg(test) only. The Playground profile was a temporary source probe and is not part of the current example binary.

## Reproduction

From the workspace root, run the original baseline command above. For the resolver comparison alone, run `cargo test -p mue-macroquad --lib --release resolver_identity_scaling -- --ignored --nocapture --test-threads=1`. The ignored test prints timings, pipeline counters, allocation deltas, and reactive resource snapshots. Keep compiler/profile, instrumentation, sample counts, scenario code, and virtualization environment fixed when comparing revisions.
