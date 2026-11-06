//! Opt-in CPU baselines. Run with --release -- --ignored --nocapture.
mod deferred_tween;
mod layout_compare;
mod transition_compare;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct CountingAllocator;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

// This module is compiled only for the unit-test binary. Every operation is
// forwarded unchanged; counters use atomics and do not allocate or gate behavior.
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's valid layout directly to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Preserve the caller's allocation and zeroing contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the same allocation/layout required by System.
        unsafe { System.dealloc(pointer, layout) };
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        DEALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Forward the original allocation, layout, and requested size.
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            // Count successful realloc as old-layout release plus new allocation,
            // including in-place resizes; these are logical API byte counts.
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        resized
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Allocations {
    allocated: usize,
    deallocated: usize,
    allocated_bytes: usize,
    deallocated_bytes: usize,
}

impl Allocations {
    fn snapshot() -> Self {
        Self {
            allocated: ALLOCATIONS.load(Ordering::Relaxed),
            deallocated: DEALLOCATIONS.load(Ordering::Relaxed),
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        }
    }

    fn since(self, before: Self) -> Self {
        Self {
            allocated: self.allocated - before.allocated,
            deallocated: self.deallocated - before.deallocated,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            deallocated_bytes: self.deallocated_bytes - before.deallocated_bytes,
        }
    }

    fn add(&mut self, other: Self) {
        self.allocated += other.allocated;
        self.deallocated += other.deallocated;
        self.allocated_bytes += other.allocated_bytes;
        self.deallocated_bytes += other.deallocated_bytes;
    }

    fn print(self, label: &str) {
        println!(
            "    {label}: allocs={} frees={} allocated_bytes={} freed_bytes={} net_blocks={} net_bytes={}",
            self.allocated,
            self.deallocated,
            self.allocated_bytes,
            self.deallocated_bytes,
            self.allocated as i128 - self.deallocated as i128,
            self.allocated_bytes as i128 - self.deallocated_bytes as i128,
        );
    }
}

use mue_core::{
    effect::watch_effect,
    prop::PropValue,
    runtime::{ResourceCounts, resource_counts},
    scope::OwnedScope,
    signal::{Access, signal},
};
use taffy::AvailableSpace;

use super::{BuildNode, NodeContext};
use crate::{
    layout,
    node::{IntoNode, Node},
    paint,
    runtime::Runtime,
    style::{Style, Styleable},
};

const BUILD_WARMUP: usize = 32;
const BUILD_SAMPLES: usize = 250;

#[derive(Default)]
struct Counters {
    builds: Cell<usize>,
    inits: Cell<usize>,
    width_consumers: Cell<usize>,
    height_consumers: Cell<usize>,
    paint_builds: Cell<usize>,
}
impl PropValue for Counters {}
impl Counters {
    fn clear(&self) {
        self.builds.set(0);
        self.inits.set(0);
        self.width_consumers.set(0);
        self.height_consumers.set(0);
        self.paint_builds.set(0);
    }
}

#[derive(Clone, Copy, Debug)]
struct Timings {
    p50: u128,
    p95: u128,
}
fn summarize(mut samples: Vec<u128>) -> Timings {
    samples.sort_unstable();
    Timings {
        p50: samples[(samples.len() - 1) * 50 / 100],
        p95: samples[(samples.len() - 1) * 95 / 100],
    }
}
fn delta(after: usize, before: usize) -> isize {
    after as isize - before as isize
}
fn print_counts(before: ResourceCounts, live: ResourceCounts, after: ResourceCounts) {
    println!(
        "    live delta scopes={} signals={} effects={}; final delta scopes={} signals={} effects={}",
        delta(live.scopes, before.scopes),
        delta(live.signals, before.signals),
        delta(live.effects, before.effects),
        delta(after.scopes, before.scopes),
        delta(after.signals, before.signals),
        delta(after.effects, before.effects),
    );
}

fn report_build(
    name: &str,
    timings: Timings,
    counters: &Counters,
    before: ResourceCounts,
    live: ResourceCounts,
    after: ResourceCounts,
) {
    println!(
        "{name}: p50={}ns p95={}ns; width_notify={} height_notify={} paint_build={}",
        timings.p50,
        timings.p95,
        counters.width_consumers.get(),
        counters.height_consumers.get(),
        counters.paint_builds.get(),
    );
    println!(
        "    synchronous builds={} initializers={} layout_calls={BUILD_SAMPLES}",
        counters.builds.get(),
        counters.inits.get()
    );
    print_counts(before, live, after);
}

#[mue_macros::node]
fn baseline_leaf(ctx: NodeContext, counters: Rc<Counters>) -> Node {
    let counters = counters.get_clone();
    counters.builds.set(counters.builds.get() + 1);
    ctx.finish(Style::new().width(120.).height(24.), move |style| {
        counters.inits.set(counters.inits.get() + 1);
        let width = style.width.clone().unwrap();
        let height = style.height.clone().unwrap();
        let width_counter = counters.clone();
        watch_effect(move || {
            width_counter
                .width_consumers
                .set(width_counter.width_consumers.get() + 1);
            let _ = width.get_clone();
        });
        let height_counter = counters.clone();
        watch_effect(move || {
            height_counter
                .height_consumers
                .set(height_counter.height_consumers.get() + 1);
            let _ = height.get_clone();
        });

        let layout = layout::use_layout(style);
        let paint = paint::use_paint(style);
        let paint_counter = counters.clone();
        let _shapes = paint.build(move |_| {
            paint_counter
                .paint_builds
                .set(paint_counter.paint_builds.get() + 1);
        });
        let _ = layout;
    })
}

fn measure_layout(node: &Node) {
    let layout_id = Runtime::with(|rt| rt.node(node.id).layout_id.expect("benchmark layout"));
    Runtime::with_taffy_mut(|tree| {
        tree.compute_layout(
            layout_id,
            taffy::Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            },
        )
        .expect("headless benchmark layout");
    });
}
fn max_counts(a: ResourceCounts, b: ResourceCounts) -> ResourceCounts {
    ResourceCounts {
        scopes: a.scopes.max(b.scopes),
        signals: a.signals.max(b.signals),
        effects: a.effects.max(b.effects),
    }
}

fn run_build_case(name: &str) {
    let counters = Rc::new(Counters::default());
    let before = resource_counts();
    for _ in 0..BUILD_WARMUP {
        let node = baseline_leaf(counters.clone()).into_node();
        measure_layout(&node);
        drop(node);
    }
    counters.clear();

    let mut live = before;
    let mut samples = Vec::with_capacity(BUILD_SAMPLES);
    let mut measured_allocations = Allocations::default();
    for _ in 0..BUILD_SAMPLES {
        let allocation_before = Allocations::snapshot();
        let start = Instant::now();
        let node = baseline_leaf(counters.clone()).into_node();
        measure_layout(&node);
        live = max_counts(live, resource_counts());
        drop(node);
        let elapsed = start.elapsed().as_nanos();
        measured_allocations.add(Allocations::snapshot().since(allocation_before));
        samples.push(elapsed);
    }
    let after = resource_counts();
    report_build(name, summarize(samples), &counters, before, live, after);
    measured_allocations.print("measured build operations incl. node drop");
    assert_eq!(after, before, "{name} leaked reactive resources");

    let cleanup_before = Allocations::snapshot();
    crate::transition::advance(1.);
    drop(counters);
    Allocations::snapshot()
        .since(cleanup_before)
        .print("post-drop cleanup + transition driver maintenance");
    assert_eq!(
        resource_counts(),
        before,
        "{name} retained reactive resources after maintenance"
    );
}

#[test]
#[ignore = "CPU baseline; run explicitly with --release -- --ignored --nocapture"]
fn cpu_performance_baseline() {
    println!(
        "static build baseline (node mount + headless layout + drop), {BUILD_SAMPLES} samples after {BUILD_WARMUP} warmups"
    );
    run_build_case("static build");
}

fn run_instant_transition_case(dynamic: bool) -> ResourceCounts {
    use mue_core::prop::Prop;

    use crate::transition::Transition;

    let baseline = resource_counts();
    let owner = OwnedScope::new();
    // Keep an identical external source alive in both cases, outside measured builds.
    let source = owner.run(|| signal(Transition::Instant));
    let curve = if dynamic {
        source.into()
    } else {
        Prop::Static(Transition::Instant)
    };
    let width = owner.run(|| signal(crate::style::Size::Px(120.)));
    let counters = Rc::new(Counters::default());
    let mount = || {
        baseline_leaf(counters.clone())
            .width(width)
            .transition(curve)
            .into_node()
    };
    let before = resource_counts();
    for _ in 0..BUILD_WARMUP {
        let node = mount();
        measure_layout(&node);
        drop(node);
    }
    assert_eq!(resource_counts(), before, "Instant warmup leaked resources");
    counters.clear();
    let mut samples = Vec::with_capacity(BUILD_SAMPLES);
    let mut measured_allocations = Allocations::default();
    let mut live = before;
    for _ in 0..BUILD_SAMPLES {
        let allocation_before = Allocations::snapshot();
        let start = Instant::now();
        let node = mount();
        measure_layout(&node);
        let size = Runtime::with(|rt| {
            let layout_id = rt.node(node.id).layout_id.expect("Instant baseline layout");
            rt.taffy.borrow().layout(layout_id).unwrap().size
        });
        live = max_counts(live, resource_counts());
        drop(node);
        let elapsed = start.elapsed().as_nanos();
        measured_allocations.add(Allocations::snapshot().since(allocation_before));
        samples.push(elapsed);
        assert_eq!(size.width, 120.);
        assert_eq!(size.height, 24.);
    }
    let after = resource_counts();
    assert_eq!(after, before, "Instant baseline leaked reactive resources");
    assert_eq!(counters.builds.get(), BUILD_SAMPLES);
    assert_eq!(counters.inits.get(), BUILD_SAMPLES);
    assert!(counters.width_consumers.get() >= BUILD_SAMPLES);
    assert!(counters.height_consumers.get() >= BUILD_SAMPLES);
    assert!(
        !source.is_disposed(),
        "node drop disposed the external curve"
    );
    assert_eq!(source.get_clone(), Transition::Instant);
    let name = if dynamic {
        "Instant / explicit dynamic curve currently Instant"
    } else {
        "Instant / explicit static curve"
    };
    report_build(name, summarize(samples), &counters, before, live, after);
    measured_allocations.print("measured build + layout + node drop");

    let cleanup_before = Allocations::snapshot();
    drop(owner);
    drop(counters);
    crate::transition::advance(1.);
    Allocations::snapshot()
        .since(cleanup_before)
        .print("post-drop cleanup + transition driver maintenance");
    assert_eq!(
        resource_counts(),
        baseline,
        "Instant baseline cleanup leaked"
    );
    ResourceCounts {
        scopes: live.scopes - before.scopes,
        signals: live.signals - before.signals,
        effects: live.effects - before.effects,
    }
}

#[test]
#[ignore = "Instant transition baseline; run with --release -- --ignored instant_transition_baseline --nocapture --test-threads=1"]
fn instant_transition_baseline() {
    println!(
        "Instant transitions: same-binary explicit static vs dynamic curve currently Instant; {BUILD_SAMPLES} build + layout + drop samples after {BUILD_WARMUP} warmups; identical explicit width signal on both paths; process-wide allocator requires this test alone and --test-threads=1; no timing thresholds"
    );
    // A dynamic curve currently equal to Instant may animate after a later update.
    let static_curve = run_instant_transition_case(false);
    let dynamic_curve = run_instant_transition_case(true);
    assert!(
        static_curve.signals < dynamic_curve.signals,
        "static Instant did not reduce live signals"
    );
    assert!(
        static_curve.effects < dynamic_curve.effects,
        "static Instant did not reduce live effects"
    );
}
