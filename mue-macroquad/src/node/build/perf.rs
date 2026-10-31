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
