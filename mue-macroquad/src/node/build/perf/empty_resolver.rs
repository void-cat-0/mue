//! Same-binary resolver notification comparison.
use super::*;
const NODES: usize = 16;
const SAMPLES: usize = 512;
const WARMUP: usize = 32;
fn themed(width: f32, relevant: bool) -> Theme {
    let selector = if relevant {
        Selector::for_target::<BaselineLeaf>(Specificity(0, 0, 1), |_| true)
    } else {
        Selector::for_target::<Unmatched>(Specificity(0, 0, 1), |_| true)
    };
    Theme::new(vec![Rule::new(vec![selector], move || {
        Style::new().width(width)
    })])
}
#[test]
#[ignore = "Run alone in release with --nocapture --test-threads=1"]
fn empty_resolver_compare() {
    for relevant in [false, true] {
        for round in 0..3 {
            let baseline = resource_counts();
            let owner = OwnedScope::new();
            let sources = owner.run(|| {
                [
                    signal(themed(120., relevant)),
                    signal(themed(120., relevant)),
                ]
            });
            let counters = [Rc::new(Counters::default()), Rc::new(Counters::default())];
            let nodes: [Vec<Node>; 2] = std::array::from_fn(|index| {
                (0..NODES)
                    .map(|_| {
                        AppBuildContext::with_theme(sources[index].into(), || {
                            baseline_leaf(counters[index].clone()).into_node()
                        })
                    })
                    .collect()
            });
            let live = resource_counts();
            let themes = [themed(120., relevant), themed(121., relevant)];
            let mut times = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
            let mut projections = [0; 2];
            let mut resolves = [0; 2];
            let mut allocations = [Allocations::default(); 2];
            for sample in 0..WARMUP + SAMPLES {
                if sample == WARMUP {
                    for counter in &counters {
                        counter.clear();
                    }
                }
                for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
                    let stats = crate::runtime::theme_stats();
                    let allocated = Allocations::snapshot();
                    let start = Instant::now();
                    let update = || sources[index].set(themes[sample % 2].clone());
                    if index == 0 {
                        super::super::resolver_reference::run(update);
                    } else {
                        update();
                    }
                    let elapsed = start.elapsed().as_nanos();
                    let after = crate::runtime::theme_stats();
                    if sample >= WARMUP {
                        times[index].push(elapsed);
                        projections[index] += after.projection_runs - stats.projection_runs;
                        resolves[index] += after.resolver_runs - stats.resolver_runs;
                        allocations[index].add(Allocations::snapshot().since(allocated));
                    }
                    let expected = if relevant && sample % 2 == 1 {
                        121.
                    } else {
                        120.
                    };
                    for node in &nodes[index] {
                        Runtime::with(|rt| {
                            let id = rt.node(node.id).layout_id.unwrap();
                            assert_eq!(
                                rt.taffy.borrow().style(id).unwrap().size.width,
                                taffy::Dimension::length(expected)
                            );
                        });
                    }
                }
            }
            assert_eq!(resolves, [NODES * SAMPLES; 2]);
            if !relevant {
                assert_eq!(projections[1], 0);
                assert_eq!(projections[0], 47 * NODES * SAMPLES);
            } else {
                assert_eq!(projections[0], projections[1]);
            }
            assert_eq!(resource_counts(), live);
            println!(
                "empty resolver relevant={relevant} round={} nodes={NODES} updates={SAMPLES}",
                round + 1
            );
            for (index, samples) in times.into_iter().enumerate() {
                let t = summarize(samples);
                println!(
                    "  reference={} p50={}ns p95={}ns resolves={} projections={}",
                    index == 0,
                    t.p50,
                    t.p95,
                    resolves[index],
                    projections[index]
                );
                allocations[index].print("updates");
                assert_eq!(counters[index].prepares.get(), 0);
                assert_eq!(counters[index].mounts.get(), 0);
            }
            drop(nodes);
            drop(owner);
            assert_eq!(resource_counts(), baseline);
        }
    }
}
