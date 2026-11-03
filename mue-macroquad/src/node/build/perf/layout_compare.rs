//! Same-binary comparison; run this ignored test alone with --test-threads=1.
use super::*;
use crate::{style::Size, transition::Transition};
use mue_core::{prop::Prop, signal::Signal};
use std::hint::black_box;

const WARMUP: usize = 64;
const SAMPLES: usize = 2000;
const TOGGLES: usize = 2048;
const ROUNDS: usize = 3;

#[mue_macros::node]
#[theme]
fn compare_leaf(reference: bool, counters: Rc<Counters>) -> NodePlan<impl FnOnce(&mut Style)> {
    let reference = reference.get_clone();
    let counters = counters.get_clone();
    counters.prepares.set(counters.prepares.get() + 1);
    NodePlan::new(Style::new().width(120.).height(24.), move |style| {
        counters.mounts.set(counters.mounts.get() + 1);
        let width = style.declared_width().unwrap();
        let height = style.declared_height().unwrap();
        let counter = counters.clone();
        watch_effect(move || {
            counter
                .width_consumers
                .set(counter.width_consumers.get() + 1);
            black_box(width.get_clone());
        });
        let counter = counters.clone();
        watch_effect(move || {
            counter
                .height_consumers
                .set(counter.height_consumers.get() + 1);
            black_box(height.get_clone());
        });
        let layout = if reference {
            layout::use_layout_reference(style)
        } else {
            layout::use_layout(style)
        };
        let paint = paint::use_paint(style);
        let _shapes =
            paint.build(move |_| counters.paint_builds.set(counters.paint_builds.get() + 1));
        black_box(layout);
    })
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Static,
    EmptyTheme,
    InstantTheme,
    Raw,
}
impl Case {
    fn mount(
        self,
        reference: bool,
        counters: Rc<Counters>,
        value: Signal<Size>,
        theme: Signal<Theme>,
    ) -> Node {
        let build = || {
            let builder = compare_leaf(reference, counters);
            match self {
                Self::Raw => builder
                    .width(value)
                    .height(value)
                    .column_gap(value)
                    .row_gap(value)
                    .margin((value,))
                    .padding((value,))
                    .border((value,))
                    .inset((value,))
                    .transition(Prop::Static(Transition::Instant))
                    .into_node(),
                Self::InstantTheme => builder
                    .transition(Prop::Static(Transition::Instant))
                    .into_node(),
                _ => builder.into_node(),
            }
        };
        if matches!(self, Self::EmptyTheme | Self::InstantTheme) {
            AppBuildContext::with_theme(theme.into(), build)
        } else {
            build()
        }
    }
}

fn read_layout(node: &Node) -> (taffy::Style, taffy::Size<f32>) {
    Runtime::with(|rt| {
        let id = rt.node(node.id).layout_id.expect("comparison layout");
        let tree = rt.taffy.borrow();
        (
            tree.style(id).unwrap().clone(),
            tree.layout(id).unwrap().size,
        )
    })
}

struct Arm {
    times: Vec<u128>,
    allocations: Allocations,
    writes: usize,
    computes: usize,
    live: ResourceCounts,
    counters: Rc<Counters>,
}
impl Arm {
    fn new(capacity: usize, baseline: ResourceCounts) -> Self {
        Self {
            times: Vec::with_capacity(capacity),
            allocations: Allocations::default(),
            writes: 0,
            computes: 0,
            live: baseline,
            counters: Rc::new(Counters::default()),
        }
    }
    fn report(self, label: &str, baseline: ResourceCounts, after: ResourceCounts) {
        let timing = summarize(self.times);
        println!(
            "    {label}: p50={}ns p95={}ns prepares={} mounts={} width_notify={} height_notify={} paint_build={} taffy_style_writes={} taffy_computes={}",
            timing.p50,
            timing.p95,
            self.counters.prepares.get(),
            self.counters.mounts.get(),
            self.counters.width_consumers.get(),
            self.counters.height_consumers.get(),
            self.counters.paint_builds.get(),
            self.writes,
            self.computes
        );
        print_counts(baseline, self.live, after);
        self.allocations.print("measured operations");
    }
}

fn build_round(case: Case, round: usize) {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let value = owner.run(|| signal(Size::Px(24.)));
    let theme = owner.run(|| signal(Theme::default()));
    let before = resource_counts();
    let mut arms = [Arm::new(SAMPLES, before), Arm::new(SAMPLES, before)];
    for sample in 0..WARMUP {
        for index in [sample % 2, 1 - sample % 2] {
            let node = case.mount(index == 0, arms[index].counters.clone(), value, theme);
            measure_layout(&node);
            black_box(read_layout(&node));
            drop(node);
        }
    }
    assert_eq!(resource_counts(), before);
    for arm in &arms {
        arm.counters.clear();
    }
    for sample in 0..SAMPLES {
        let mut outputs = [None, None];
        for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
            let arm = &mut arms[index];
            let allocation = Allocations::snapshot();
            let writes = crate::layout::style_writes();
            let computes = crate::layout::style_computes();
            let start = Instant::now();
            let node = case.mount(index == 0, arm.counters.clone(), value, theme);
            measure_layout(&node);
            let output = black_box(read_layout(&node));
            arm.live = max_counts(arm.live, resource_counts());
            drop(node);
            arm.times.push(start.elapsed().as_nanos());
            arm.allocations
                .add(Allocations::snapshot().since(allocation));
            arm.writes += crate::layout::style_writes() - writes;
            arm.computes += crate::layout::style_computes() - computes;
            outputs[index] = Some(output);
        }
        assert_eq!(outputs[0], outputs[1], "{case:?} build output mismatch");
    }
    let after = resource_counts();
    assert_eq!(after, before, "{case:?} build leaked resources");
    assert_eq!(
        arms[0].writes, arms[1].writes,
        "build layout writes changed"
    );
    assert_eq!(
        arms[0].computes, arms[1].computes,
        "build layout recomputations changed"
    );
    println!(
        "layout_aggregation_compare round={} {case:?}: {SAMPLES} build/layout/drop samples per arm",
        round + 1
    );
    let [reference, folded] = arms;
    reference.report("legacy", before, after);
    folded.report("folded", before, after);
    drop(owner);
    crate::transition::advance(1.);
    assert_eq!(resource_counts(), baseline);
}

fn update_round(case: Case, round: usize) {
    let raw = matches!(case, Case::Raw);
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let values = owner.run(|| [signal(Size::Px(24.)), signal(Size::Px(24.))]);
    let themes = owner.run(|| [signal(Theme::default()), signal(Theme::default())]);
    let before = resource_counts();
    let mut arms = [Arm::new(TOGGLES, before), Arm::new(TOGGLES, before)];
    let mut nodes = Vec::with_capacity(2);
    for index in 0..2 {
        let prior = resource_counts();
        nodes.push(case.mount(
            index == 0,
            arms[index].counters.clone(),
            values[index],
            themes[index],
        ));
        let live = resource_counts();
        arms[index].live = ResourceCounts {
            scopes: before.scopes + live.scopes - prior.scopes,
            signals: before.signals + live.signals - prior.signals,
            effects: before.effects + live.effects - prior.effects,
        };
        measure_layout(&nodes[index]);
    }
    let theme_for = |wide: bool| {
        Theme::new(vec![Rule::new(
            vec![Selector::new(Specificity(0, 0, 1), |target| {
                target.is::<CompareLeaf>()
            })],
            move || {
                Style::new()
                    .width(if wide { 121. } else { 120. })
                    .height(if wide { 25. } else { 24. })
                    .padding((if wide { 3. } else { 2. },))
            },
        )])
    };
    let matched_themes = [theme_for(false), theme_for(true)];
    for sample in 0..WARMUP + TOGGLES {
        if sample == WARMUP {
            for arm in &arms {
                arm.counters.clear();
            }
        }
        for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
            let allocation = Allocations::snapshot();
            let writes = crate::layout::style_writes();
            let computes = crate::layout::style_computes();
            let start = Instant::now();
            if raw {
                mue_core::batch(|| {
                    values[index].set(Size::Px(if sample % 2 == 0 { 25. } else { 24. }))
                });
            } else {
                themes[index].set(matched_themes[sample % 2].clone());
            }
            measure_layout(&nodes[index]);
            black_box(read_layout(&nodes[index]));
            if sample >= WARMUP {
                arms[index].times.push(start.elapsed().as_nanos());
                arms[index]
                    .allocations
                    .add(Allocations::snapshot().since(allocation));
                arms[index].writes += crate::layout::style_writes() - writes;
                arms[index].computes += crate::layout::style_computes() - computes;
            }
        }
        let reference = read_layout(&nodes[0]);
        let folded = read_layout(&nodes[1]);
        assert_eq!(reference, folded, "raw={raw} update {sample} mismatch");
        if !raw {
            assert_eq!(
                reference.0.size.width,
                taffy::Dimension::length(if sample % 2 == 1 { 121. } else { 120. })
            );
            assert_eq!(reference.1.width, if sample % 2 == 1 { 121. } else { 120. });
        }
    }
    assert_eq!(
        arms[0].counters.width_consumers.get(),
        arms[1].counters.width_consumers.get()
    );
    assert_eq!(
        arms[0].counters.height_consumers.get(),
        arms[1].counters.height_consumers.get()
    );
    assert_eq!(
        arms[0].writes, arms[1].writes,
        "update layout writes changed"
    );
    assert_eq!(
        arms[0].computes, arms[1].computes,
        "update layout recomputations changed"
    );
    drop(nodes);
    crate::transition::advance(1.);
    let after = resource_counts();
    assert_eq!(after, before, "mounted updates leaked resources");
    let label = format!("{case:?}");
    println!(
        "layout_aggregation_compare round={} {label} updates: {TOGGLES} per arm",
        round + 1
    );
    let [reference, folded] = arms;
    reference.report("legacy", before, after);
    folded.report("folded", before, after);
    drop(owner);
    assert_eq!(resource_counts(), baseline);
}

#[test]
#[ignore = "Run alone: cargo test -p mue-macroquad --release layout_aggregation_compare -- --ignored --nocapture --test-threads=1"]
fn layout_aggregation_compare() {
    println!(
        "layout aggregation: {ROUNDS} rounds, {WARMUP} warmups, {SAMPLES} builds and {TOGGLES} updates per arm; alternating order; no timing thresholds"
    );
    for round in 0..ROUNDS {
        for case in [
            Case::Static,
            Case::EmptyTheme,
            Case::InstantTheme,
            Case::Raw,
        ] {
            build_round(case, round);
        }
        update_round(Case::EmptyTheme, round);
        update_round(Case::InstantTheme, round);
        update_round(Case::Raw, round);
    }
}
