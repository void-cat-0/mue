//! Same-binary comparison of eager and deferred frame allocation.
use super::*;
use crate::transition;
const WARMUP: usize = 32;
const SAMPLES: usize = 1000;
fn build(eager: bool, counters: Rc<Counters>) -> Node {
    let f = || {
        AppBuildContext::with_theme(Theme::default().into(), || {
            baseline_leaf(counters).into_node()
        })
    };
    if eager {
        transition::eager_reference::run(f)
    } else {
        f()
    }
}
#[test]
#[ignore = "Run alone in release mode with --nocapture --test-threads=1"]
fn deferred_tween_compare() {
    for round in 0..3 {
        let before = resource_counts();
        let counters = Rc::new(Counters::default());
        let mut times = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
        let mut allocations = [Allocations::default(); 2];
        let mut live = [before; 2];
        for sample in 0..WARMUP + SAMPLES {
            for index in [(sample + round) % 2, 1 - (sample + round) % 2] {
                let allocation = Allocations::snapshot();
                let start = Instant::now();
                let node = build(index == 0, counters.clone());
                measure_layout(&node);
                live[index] = resource_counts();
                drop(node);
                if sample >= WARMUP {
                    times[index].push(start.elapsed().as_nanos());
                    allocations[index].add(Allocations::snapshot().since(allocation));
                }
            }
        }
        assert_eq!(resource_counts(), before);
        assert_eq!(live[0].effects, live[1].effects);
        assert!(live[1].signals < live[0].signals);
        println!("deferred_tween round={} build/layout/drop", round + 1);
        for (index, samples) in times.into_iter().enumerate() {
            let t = summarize(samples);
            println!("  eager={} p50={}ns p95={}ns", index == 0, t.p50, t.p95);
            print_counts(before, live[index], resource_counts());
            allocations[index].print("builds");
        }
        // Idle frame advancement scans exactly the drivers belonging to this arm.
        for index in [round % 2, 1 - round % 2] {
            let node = build(index == 0, counters.clone());
            let mut samples = Vec::with_capacity(SAMPLES);
            for sample in 0..WARMUP + SAMPLES {
                let start = Instant::now();
                transition::advance(1. / 60.);
                if sample >= WARMUP {
                    samples.push(start.elapsed().as_nanos());
                }
            }
            let t = summarize(samples);
            println!(
                "  idle eager={} p50={}ns p95={}ns",
                index == 0,
                t.p50,
                t.p95
            );
            drop(node);
            assert_eq!(resource_counts(), before);
        }
    }
}
