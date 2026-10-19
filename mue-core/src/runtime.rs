use std::{
    cell::{Cell, RefCell, RefMut},
    collections::{HashMap, VecDeque},
    mem,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    thread::AccessError,
};

use slotmap::{Key, SlotMap};

use crate::{
    effect::{Dependencies, DependencyList, EffectId, EffectInner, EffectState},
    scope::{ScopeId, ScopeInner},
    signal::{SignalId, SignalInner},
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

/// The reactive runtime that manages all signals, memos, and effects.
pub(crate) struct Runtime {
    pub signals: RefCell<SlotMap<SignalId, SignalInner>>,
    pub effects: RefCell<SlotMap<EffectId, EffectInner>>,
    pub scopes: RefCell<SlotMap<ScopeId, ScopeInner>>,

    pub null_signal: SignalId,

    pub current_effect: RefCell<Option<EffectId>>,
    tracker: RefCell<Option<DependencyTracker>>,

    batch_depth: Cell<usize>,
    pending_updates: RefCell<VecDeque<EffectId>>,
    execution_depth: Cell<usize>,
    flushing: Cell<bool>,
    executions: RefCell<HashMap<EffectId, usize>>,
    checking: RefCell<Vec<EffectId>>,
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
            execution_depth: Cell::new(0),
            flushing: Cell::new(false),
            executions: RefCell::default(),
            checking: RefCell::default(),
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
        let effect_id = *self.current_effect.borrow();
        if let Some(tracker) = self.tracker.borrow_mut().as_mut() {
            tracker.push(signal_id);
        } else {
            return;
        }
        if let Some(effect_id) = effect_id {
            // A read consumes the refreshed value, not the invalidation that
            // produced it. Keep other writes during the read observable.
            let computed = self.signal_mut(signal_id).effect.is_some();
            if computed {
                self.signal_mut(signal_id).subscribers.remove(&effect_id);
            }
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                if computed {
                    self.update_signal_if_necessary(signal_id);
                }
            }));
            if self.effects.borrow().contains_key(effect_id)
                && let Some(signal) = self.signals.borrow_mut().get_mut(signal_id)
            {
                signal.subscribers.insert(effect_id);
            }
            if let Err(error) = outcome {
                resume_unwind(error);
            }
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

        self.maybe_flush();
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
        let owner = {
            let mut effects = self.effects.borrow_mut();
            let effect = effects.get_mut(effect_id)?;
            if effect.running {
                effect.rerun = true;
                return None;
            }
            effect.owner
        };
        if owner.is_disposed() {
            self.dispose_effect(effect_id);
            return None;
        }
        if self.execution_depth.get() == 0 && !self.flushing.get() {
            self.executions.borrow_mut().clear();
        }
        let count = {
            let mut executions = self.executions.borrow_mut();
            let count = executions.entry(effect_id).or_default();
            *count += 1;
            *count
        };
        if count > 100 {
            self.effect_mut(effect_id).state = EffectState::Dirty;
            if !self.pending_updates.borrow().contains(&effect_id) {
                self.pending_updates.borrow_mut().push_back(effect_id);
            }
            panic!(
                "reactive update limit exceeded for effect {effect_id:?}{}",
                {
                    #[cfg(debug_assertions)]
                    {
                        format!(" created at {}", self.effect_mut(effect_id).location)
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        String::new()
                    }
                }
            );
        }
        let cleanups = {
            let mut effect = self.effect_mut(effect_id);
            effect.running = true;
            mem::take(&mut effect.cleanups)
        };
        let depth = self.execution_depth.replace(self.execution_depth.get() + 1);
        let cleanup_result = catch_unwind(AssertUnwindSafe(|| {
            self.run_cleanups(owner, cleanups);
        }));
        if let Err(error) = cleanup_result {
            if let Some(effect) = self.effects.borrow_mut().get_mut(effect_id) {
                effect.running = false;
                effect.rerun = false;
                effect.state = EffectState::Clean;
            }
            self.execution_depth.set(depth);
            if self.effects.borrow().contains_key(effect_id) {
                self.mark_stale(effect_id, EffectState::Dirty);
            }
            resume_unwind(error);
        }
        if !self.effects.borrow().contains_key(effect_id) {
            self.execution_depth.set(depth);
            self.maybe_flush();
            return None;
        }

        let mut effect = self.effect_mut(effect_id);
        let owner = effect.owner;
        let update_context = mem::take(&mut effect.update_context);
        let tracker = match &mut effect.dependencies {
            Dependencies::Static(_) => None,
            Dependencies::Dynamic(deps) => Some(DependencyTracker::new(mem::take(deps))),
        };
        let signal_id = effect.signal;
        let mut callback = if track_fn.is_none() {
            effect.callback.take()
        } else {
            None
        };
        drop(effect);
        let mut value = if callback.is_some() {
            self.signal_mut(signal_id).value.take()
        } else {
            None
        };
        let prev_tracker = self.tracker.replace(tracker);
        let prev_effect = self.current_effect.replace(Some(effect_id));
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            owner.run(|| {
                update_context.run(|| {
                    if let Some(track_fn) = track_fn {
                        (Some(track_fn()), false)
                    } else {
                        (
                            None,
                            callback.as_mut().expect("effect callback unavailable")(&mut value),
                        )
                    }
                })
            })
        }));
        self.current_effect.replace(prev_effect);
        let tracker = self.tracker.replace(prev_tracker);
        let alive = self.effects.borrow().contains_key(effect_id);
        if alive {
            if callback.is_some() {
                self.signal_mut(signal_id).value = value.take();
            }
            let mut effect = self.effect_mut(effect_id);
            if callback.is_some() {
                effect.callback = callback.take();
            }
            effect.running = false;
            effect.state = EffectState::Clean;
            if let Some(mut tracker) = tracker {
                let mut signals = self.signals.borrow_mut();
                if outcome.is_ok() {
                    for id in tracker.dependencies.drain(tracker.index..) {
                        if let Some(signal) = signals.get_mut(id) {
                            signal.subscribers.remove(&effect_id);
                        }
                    }
                }
                tracker.dependencies.extend(tracker.new_dependencies);
                tracker.dependencies.retain(|id| signals.contains_key(*id));
                for id in &tracker.dependencies {
                    signals[*id].subscribers.insert(effect_id);
                }
                effect.dependencies = Dependencies::Dynamic(tracker.dependencies);
            }
        } else if let Some(tracker) = tracker {
            let mut signals = self.signals.borrow_mut();
            for id in tracker
                .dependencies
                .iter()
                .chain(tracker.new_dependencies.iter())
            {
                if let Some(signal) = signals.get_mut(*id) {
                    signal.subscribers.remove(&effect_id);
                }
            }
        }
        self.execution_depth.set(depth);
        let (result, updated) = match outcome {
            Ok(result) => result,
            Err(error) => {
                if alive {
                    self.effect_mut(effect_id).rerun = false;
                    update_context.run(|| self.mark_stale(effect_id, EffectState::Dirty));
                }
                resume_unwind(error);
            }
        };
        if alive {
            let rerun = mem::take(&mut self.effect_mut(effect_id).rerun);
            if rerun {
                self.mark_stale(effect_id, EffectState::Dirty);
            }
            if updated {
                update_context.run(|| self.on_update(signal_id));
            }
        }
        self.maybe_flush();
        result
    }

    fn run_cleanups(
        &self,
        owner: crate::scope::Scope,
        cleanups: Vec<crate::effect::CleanupCallback>,
    ) {
        let depth = self.execution_depth.replace(self.execution_depth.get() + 1);
        let prev_tracker = self.tracker.replace(None);
        let prev_effect = self.current_effect.replace(None);
        let mut first_error = None;
        for cleanup in cleanups {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                let scope = if owner.is_disposed() {
                    crate::scope::Scope::null()
                } else {
                    owner
                };
                scope.run(cleanup);
            }));
            if let Err(error) = outcome
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        self.current_effect.replace(prev_effect);
        self.tracker.replace(prev_tracker);
        self.execution_depth.set(depth);
        if let Some(error) = first_error {
            resume_unwind(error);
        }
    }

    fn maybe_flush(&self) {
        if self.batch_depth.get() == 0
            && self.execution_depth.get() == 0
            && self.checking.borrow().is_empty()
            && !self.flushing.get()
        {
            self.flush_updates();
        }
    }

    pub fn update_effect_if_necessary(&self, effect_id: EffectId) {
        if effect_id.is_null() || !self.effects.borrow().contains_key(effect_id) {
            return;
        }

        struct Checking<'a>(&'a RefCell<Vec<EffectId>>);
        impl Drop for Checking<'_> {
            fn drop(&mut self) {
                self.0.borrow_mut().pop();
            }
        }
        assert!(
            !self.checking.borrow().contains(&effect_id),
            "cyclic computed dependency at effect {effect_id:?}"
        );
        self.checking.borrow_mut().push(effect_id);
        let _checking = Checking(&self.checking);
        let mut effect = self.effect_mut(effect_id);
        assert!(
            !effect.running,
            "cannot read a running computed effect {effect_id:?}"
        );
        if effect.state == EffectState::Check {
            let dependencies = effect.dependencies.clone();
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
            drop(_checking);
            self.update(effect_id);
        } else {
            effect.state = EffectState::Clean;
            drop(effect);
            drop(_checking);
            self.maybe_flush();
        }
    }

    pub fn update_signal_if_necessary(&self, signal_id: SignalId) {
        let effect_id = self.signal_mut(signal_id).effect;
        if let Some(effect_id) = effect_id {
            self.update_effect_if_necessary(effect_id);
        }
    }

    fn mark_stale(&self, effect_id: EffectId, state: EffectState) {
        let mut effects = self.effects.borrow_mut();
        let Some(effect) = effects.get_mut(effect_id) else {
            return;
        };
        effect.update_context = crate::update_context::UpdateContext::current();
        if effect.running {
            if state == EffectState::Dirty {
                effect.rerun = true;
            }
            return;
        }
        if effect.state >= state {
            return;
        }

        let old_state = mem::replace(&mut effect.state, state);
        if old_state == EffectState::Clean {
            self.pending_updates.borrow_mut().push_back(effect_id);
        } else {
            return;
        }

        let signal = effect.signal;
        drop(effects);

        let subscribers = mem::take(&mut self.signal_mut(signal).subscribers);
        for subscriber in &subscribers {
            self.mark_stale(*subscriber, EffectState::Check);
        }
        self.signal_mut(signal).subscribers = subscribers;
    }

    fn flush_updates(&self) {
        if self.flushing.replace(true) {
            return;
        }
        struct Restore<'a>(&'a Cell<bool>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _restore = Restore(&self.flushing);
        self.executions.borrow_mut().clear();
        loop {
            let pending = self.pending_updates.borrow_mut().pop_front();
            let Some(effect_id) = pending else {
                break;
            };
            self.update_effect_if_necessary(effect_id);
        }
    }

    pub fn batch(&self, f: impl FnOnce()) {
        struct Restore<'a>(&'a Cell<usize>, usize);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let depth = self.batch_depth.replace(self.batch_depth.get() + 1);
        {
            let _restore = Restore(&self.batch_depth, depth);
            f();
        }
        self.maybe_flush();
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

        let cleanup_result = catch_unwind(AssertUnwindSafe(|| {
            self.run_cleanups(effect.owner, mem::take(&mut effect.cleanups));
        }));

        // Remove this effect from all signals it depends on
        for signal_id in &*effect.dependencies {
            if let Some(signal) = self.signals.borrow_mut().get_mut(*signal_id) {
                signal.subscribers.remove(&effect_id);
            }
        }

        self.dispose_signal(effect.signal);
        if let Err(error) = cleanup_result {
            resume_unwind(error);
        }
        drop(effect);
        self.maybe_flush();
    }
}

pub fn batch(f: impl FnOnce()) {
    Runtime::with(|rt| rt.batch(f))
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
