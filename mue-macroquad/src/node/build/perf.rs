//! Opt-in CPU baselines. Run with --release -- --ignored --nocapture.
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

fn print_theme_stats() {
    let stats = crate::runtime::theme_stats();
    println!(
        "    exact test stats: resolver_runs={} projection_runs={} transition_binds={}",
        stats.resolver_runs, stats.projection_runs, stats.transition_binds,
    );
}

use mue_core::{
    effect::watch_effect,
    prop::PropValue,
    runtime::{ResourceCounts, resource_counts},
    scope::OwnedScope,
    signal::{Access, signal},
};
use taffy::AvailableSpace;

use super::{AppBuildContext, Node, NodePlan};
use crate::{
    layout,
    node::IntoNode,
    paint,
    runtime::Runtime,
    style::{Style, Styleable, StyleableExt},
    theme::{ClassName, ComponentRef, Rule, Selector, Specificity, Theme, ThemeTarget, Themeable},
};

const BUILD_WARMUP: usize = 32;
const BUILD_SAMPLES: usize = 250;
const UPDATE_WARMUP: usize = 64;
const UPDATE_SAMPLES: usize = 2048;

#[derive(Default)]
struct Counters {
    prepares: Cell<usize>,
    mounts: Cell<usize>,
    selectors: Cell<usize>,
    rule_bodies: Cell<usize>,
    width_consumers: Cell<usize>,
    height_consumers: Cell<usize>,
    paint_builds: Cell<usize>,
}
impl PropValue for Counters {}
impl Counters {
    fn clear(&self) {
        self.prepares.set(0);
        self.mounts.set(0);
        self.selectors.set(0);
        self.rule_bodies.set(0);
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
        "{name}: p50={}ns p95={}ns; selectors={} rule_bodies={} width_notify={} height_notify={} paint_build={}",
        timings.p50,
        timings.p95,
        counters.selectors.get(),
        counters.rule_bodies.get(),
        counters.width_consumers.get(),
        counters.height_consumers.get(),
        counters.paint_builds.get(),
    );
    println!(
        "    prepares={} mounts={} layout_calls={BUILD_SAMPLES}",
        counters.prepares.get(),
        counters.mounts.get()
    );
    print_counts(before, live, after);
}

fn theme_rule(
    counters: &Rc<Counters>,
    specificity: Specificity,
    predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
    declarations: impl Fn() -> Style + 'static,
) -> Rule {
    let selectors = counters.clone();
    let declarations_counter = counters.clone();
    Rule::new(
        vec![Selector::new(specificity, move |target| {
            selectors.selectors.set(selectors.selectors.get() + 1);
            predicate(target)
        })],
        move || {
            declarations_counter
                .rule_bodies
                .set(declarations_counter.rule_bodies.get() + 1);
            declarations()
        },
    )
}

#[mue_macros::node]
#[theme]
fn baseline_leaf(counters: Rc<Counters>) -> NodePlan<impl FnOnce(&mut Style)> {
    let counters = counters.get_clone();
    counters.prepares.set(counters.prepares.get() + 1);
    NodePlan::new(Style::new().width(120.).height(24.), move |style| {
        counters.mounts.set(counters.mounts.get() + 1);
        let width = style.declared_width().unwrap();
        let height = style.declared_height().unwrap();
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

#[derive(Default)]
struct Unmatched;
impl ThemeTarget for Unmatched {
    type Options = ();
}

#[mue_macros::node]
#[theme(active)]
fn option_leaf(#[model] active: bool, counters: Rc<Counters>) -> NodePlan<impl FnOnce(&mut Style)> {
    let counters = counters.get_clone();
    counters.prepares.set(counters.prepares.get() + 1);
    NodePlan::new(Style::new().width(120.).height(24.), move |style| {
        counters.mounts.set(counters.mounts.get() + 1);
        let width = style.declared_width().unwrap();
        let width_counter = counters.clone();
        watch_effect(move || {
            width_counter
                .width_consumers
                .set(width_counter.width_consumers.get() + 1);
            let _ = width.get_clone();
        });
        let height = style.declared_height().unwrap();
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
        let _ = (active, layout);
    })
}

fn mount(theme: Option<Theme>, counters: Rc<Counters>) -> Node {
    let build = move || baseline_leaf(counters).into_node();
    if let Some(theme) = theme {
        AppBuildContext::with_theme(theme.into(), build)
    } else {
        build()
    }
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
fn run_build_case(name: &str, make_theme: impl FnOnce(&Rc<Counters>) -> Option<Theme>) {
    let counters = Rc::new(Counters::default());
    let theme = make_theme(&counters);
    let before = resource_counts();
    for _ in 0..BUILD_WARMUP {
        let node = mount(theme.clone(), counters.clone());
        measure_layout(&node);
        drop(node);
    }
    counters.clear();

    let mut live = before;
    let mut samples = Vec::with_capacity(BUILD_SAMPLES);
    let mut measured_allocations = Allocations::default();
    crate::runtime::reset_theme_stats();
    for _ in 0..BUILD_SAMPLES {
        let allocation_before = Allocations::snapshot();
        let start = Instant::now();
        let node = mount(theme.clone(), counters.clone());
        measure_layout(&node);
        live = max_counts(live, resource_counts());
        drop(node);
        let elapsed = start.elapsed().as_nanos();
        measured_allocations.add(Allocations::snapshot().since(allocation_before));
        samples.push(elapsed);
    }
    let after = resource_counts();
    report_build(name, summarize(samples), &counters, before, live, after);
    print_theme_stats();
    measured_allocations.print("measured build operations incl. node drop");
    assert_eq!(after, before, "{name} leaked reactive resources");

    let cleanup_before = Allocations::snapshot();
    crate::transition::advance(1.);
    drop(theme);
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

fn class_set(names: &[&str]) -> std::collections::HashSet<ClassName> {
    names
        .iter()
        .map(|name| ClassName::try_from(*name).unwrap())
        .collect()
}
fn class_theme(counters: &Rc<Counters>) -> Theme {
    Theme::new(vec![
        theme_rule(
            counters,
            Specificity(0, 1, 0),
            move |target| target.has_class("selected"),
            || Style::new().height(25.),
        ),
        theme_rule(
            counters,
            Specificity(0, 0, 1),
            move |target| target.hovered(),
            || Style::new().width(121.),
        ),
    ])
}

fn measure_updates(
    name: &str,
    node: &Node,
    counters: &Counters,
    baseline: ResourceCounts,
    mut update: impl FnMut(usize),
) {
    for index in 0..UPDATE_WARMUP {
        update(index);
        measure_layout(node);
    }
    counters.clear();
    let live = resource_counts();
    let mut samples = Vec::with_capacity(UPDATE_SAMPLES);
    let mut measured_allocations = Allocations::default();
    crate::runtime::reset_theme_stats();
    for index in 0..UPDATE_SAMPLES {
        let allocation_before = Allocations::snapshot();
        let start = Instant::now();
        update(index + UPDATE_WARMUP);
        measure_layout(node);
        let elapsed = start.elapsed().as_nanos();
        measured_allocations.add(Allocations::snapshot().since(allocation_before));
        samples.push(elapsed);
    }
    let after_updates = resource_counts();
    assert_eq!(
        after_updates, live,
        "{name} accumulated reactive resources during updates"
    );
    assert_eq!(counters.prepares.get(), 0, "updates reran prepare");
    assert_eq!(counters.mounts.get(), 0, "updates reran mount");
    let timings = summarize(samples);
    println!(
        "{name}: p50={}ns p95={}ns per update; selectors={} rule_bodies={} width_notify={} height_notify={} paint_build={}",
        timings.p50,
        timings.p95,
        counters.selectors.get(),
        counters.rule_bodies.get(),
        counters.width_consumers.get(),
        counters.height_consumers.get(),
        counters.paint_builds.get(),
    );
    println!(
        "    prepares={} mounts={} layout_calls={UPDATE_SAMPLES}; live snapshots before/after measured updates:",
        counters.prepares.get(),
        counters.mounts.get()
    );
    print_counts(baseline, live, after_updates);
    print_theme_stats();
    measured_allocations.print("measured update operations");
}

#[mue_macros::node]
#[theme]
fn hover_leaf(counters: Rc<Counters>) -> NodePlan<impl FnOnce(&mut Style)> {
    let counters = counters.get_clone();
    counters.prepares.set(counters.prepares.get() + 1);
    NodePlan::new(Style::new().width(120.).height(24.), move |style| {
        counters.mounts.set(counters.mounts.get() + 1);
        let width = style.declared_width().unwrap();
        let counter = counters.clone();
        watch_effect(move || {
            counter
                .width_consumers
                .set(counter.width_consumers.get() + 1);
            let _ = width.get_clone();
        });
        let height = style.declared_height().unwrap();
        let counter = counters.clone();
        watch_effect(move || {
            counter
                .height_consumers
                .set(counter.height_consumers.get() + 1);
            let _ = height.get_clone();
        });
        let _layout = layout::use_layout(style);
        let paint = paint::use_paint(style);
        let counter = counters.clone();
        let _shapes =
            paint.build(move |_| counter.paint_builds.set(counter.paint_builds.get() + 1));
    })
}

#[mue_macros::component]
#[theme]
fn hovered_leaf(counters: Rc<Counters>) -> impl crate::node::Component {
    hover_leaf(counters).with_hovered(|builder, _hovered| builder)
}

fn run_class_hover_case() {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let class_source = owner.run(|| signal(class_set(&[])));
    let counters = Rc::new(Counters::default());
    let builder = hovered_leaf(counters.clone()).classes(class_source);
    let theme = class_theme(&counters);
    let node = AppBuildContext::with_theme(theme.into(), || builder.into_node());
    measure_layout(&node);
    measure_updates("class/hover toggle", &node, &counters, baseline, |index| {
        class_source.set(if index % 2 == 0 {
            class_set(&["selected"])
        } else {
            class_set(&[])
        });
        let aliases = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = &nodes[node.id];
            std::iter::once(node.hovered.unwrap())
                .chain(node.hovered_aliases.iter().copied())
                .collect::<Vec<_>>()
        });
        mue_core::batch(|| {
            for alias in aliases {
                alias.set(index % 2 == 0);
            }
        });
    });
    let before_cleanup = Allocations::snapshot();
    drop(node);
    drop(owner);
    drop(counters);
    crate::transition::advance(1.);
    Allocations::snapshot()
        .since(before_cleanup)
        .print("post-drop cleanup + transition driver maintenance (setup excluded)");
    let after_cleanup = resource_counts();
    println!("    after cleanup:");
    print_counts(baseline, after_cleanup, after_cleanup);
    assert_eq!(after_cleanup, baseline);
}

fn run_option_case() {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let active = owner.run(|| signal(false));
    let counters = Rc::new(Counters::default());
    let theme = Theme::new(vec![theme_rule(
        &counters,
        Specificity(0, 0, 1),
        move |target| {
            let _ = target.options::<OptionLeaf>().unwrap().active();
            true
        },
        || Style::new().opacity(1.),
    )]);
    let node = AppBuildContext::with_theme(theme.into(), || {
        option_leaf(active, counters.clone()).into_node()
    });
    measure_layout(&node);
    measure_updates(
        "option changes, same visual result",
        &node,
        &counters,
        baseline,
        |index| active.set(index % 2 == 0),
    );
    let before_cleanup = Allocations::snapshot();
    drop(node);
    drop(owner);
    drop(counters);
    crate::transition::advance(1.);
    Allocations::snapshot()
        .since(before_cleanup)
        .print("post-drop cleanup + transition driver maintenance (setup excluded)");
    let after_cleanup = resource_counts();
    println!("    after cleanup:");
    print_counts(baseline, after_cleanup, after_cleanup);
    assert_eq!(after_cleanup, baseline);
}

fn run_dynamic_theme_case() {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let source = owner.run(|| signal(Theme::default()));
    let counters = Rc::new(Counters::default());
    let matching = theme_rule(
        &counters,
        Specificity(0, 0, 1),
        |target| target.is::<BaselineLeaf>(),
        || Style::new().height(25.),
    );
    let on = Theme::new(vec![matching]);
    let node = AppBuildContext::with_theme(source.into(), || {
        baseline_leaf(counters.clone()).into_node()
    });
    measure_layout(&node);
    measure_updates(
        "dynamic rule add/remove",
        &node,
        &counters,
        baseline,
        |index| {
            source.set(if index % 2 == 0 {
                on.clone()
            } else {
                Theme::default()
            });
        },
    );
    let before_cleanup = Allocations::snapshot();
    drop(node);
    drop(owner);
    drop(counters);
    crate::transition::advance(1.);
    Allocations::snapshot()
        .since(before_cleanup)
        .print("post-drop cleanup + transition driver maintenance (setup excluded)");
    let after_cleanup = resource_counts();
    println!("    after cleanup:");
    print_counts(baseline, after_cleanup, after_cleanup);
    assert_eq!(after_cleanup, baseline);
}

fn run_derived_declaration_case() {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let source = owner.run(|| signal(Theme::default()));
    let value = owner.run(|| signal(25.));
    let counters = Rc::new(Counters::default());
    let selector = theme_rule(
        &counters,
        Specificity(0, 0, 1),
        |target| target.is::<BaselineLeaf>(),
        move || Style::new().height(value.map(|height| height)),
    );
    let on = Theme::new(vec![selector]);
    let node = AppBuildContext::with_theme(source.into(), || {
        baseline_leaf(counters.clone()).into_node()
    });
    measure_layout(&node);
    measure_updates(
        "dynamic rule with owned derived declaration",
        &node,
        &counters,
        baseline,
        |index| {
            source.set(if index % 2 == 0 {
                on.clone()
            } else {
                Theme::default()
            });
        },
    );
    let before_cleanup = Allocations::snapshot();
    drop(node);
    assert!(!source.is_disposed());
    assert!(!value.is_disposed());
    drop(owner);
    drop(counters);
    crate::transition::advance(1.);
    Allocations::snapshot()
        .since(before_cleanup)
        .print("post-drop cleanup + transition driver maintenance (setup excluded)");
    let after_cleanup = resource_counts();
    println!("    after cleanup:");
    print_counts(baseline, after_cleanup, after_cleanup);
    assert_eq!(after_cleanup, baseline);
}

#[test]
#[ignore = "CPU baseline; run explicitly with --release -- --ignored --nocapture"]
fn cpu_performance_baseline() {
    println!(
        "theme build baselines (node mount + headless layout + drop), {BUILD_SAMPLES} samples after {BUILD_WARMUP} warmups"
    );
    run_build_case("no theme", |_| None);
    run_build_case("empty static theme", |_| Some(Theme::default()));
    run_build_case("theme with no match", |counters| {
        Some(Theme::new(vec![theme_rule(
            counters,
            Specificity(0, 0, 1),
            |target| target.is::<Unmatched>(),
            || Style::new().width(121.),
        )]))
    });
    run_build_case("static theme hit", |counters| {
        Some(Theme::new(vec![theme_rule(
            counters,
            Specificity(0, 0, 1),
            |target| target.is::<BaselineLeaf>(),
            || Style::new().width(121.),
        )]))
    });
    println!(
        "update baselines: {UPDATE_SAMPLES} individually timed toggles; each operation also computes the headless layout"
    );
    run_class_hover_case();
    run_option_case();
    run_dynamic_theme_case();
    run_derived_declaration_case();
    println!(
        "allocation counts: test-only System forwarding allocator with atomics; successful realloc counts as allocation+free. GPU draw/paint execution and transition interpolation: not measured; post-drop transition driver maintenance is counted only in cleanup"
    );
}

// Resolver-only measurements leave cpu_performance_baseline unchanged.
// Filter to this test and pass --test-threads=1: allocation counters are process-wide.
const RESOLVER_KEYS: usize = 64;
const RESOLVER_WARMUP: usize = 16;
const RESOLVER_SAMPLES: usize = 256;

struct ResolverTarget<const KEY: usize>;
impl<const KEY: usize> ThemeTarget for ResolverTarget<KEY> {
    type Options = usize;
}

struct ResolverCounters {
    predicates: Cell<usize>,
    body_calls: Cell<usize>,
    // Per-rule counts detect duplicate execution even if another rule was skipped.
    bodies: Vec<Cell<usize>>,
}
impl ResolverCounters {
    fn new(rules: usize) -> Self {
        Self {
            predicates: Cell::new(0),
            body_calls: Cell::new(0),
            bodies: (0..rules).map(|_| Cell::new(0)).collect(),
        }
    }

    fn clear(&self) {
        self.predicates.set(0);
        self.body_calls.set(0);
        for body in &self.bodies {
            body.set(0);
        }
    }

    fn assert_bodies(&self, matched: &[bool], operations: usize) {
        assert_eq!(self.bodies.len(), matched.len());
        for (index, (body, expected)) in self.bodies.iter().zip(matched).enumerate() {
            assert_eq!(
                body.get(),
                usize::from(*expected) * operations,
                "rule {index} must execute once per matching operation",
            );
        }
    }
}

fn resolver_selector<const KEY: usize>(
    specificity: Specificity,
    counters: Rc<ResolverCounters>,
) -> Selector {
    Selector::for_target::<ResolverTarget<KEY>>(specificity, move |target| {
        counters.predicates.set(counters.predicates.get() + 1);
        std::hint::black_box(target.has_class("selected"))
    })
}
type ResolverSelectorFactory = fn(Specificity, Rc<ResolverCounters>) -> Selector;

fn resolver_keys() -> [(crate::theme::ThemeKey, ResolverSelectorFactory); RESOLVER_KEYS] {
    macro_rules! keys {
        ($($key:literal),* $(,)?) => {
            [$(
                (ResolverTarget::<$key>::key(), resolver_selector::<$key> as ResolverSelectorFactory),
            )*]
        };
    }
    keys![
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
    ]
}

#[derive(Clone, Copy, Debug)]
enum ResolverWorkload {
    Sparse,
    Universal,
    ClassOnly,
    CustomUnkeyed,
    Mixed,
    Dense,
}

#[derive(Clone, Copy, Debug)]
struct ResolverCase {
    workload: ResolverWorkload,
    rules: usize,
    branches: usize,
    targets: usize,
}

fn resolver_unkeyed(
    kind: usize,
    specificity: Specificity,
    counters: Rc<ResolverCounters>,
) -> Selector {
    Selector::new(specificity, move |target| {
        counters.predicates.set(counters.predicates.get() + 1);
        std::hint::black_box(match kind {
            0 => true,
            1 => target.has_class("selected"),
            // The opaque closure intentionally remains unkeyed despite checking a type.
            _ => target
                .options::<ResolverTarget<0>>()
                .is_some_and(|value| *value == 7),
        })
    })
}

fn resolver_rule(
    index: usize,
    case: ResolverCase,
    counters: &Rc<ResolverCounters>,
    keys: &[(crate::theme::ThemeKey, ResolverSelectorFactory); RESOLVER_KEYS],
) -> Rule {
    let selectors = (0..case.branches)
        .map(|branch| {
            let specificity = Specificity(0, ((index + branch) % 3) as u32, 1);
            let kind = match case.workload {
                ResolverWorkload::Universal => Some(0),
                ResolverWorkload::ClassOnly => Some(1),
                ResolverWorkload::CustomUnkeyed => Some(2),
                ResolverWorkload::Mixed if branch != 0 => Some(branch - 1),
                _ => None,
            };
            if let Some(kind) = kind {
                resolver_unkeyed(kind, specificity, counters.clone())
            } else {
                let key = match case.workload {
                    ResolverWorkload::Dense => 0,
                    _ => (index + branch * 17) % RESOLVER_KEYS,
                };
                (keys[key].1)(specificity, counters.clone())
            }
        })
        .collect();
    let counters = counters.clone();
    Rule::new(selectors, move || {
        counters.body_calls.set(counters.body_calls.get() + 1);
        counters.bodies[index].set(counters.bodies[index].get() + 1);
        // These are the only fields emitted; equivalence checks compare both.
        if index % 2 == 0 {
            Style::new().width((index + 1) as f32)
        } else {
            Style::new().height((index + 1) as f32)
        }
    })
}

#[derive(Default)]
struct ResolverMeasurements {
    samples: Vec<u128>,
    allocations: Allocations,
    predicates: usize,
    bodies: usize,
}
impl ResolverMeasurements {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(RESOLVER_SAMPLES),
            ..Self::default()
        }
    }

    fn record(&mut self, resolve: impl FnOnce() -> Style, counters: &ResolverCounters) {
        let predicate_before = counters.predicates.get();
        let body_before = counters.body_calls.get();
        let allocation_before = Allocations::snapshot();
        let start = Instant::now();
        let result = std::hint::black_box(resolve());
        let elapsed = start.elapsed().as_nanos();
        self.predicates += counters.predicates.get() - predicate_before;
        self.bodies += counters.body_calls.get() - body_before;
        self.samples.push(elapsed);
        drop(result);
        // Include result disposal in allocation accounting, outside the timer.
        self.allocations
            .add(Allocations::snapshot().since(allocation_before));
    }

    fn report(self, label: &str) {
        let operations = self.samples.len() as f64;
        let timings = summarize(self.samples);
        println!(
            "    {label}: p50={}ns p95={}ns predicates/op={:.2} bodies/op={:.2} allocs/op={:.2} frees/op={:.2} allocated_bytes/op={:.2} freed_bytes/op={:.2}",
            timings.p50,
            timings.p95,
            self.predicates as f64 / operations,
            self.bodies as f64 / operations,
            self.allocations.allocated as f64 / operations,
            self.allocations.deallocated as f64 / operations,
            self.allocations.allocated_bytes as f64 / operations,
            self.allocations.deallocated_bytes as f64 / operations,
        );
    }
}

fn measure_theme_setup(rules: &[Rule]) {
    const WARMUP: usize = 16;
    const SAMPLES: usize = 64;
    let mut times = Vec::with_capacity(SAMPLES);
    let mut allocations = Allocations::default();

    for _ in 0..WARMUP {
        let cloned_rules = rules.to_vec();
        drop(Theme::new(cloned_rules));
    }
    for _ in 0..SAMPLES {
        // Rule cloning is shared setup, excluded from both measurement windows.
        let cloned_rules = rules.to_vec();
        let before = Allocations::snapshot();
        let start = Instant::now();
        let theme = std::hint::black_box(Theme::new(cloned_rules));
        let elapsed = start.elapsed().as_nanos();
        drop(theme);
        allocations.add(Allocations::snapshot().since(before));
        times.push(elapsed);
    }

    let timings = summarize(times);
    let operations = SAMPLES as f64;
    println!(
        "    Theme::new setup (prebuilt Rule clone excluded): p50={}ns p95={}ns allocs/op={:.2} frees/op={:.2} allocated_bytes/op={:.2} freed_bytes/op={:.2}; Theme drop is allocation-counted but untimed",
        timings.p50,
        timings.p95,
        allocations.allocated as f64 / operations,
        allocations.deallocated as f64 / operations,
        allocations.allocated_bytes as f64 / operations,
        allocations.deallocated_bytes as f64 / operations,
    );
}

fn resolver_case(case: ResolverCase) {
    use std::hint::black_box;

    let keys = resolver_keys();
    let counters = Rc::new(ResolverCounters::new(case.rules));
    let theme = Theme::new(
        (0..case.rules)
            .map(|index| resolver_rule(index, case, &counters, &keys))
            .collect(),
    );
    let classes = class_set(&["selected"]);
    let options = 7usize;
    // Sparse inputs have distinct keys; dense inputs repeat the same identity.
    let components = (0..case.targets)
        .map(|index| {
            let key = if matches!(case.workload, ResolverWorkload::Dense) {
                0
            } else {
                index
            };
            ComponentRef::new(keys[key].0, &options, &classes, None)
        })
        .collect::<Vec<_>>();

    // Exhaustively derive matching rule ids outside measurement; body counters
    // independently assert each id executes once despite matching branches/aliases.
    let expected_bodies = theme
        .rules()
        .iter()
        .map(|rule| {
            rule.selectors().iter().any(|selector| {
                components
                    .iter()
                    .any(|component| selector.matches(*component))
            })
        })
        .collect::<Vec<_>>();
    counters.clear();
    let resolved = theme.resolve(&components);
    counters.assert_bodies(&expected_bodies, 1);
    counters.clear();
    let linear = theme.resolve_linear(&components);
    counters.assert_bodies(&expected_bodies, 1);
    assert_eq!(
        (
            resolved.declared_width().map(|value| value.get_clone()),
            resolved.declared_height().map(|value| value.get_clone())
        ),
        (
            linear.declared_width().map(|value| value.get_clone()),
            linear.declared_height().map(|value| value.get_clone())
        ),
        "resolver output differs: {case:?}",
    );
    drop((resolved, linear));

    for _ in 0..RESOLVER_WARMUP {
        black_box(theme.resolve(black_box(&components)));
        black_box(theme.resolve_linear(black_box(&components)));
    }
    counters.clear();
    let mut indexed = ResolverMeasurements::new();
    let mut linear = ResolverMeasurements::new();
    for sample in 0..RESOLVER_SAMPLES {
        // Alternate ordering so neither implementation is always measured first.
        if sample % 2 == 0 {
            indexed.record(
                || black_box(&theme).resolve(black_box(&components)),
                &counters,
            );
            linear.record(
                || black_box(&theme).resolve_linear(black_box(&components)),
                &counters,
            );
        } else {
            linear.record(
                || black_box(&theme).resolve_linear(black_box(&components)),
                &counters,
            );
            indexed.record(
                || black_box(&theme).resolve(black_box(&components)),
                &counters,
            );
        }
    }
    counters.assert_bodies(&expected_bodies, RESOLVER_SAMPLES * 2);
    assert_eq!(
        indexed.bodies, linear.bodies,
        "body count differs: {case:?}"
    );
    println!("resolver_identity_scaling {case:?}");
    measure_theme_setup(theme.rules());
    indexed.report("resolve");
    linear.report("resolve_linear");
}

#[test]
#[ignore = "Resolver scaling; run with --release -- --ignored resolver_identity_scaling --nocapture --test-threads=1"]
fn resolver_identity_scaling() {
    println!(
        "resolver only, {RESOLVER_SAMPLES} samples/path after {RESOLVER_WARMUP} warmups; same instrumented binary, 64 identity keys; setup excluded, result drop allocation-counted but untimed; process-wide allocation counters require this test alone and --test-threads=1"
    );
    // Six representative sparse cases, not the full Cartesian product.
    for (rules, branches, targets) in [
        (16, 1, 1),
        (16, 4, 8),
        (256, 1, 8),
        (256, 4, 1),
        (4096, 1, 1),
        (4096, 4, 8),
    ] {
        resolver_case(ResolverCase {
            workload: ResolverWorkload::Sparse,
            rules,
            branches,
            targets,
        });
    }
    for workload in [
        ResolverWorkload::Universal,
        ResolverWorkload::ClassOnly,
        ResolverWorkload::CustomUnkeyed,
        ResolverWorkload::Mixed,
        ResolverWorkload::Dense,
    ] {
        resolver_case(ResolverCase {
            workload,
            rules: 256,
            branches: 4,
            targets: 8,
        });
    }
    println!(
        "allocations: existing test-only System forwarding allocator; successful realloc counts as allocation+free; predicate/body Cell counters instrument both paths equally; no timing thresholds"
    );
}
