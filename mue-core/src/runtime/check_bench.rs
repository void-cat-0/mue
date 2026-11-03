//! Previous snapshot traversal and isolated, opt-in allocation benchmark.
use super::*;
use crate::prelude::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

thread_local! { static SNAPSHOT: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn use_snapshot() -> bool {
    SNAPSHOT.get()
}
fn policy<R>(snapshot: bool, f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SNAPSHOT.set(self.0);
        }
    }
    let _restore = Restore(SNAPSHOT.replace(snapshot));
    f()
}
impl Runtime {
    pub(super) fn update_effect_snapshot(&self, effect_id: EffectId) {
        if effect_id.is_null() || !self.effects.borrow().contains_key(effect_id) {
            return;
        }
        let mut effect = self.effect_mut(effect_id);
        if effect.state == EffectState::Check {
            let dependencies = effect.dependencies.to_vec();
            drop(effect);
            for dependency in dependencies {
                let parent = self
                    .signals
                    .borrow()
                    .get(dependency)
                    .and_then(|signal| signal.effect);
                if let Some(parent) = parent {
                    self.update_effect_if_necessary(parent);
                }
                if self
                    .effects
                    .borrow()
                    .get(effect_id)
                    .is_some_and(|effect| effect.state == EffectState::Dirty)
                {
                    break;
                }
            }
            if !self.effects.borrow().contains_key(effect_id) {
                return;
            }
            effect = self.effect_mut(effect_id);
        }
        if effect.state == EffectState::Dirty {
            drop(effect);
            self.update(effect_id);
        } else {
            effect.state = EffectState::Clean;
        }
    }
}
struct Allocator;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        p
    }
}
#[test]
#[ignore = "Run alone in release mode with --nocapture --test-threads=1"]
fn check_traversal_compare() {
    const SAMPLES: usize = 2048;
    for width in [1, 2, 8, 47] {
        for changed in [false, true] {
            for round in 0..3 {
                let baseline = resource_counts();
                let owners = [OwnedScope::new(), OwnedScope::new()];
                let runs = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
                let graphs: [(Signal<usize>, ReadSignal<usize>); 2] = std::array::from_fn(|arm| {
                    owners[arm].run(|| {
                        let source = signal(0usize);
                        let parent = computed(move |_| {
                            if changed {
                                source.get()
                            } else {
                                source.get() % 2
                            }
                        });
                        let slots: Vec<_> = (0..width)
                            .map(|i| computed(move |_| parent.get() + i))
                            .collect();
                        let counter = runs[arm].clone();
                        let output = computed(move |_| {
                            counter.set(counter.get() + 1);
                            slots.iter().map(Access::get).sum::<usize>()
                        });
                        (source, output)
                    })
                });
                let live = resource_counts();
                let mut times = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
                let mut allocations = [0; 2];
                let mut bytes = [0; 2];
                for sample in 0..64 + SAMPLES {
                    for arm in [(sample + round) % 2, 1 - (sample + round) % 2] {
                        let a = ALLOCS.load(Ordering::Relaxed);
                        let b = BYTES.load(Ordering::Relaxed);
                        let start = Instant::now();
                        policy(arm == 0, || graphs[arm].0.set((sample + 1) * 2));
                        let elapsed = start.elapsed().as_nanos();
                        if sample >= 64 {
                            times[arm].push(elapsed);
                            allocations[arm] += ALLOCS.load(Ordering::Relaxed) - a;
                            bytes[arm] += BYTES.load(Ordering::Relaxed) - b;
                        }
                        let expected = width * (width - 1) / 2
                            + if changed { width * (sample + 1) * 2 } else { 0 };
                        assert_eq!(graphs[arm].1.get(), expected);
                    }
                }
                assert_eq!(runs[0].get(), runs[1].get());
                assert_eq!(runs[0].get(), if changed { 64 + SAMPLES + 1 } else { 1 });
                assert_eq!(resource_counts(), live);
                println!("Check width={width} changed={changed} round={}", round + 1);
                for (arm, mut samples) in times.into_iter().enumerate() {
                    samples.sort_unstable();
                    println!(
                        "  snapshot={} p50={}ns p95={}ns allocs={} bytes={}",
                        arm == 0,
                        samples[(SAMPLES - 1) / 2],
                        samples[(SAMPLES - 1) * 95 / 100],
                        allocations[arm],
                        bytes[arm]
                    );
                }
                drop(owners);
                assert_eq!(resource_counts(), baseline);
            }
        }
    }
}
