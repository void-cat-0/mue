//! Same-binary, alternating-order comparison of transition bindings.
use super::*;
use crate::transition::{self, Transition};
use mue_core::{prop::Prop, signal::Signal};
use std::hint::black_box;

const WARMUP: usize = 64;
const SAMPLES: usize = 2048;

fn arm<R>(legacy: bool, f: impl FnOnce() -> R) -> R {
    if legacy {
        transition::reference::run(f)
    } else {
        f()
    }
}

fn output(node: &Node) -> (taffy::Style, taffy::Size<f32>) {
    Runtime::with(|rt| {
        let id = rt.node(node.id).layout_id.unwrap();
        let tree = rt.taffy.borrow();
        (
            tree.style(id).unwrap().clone(),
            tree.layout(id).unwrap().size,
        )
    })
}

fn mount_arm(legacy: bool, theme: Signal<Theme>, counters: Rc<Counters>, instant: bool) -> Node {
    arm(legacy, || {
        AppBuildContext::with_theme(theme.into(), || {
            let builder = baseline_leaf(counters);
            if instant {
                builder
                    .transition(Prop::Static(Transition::Instant))
                    .into_node()
            } else {
                builder.into_node()
            }
        })
    })
}

fn updates(round: usize, instant: bool) {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let themes = owner.run(|| [signal(Theme::default()), signal(Theme::default())]);
    let before = resource_counts();
    let counters = [Rc::new(Counters::default()), Rc::new(Counters::default())];
    let nodes = [
        mount_arm(true, themes[0], counters[0].clone(), instant),
        mount_arm(false, themes[1], counters[1].clone(), instant),
    ];
    let live = resource_counts();
    for node in &nodes {
        measure_layout(node);
    }
    let rule = |wide| {
        Theme::new(vec![Rule::new(
            vec![Selector::for_target::<BaselineLeaf>(
                Specificity(0, 0, 1),
                |_| true,
            )],
            move || {
                Style::new()
                    .width(if wide { 121. } else { 120. })
                    .height(if wide { 25. } else { 24. })
                    .padding((if wide { 3. } else { 2. },))
            },
        )])
    };
    let replacements = [rule(false), rule(true)];
    let mut times = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
    let mut allocations = [Allocations::default(); 2];
    let mut writes = [0; 2];
    let mut computes = [0; 2];
    for sample in 0..WARMUP + SAMPLES {
        if sample == WARMUP {
            for counter in &counters {
                counter.clear();
            }
        }
        for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
            let allocation = Allocations::snapshot();
            let prior_writes = layout::style_writes();
            let prior_computes = layout::style_computes();
            let start = Instant::now();
            arm(index == 0, || {
                themes[index].set(replacements[sample % 2].clone())
            });
            measure_layout(&nodes[index]);
            black_box(output(&nodes[index]));
            if sample >= WARMUP {
                times[index].push(start.elapsed().as_nanos());
                allocations[index].add(Allocations::snapshot().since(allocation));
                writes[index] += layout::style_writes() - prior_writes;
                computes[index] += layout::style_computes() - prior_computes;
            }
        }
        assert_eq!(output(&nodes[0]), output(&nodes[1]));
        assert_eq!(
            output(&nodes[1]).1.width,
            if sample % 2 == 1 { 121. } else { 120. }
        );
    }
    assert_eq!(resource_counts(), live);
    assert_eq!(writes[1], SAMPLES);
    if instant {
        assert_eq!(writes[0], SAMPLES);
    } else {
        // HashSet subscriber order changes how many intermediate layouts the
        // old watch-based graph publishes. Only the new graph promises one.
        assert!(
            (SAMPLES..=SAMPLES * 6).contains(&writes[0]),
            "legacy writes={} samples={SAMPLES}",
            writes[0]
        );
    }
    assert_eq!(computes, writes);
    for counter in &counters {
        assert_eq!(counter.prepares.get(), 0);
        assert_eq!(counter.mounts.get(), 0);
        assert_eq!(counter.width_consumers.get(), SAMPLES);
        assert_eq!(counter.height_consumers.get(), SAMPLES);
    }
    println!(
        "transition comparison round={} instant={instant}: {SAMPLES} updates + layout per arm",
        round + 1
    );
    for (index, samples) in times.into_iter().enumerate() {
        let timing = summarize(samples);
        println!(
            "  {} p50={}ns p95={}ns computes={} writes={}",
            if index == 0 { "legacy" } else { "computed" },
            timing.p50,
            timing.p95,
            computes[index],
            writes[index]
        );
        allocations[index].print("updates");
    }
    drop(nodes);
    assert_eq!(resource_counts(), before);
    drop(owner);
    assert_eq!(resource_counts(), baseline);
}

fn builds(round: usize) {
    let baseline = resource_counts();
    let owner = OwnedScope::new();
    let theme = owner.run(|| signal(Theme::default()));
    let before = resource_counts();
    let counters = Rc::new(Counters::default());
    let mut times = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
    let mut allocations = [Allocations::default(); 2];
    let mut lives = [before; 2];
    for sample in 0..WARMUP + SAMPLES {
        for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
            let allocation = Allocations::snapshot();
            let start = Instant::now();
            let node = mount_arm(index == 0, theme, counters.clone(), false);
            measure_layout(&node);
            black_box(output(&node));
            lives[index] = resource_counts();
            drop(node);
            if sample >= WARMUP {
                times[index].push(start.elapsed().as_nanos());
                allocations[index].add(Allocations::snapshot().since(allocation));
            }
        }
    }
    assert_eq!(resource_counts(), before);
    println!(
        "transition comparison round={}: {SAMPLES} empty-theme build/layout/drop samples per arm",
        round + 1
    );
    for (index, samples) in times.into_iter().enumerate() {
        let timing = summarize(samples);
        println!(
            "  {} p50={}ns p95={}ns",
            if index == 0 { "legacy" } else { "computed" },
            timing.p50,
            timing.p95
        );
        print_counts(before, lives[index], resource_counts());
        allocations[index].print("builds");
    }
    drop(owner);
    assert_eq!(resource_counts(), baseline);
}

#[test]
#[ignore = "Run alone in release mode with --nocapture --test-threads=1"]
fn transition_publication_compare() {
    for round in 0..3 {
        builds(round);
        updates(round, false);
        updates(round, true);
    }
}
