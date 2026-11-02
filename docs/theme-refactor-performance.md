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

## Reproduction

From the workspace root, run the original baseline command above. For the resolver comparison alone, run `cargo test -p mue-macroquad --lib --release resolver_identity_scaling -- --ignored --nocapture --test-threads=1`. The ignored test prints timings, pipeline counters, allocation deltas, and reactive resource snapshots. Keep compiler/profile, instrumentation, sample counts, scenario code, and virtualization environment fixed when comparing revisions.
