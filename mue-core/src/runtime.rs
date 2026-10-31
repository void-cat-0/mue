use std::{
    cell::{Cell, RefCell, RefMut},
    mem,
    thread::AccessError,
};

use slotmap::{Key, SlotMap};

use crate::{
    effect::{Dependencies, DependencyList, EffectCallback, EffectId, EffectInner, EffectState},
    scope::{ScopeId, ScopeInner},
    signal::{SignalId, SignalInner, Value},
};

thread_local! {
    static RUNTIME: Runtime = Runtime::new();
}

struct DependencyTracker {
    dependencies: DependencyList,
    index: usize,
    new_dependencies: DependencyList,
}

impl DependencyTracker {
    pub fn new(dependencies: DependencyList) -> Self {
        Self {
            dependencies,
            index: 0,
            new_dependencies: DependencyList::new(),
        }
    }

    pub fn push(&mut self, signal_id: SignalId) {
        if self.new_dependencies.is_empty() && self.dependencies.get(self.index) == Some(&signal_id)
        {
            self.index += 1;
        } else {
            self.new_dependencies.push(signal_id);
        }
    }
}

/// Restores the runtime stack and temporarily removed effect data on unwind.
struct EffectRun<'a> {
    runtime: &'a Runtime,
    effect_id: EffectId,
    signal_id: SignalId,
    previous_effect: Option<EffectId>,
    previous_tracker: Option<DependencyTracker>,
    callback: Option<EffectCallback>,
    value: Option<Value>,
    took_callback: bool,
    completed: bool,
}

impl Drop for EffectRun<'_> {
    fn drop(&mut self) {
        let rt = self.runtime;
        rt.current_effect.replace(self.previous_effect);
        let tracker = rt.tracker.replace(self.previous_tracker.take());

        // The callback may have disposed itself or its owning scope. Never
        // resurrect resources removed while their callback was executing.
        if self.took_callback
            && let Some(signal) = rt.signals.borrow_mut().get_mut(self.signal_id)
        {
            signal.value = self.value.take();
        }

        let mut effects = rt.effects.borrow_mut();
        let effect = effects.get_mut(self.effect_id);
        let mut signals = rt.signals.borrow_mut();
        if let Some(effect) = effect {
            if self.took_callback {
                effect.callback = self.callback.take();
            }
            if self.completed {
                effect.state = EffectState::Clean;
            }
            if let Some(mut tracker) = tracker {
                if self.completed {
                    for signal_id in tracker.dependencies.drain(tracker.index..) {
                        if let Some(signal) = signals.get_mut(signal_id) {
                            signal.subscribers.remove(&self.effect_id);
                        }
                    }
                }
                tracker.dependencies.retain(|id| signals.contains_key(*id));
                // On unwind retain old and newly observed dependencies so that
                // disposing the failed build can remove every subscription.
                for signal_id in tracker.new_dependencies {
                    if let Some(signal) = signals.get_mut(signal_id) {
                        signal.subscribers.insert(self.effect_id);
                        if !tracker.dependencies.contains(&signal_id) {
                            tracker.dependencies.push(signal_id);
                        }
                    }
                }
                effect.dependencies = Dependencies::Dynamic(tracker.dependencies);
            }
        } else if let Some(tracker) = tracker {
            for signal_id in tracker.dependencies.iter().chain(&tracker.new_dependencies) {
                if let Some(signal) = signals.get_mut(*signal_id) {
                    signal.subscribers.remove(&self.effect_id);
                }
            }
        }
    }
}

/// The reactive runtime that manages all signals, memos, and effects.
pub(crate) struct Runtime {
    pub signals: RefCell<SlotMap<SignalId, SignalInner>>,
    pub effects: RefCell<SlotMap<EffectId, EffectInner>>,
    pub scopes: RefCell<SlotMap<ScopeId, ScopeInner>>,

    pub null_signal: SignalId,

    pub current_effect: RefCell<Option<EffectId>>,
    tracker: RefCell<Option<DependencyTracker>>,

    batch_depth: Cell<usize>,
    pending_updates: RefCell<Vec<EffectId>>,
}

impl Runtime {
    fn new() -> Self {
        let signals = RefCell::new(SlotMap::with_key());
        let null_signal = signals.borrow_mut().insert(SignalInner::new(
            None,
            None,
            #[cfg(debug_assertions)]
            std::panic::Location::caller(),
        ));

        Self {
            signals,
            effects: RefCell::new(SlotMap::with_key()),
            scopes: RefCell::new(SlotMap::with_key()),

            null_signal,

            current_effect: RefCell::default(),
            tracker: RefCell::default(),

            batch_depth: Cell::new(0),
            pending_updates: RefCell::default(),
        }
    }

    pub fn with<R>(f: impl FnOnce(&Runtime) -> R) -> R {
        RUNTIME.with(f)
    }
    pub fn try_with<R>(f: impl FnOnce(&Runtime) -> R) -> Result<R, AccessError> {
        RUNTIME.try_with(f)
    }

    pub fn signal_mut(&self, signal_id: SignalId) -> RefMut<'_, SignalInner> {
        RefMut::map(self.signals.borrow_mut(), |arena| &mut arena[signal_id])
    }

    pub fn effect_mut(&self, effect_id: EffectId) -> RefMut<'_, EffectInner> {
        RefMut::map(self.effects.borrow_mut(), |arena| &mut arena[effect_id])
    }

    pub fn track(&self, signal_id: SignalId) {
        if let Some(tracker) = self.tracker.borrow_mut().as_mut() {
            tracker.push(signal_id);
        }
    }

    pub fn on_update(&self, signal_id: SignalId) {
        let mut subscribers = mem::take(&mut self.signal_mut(signal_id).subscribers);
        for effect_id in &subscribers {
            self.mark_stale(*effect_id, EffectState::Dirty);
        }
        let mut signal = self.signal_mut(signal_id);
        if signal.subscribers.len() < subscribers.len() {
            mem::swap(&mut signal.subscribers, &mut subscribers);
        }
        signal.subscribers.extend(subscribers);
        drop(signal);

        if self.batch_depth.get() == 0 {
            self.flush_updates();
        }
    }

    pub fn update(&self, effect_id: EffectId) {
        self.update_inner(effect_id, Option::<fn()>::None);
    }
    pub fn update_with<R>(&self, effect_id: EffectId, track_fn: impl FnOnce() -> R) -> R {
        self.update_inner(effect_id, Some(track_fn)).unwrap()
    }

    fn update_inner<R>(
        &self,
        effect_id: EffectId,
        track_fn: Option<impl FnOnce() -> R>,
    ) -> Option<R> {
        if effect_id.is_null() {
            return None;
        }

        let mut effect = self.effect_mut(effect_id);
        let cleanups = mem::take(&mut effect.cleanups);
        drop(effect);
        for cleanup in cleanups {
            cleanup();
        }
        if !self.effects.borrow().contains_key(effect_id) {
            return None;
        }
        let mut effect = self.effect_mut(effect_id);
        let update_context = mem::take(&mut effect.update_context);

        let tracker = match &mut effect.dependencies {
            Dependencies::Static(_) => None,
            Dependencies::Dynamic(deps) => Some(DependencyTracker::new(mem::take(deps))),
        };
        let prev_tracker = self.tracker.replace(tracker);

        let prev_effect = self.current_effect.replace(Some(effect_id));

        let signal_id = effect.signal;

        let took_callback = track_fn.is_none();
        let callback = if took_callback {
            effect.callback.take()
        } else {
            None
        };
        drop(effect);
        let mut run = EffectRun {
            runtime: self,
            effect_id,
            signal_id,
            previous_effect: prev_effect,
            previous_tracker: prev_tracker,
            callback,
            value: None,
            took_callback,
            completed: false,
        };
        let mut result = None;
        let updated = if let Some(track_fn) = track_fn {
            result = Some(track_fn());
            false
        } else {
            run.value = self.signal_mut(signal_id).value.take();
            update_context.run(|| run.callback.as_mut().unwrap()(&mut run.value))
        };
        run.completed = true;
        drop(run);

        if updated {
            update_context.run(|| self.on_update(signal_id));
        }

        result
    }

    pub fn update_effect_if_necessary(&self, effect_id: EffectId) {
        if effect_id.is_null() || !self.effects.borrow().contains_key(effect_id) {
            return;
        }

        let mut effect = self.effect_mut(effect_id);
        if effect.state == EffectState::Check {
            let dependencies = effect.dependencies.to_vec();
            drop(effect);

            for dependency_signal_id in &*dependencies {
                // Updating a parent can remove keyed children and their signals.
                let effect = self
                    .signals
                    .borrow()
                    .get(*dependency_signal_id)
                    .and_then(|signal| signal.effect);
                if let Some(parent_effect_id) = effect {
                    self.update_effect_if_necessary(parent_effect_id);
                }

                if self
                    .effects
                    .borrow()
                    .get(effect_id)
                    .is_some_and(|it| it.state == EffectState::Dirty)
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

    pub fn update_signal_if_necessary(&self, signal_id: SignalId) {
        let effect_id = self.signal_mut(signal_id).effect;
        if let Some(effect_id) = effect_id {
            self.update_effect_if_necessary(effect_id);
        }
    }

    fn mark_stale(&self, effect_id: EffectId, state: EffectState) {
        let mut effect = self.effect_mut(effect_id);
        effect.update_context = crate::update_context::UpdateContext::current();
        if effect.state >= state {
            return;
        }

        let old_state = mem::replace(&mut effect.state, state);
        if old_state == EffectState::Clean {
            self.pending_updates.borrow_mut().push(effect_id);
        } else {
            return;
        }

        let signal = effect.signal;
        drop(effect);

        let subscribers = mem::take(&mut self.signal_mut(signal).subscribers);
        for subscriber in &subscribers {
            self.mark_stale(*subscriber, EffectState::Check);
        }
        self.signal_mut(signal).subscribers = subscribers;
    }

    fn flush_updates(&self) {
        loop {
            let pending = mem::take(&mut *self.pending_updates.borrow_mut());
            if pending.is_empty() {
                break;
            }
            for effect_id in pending {
                self.update_effect_if_necessary(effect_id);
            }
        }
    }

    pub fn batch(&self, f: impl FnOnce()) {
        struct RestoreDepth<'a> {
            depth: &'a Cell<usize>,
            previous: usize,
        }
        impl Drop for RestoreDepth<'_> {
            fn drop(&mut self) {
                self.depth.set(self.previous);
            }
        }

        let depth = self.batch_depth.get();
        self.batch_depth.set(depth + 1);
        let _restore = RestoreDepth {
            depth: &self.batch_depth,
            previous: depth,
        };
        f();

        if depth == 0 {
            self.flush_updates();
        }
    }

    /// Dispose a signal, removing it and cleaning up all dependencies.
    pub fn dispose_signal(&self, signal_id: SignalId) {
        // Don't dispose the null signal
        if signal_id == self.null_signal {
            return;
        }

        let Some(mut signal) = self.signals.borrow_mut().remove(signal_id) else {
            return;
        };

        // Remove this signal from all effects that depend on it
        let subscribers = mem::take(&mut signal.subscribers);
        for effect_id in subscribers {
            if let Some(effect) = self.effects.borrow_mut().get_mut(effect_id) {
                // Remove signal from effect's dependencies
                if let Dependencies::Dynamic(deps) = &mut effect.dependencies {
                    deps.retain(|&mut id| id != signal_id);
                }
            }
        }

        // If there's an effect associated with this signal (computed), dispose it too
        if let Some(effect_id) = signal.effect {
            self.dispose_effect(effect_id);
        }
    }

    /// Dispose an effect, removing it and cleaning up all dependencies.
    pub fn dispose_effect(&self, effect_id: EffectId) {
        let Some(mut effect) = self.effects.borrow_mut().remove(effect_id) else {
            return;
        };

        // Execute cleanup before disposing
        effect.cleanup();

        // Remove this effect from all signals it depends on
        for signal_id in &*effect.dependencies {
            if let Some(signal) = self.signals.borrow_mut().get_mut(*signal_id) {
                signal.subscribers.remove(&effect_id);
            }
        }

        self.dispose_signal(effect.signal);
    }
}

pub fn batch(f: impl FnOnce()) {
    Runtime::with(|rt| rt.batch(f))
}

/// Live arena entry counts for current-thread lifecycle instrumentation.
/// This reports live entries, not cumulative allocations or memory usage.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceCounts {
    pub scopes: usize,
    /// Includes the runtime's permanent null signal.
    pub signals: usize,
    pub effects: usize,
}

/// Snapshot current-thread reactive resources, including the null signal.
/// Compare snapshots on the same thread to measure creation and cleanup.
#[doc(hidden)]
pub fn resource_counts() -> ResourceCounts {
    Runtime::with(|rt| ResourceCounts {
        scopes: rt.scopes.borrow().len(),
        signals: rt.signals.borrow().len(),
        effects: rt.effects.borrow().len(),
    })
}

#[cfg(debug_assertions)]
pub fn debug() {
    crate::runtime::Runtime::with(|rt| {
        let signals = rt.signals.borrow();
        eprintln!("Signals: {}", signals.len());
        for signal in signals.values() {
            eprintln!("  {}", signal.location);
        }

        let effects = rt.effects.borrow();
        eprintln!("Effects: {}", effects.len());
        for effect in effects.values() {
            eprintln!("  {}", effect.location);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    fn assert_context_restored() {
        Runtime::with(|rt| {
            assert_eq!(rt.batch_depth.get(), 0);
            assert!(rt.current_effect.borrow().is_none());
            assert!(rt.tracker.borrow().is_none());
        });
    }

    #[test]
    fn resource_snapshot_counts_live_entries_and_returns_to_baseline() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(1);
            let derived = computed(move |_| source.get() * 2);
            watch_effect(move || {
                derived.get();
            });
        });
        let live = resource_counts();
        assert_eq!(live.scopes - baseline.scopes, 1);
        assert_eq!(live.signals - baseline.signals, 2);
        assert_eq!(live.effects - baseline.effects, 2);
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn initial_effect_and_computed_panics_release_owned_resources() {
        let external_owner = OwnedScope::new();
        let source = external_owner.run(|| signal(1));
        let baseline = resource_counts();
        for computed_build in [false, true] {
            let resource = Rc::new(());
            let cleanup_runs = Rc::new(Cell::new(0));
            let result = catch_unwind(AssertUnwindSafe(|| {
                let build = OwnedScope::new();
                build.run(|| {
                    batch(|| {
                        let resource = resource.clone();
                        let cleanup_runs = cleanup_runs.clone();
                        if computed_build {
                            computed::<i32>(move |_| {
                                let _resource = &resource;
                                source.get();
                                let cleanup_runs = cleanup_runs.clone();
                                on_cleanup(move || cleanup_runs.set(cleanup_runs.get() + 1));
                                panic!("initial computed build failed");
                            });
                        } else {
                            watch_effect(move || {
                                let _resource = &resource;
                                source.get();
                                let cleanup_runs = cleanup_runs.clone();
                                on_cleanup(move || cleanup_runs.set(cleanup_runs.get() + 1));
                                panic!("initial effect build failed");
                            });
                        }
                    })
                });
            }));
            assert!(result.is_err());
            assert_eq!(cleanup_runs.get(), 1);
            assert_eq!(Rc::strong_count(&resource), 1);
            assert_eq!(resource_counts(), baseline);
            assert!(!source.is_disposed());
            Runtime::with(|rt| assert!(rt.signal_mut(source.id).subscribers.is_empty()));
            assert!(current_scope().is_null());
            assert_context_restored();
            source.set(source.get() + 1);
        }
    }

    #[test]
    fn nested_batch_depth_restores_without_running_callbacks_during_unwind() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0);
            let observed = Rc::new(Cell::new(0));
            watch(*source, {
                let observed = observed.clone();
                move |value| observed.set(value.get())
            });
            let result = catch_unwind(AssertUnwindSafe(|| {
                batch(|| {
                    source.set(1);
                    let inner = catch_unwind(AssertUnwindSafe(|| {
                        batch(|| panic!("inner batch failed"));
                    }));
                    assert!(inner.is_err());
                    Runtime::with(|rt| assert_eq!(rt.batch_depth.get(), 1));
                    panic!("outer batch failed");
                });
            }));
            assert!(result.is_err());
            assert_eq!(observed.get(), 0);
            assert_context_restored();
            source.set(2);
            assert_eq!(observed.get(), 2);
        });
    }

    #[test]
    fn callback_value_and_context_restore_when_batch_flush_panics() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(1);
            let fail = Rc::new(Cell::new(false));
            let previous = Rc::new(Cell::new(None));
            let derived = computed({
                let fail = fail.clone();
                let previous = previous.clone();
                move |old: Option<&i32>| {
                    let value = source.get();
                    assert!(!fail.get(), "computed update failed");
                    previous.set(old.copied());
                    value * 10
                }
            });
            assert_eq!(derived.get(), 10);
            fail.set(true);
            let result = catch_unwind(AssertUnwindSafe(|| {
                batch(|| source.set(2));
            }));
            assert!(result.is_err());
            assert_context_restored();
            Runtime::with(|rt| {
                let signals = rt.signals.borrow();
                let inner = &signals[derived.id];
                assert_eq!(
                    inner.value.as_ref().unwrap().downcast_ref::<i32>(),
                    Some(&10)
                );
                assert!(
                    rt.effects.borrow()[inner.effect.unwrap()]
                        .callback
                        .is_some()
                );
            });
            fail.set(false);
            assert_eq!(derived.get(), 20);
            assert_eq!(previous.get(), Some(10));
        });
    }

    #[test]
    fn inner_failed_effect_restores_outer_effect_and_dependency_tracker() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0);
            let runs = Rc::new(Cell::new(0));
            let cleanups = Rc::new(Cell::new(0));
            watch_effect({
                let runs = runs.clone();
                let cleanups = cleanups.clone();
                move || {
                    let outer = Runtime::with(|rt| *rt.current_effect.borrow());
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            let failed = OwnedScope::new();
                            failed.run(|| {
                                computed::<i32>(move |_| {
                                    source.get();
                                    panic!("inner failed build");
                                });
                            });
                        }))
                        .is_err()
                    );
                    assert_eq!(Runtime::with(|rt| *rt.current_effect.borrow()), outer);
                    source.get();
                    let cleanups = cleanups.clone();
                    on_cleanup(move || cleanups.set(cleanups.get() + 1));
                    runs.set(runs.get() + 1);
                }
            });
            assert_eq!(runs.get(), 1);
            source.set(1);
            assert_eq!(runs.get(), 2);
            assert_eq!(cleanups.get(), 1);
            assert_context_restored();
            Runtime::with(|rt| assert_eq!(rt.signal_mut(source.id).subscribers.len(), 1));
        });
    }
}
