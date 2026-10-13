use std::{
    cell::{Cell, RefCell, RefMut},
    mem,
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
        effect.cleanup();
        let update_context = mem::take(&mut effect.update_context);

        let tracker = match &mut effect.dependencies {
            Dependencies::Static(_) => None,
            Dependencies::Dynamic(deps) => Some(DependencyTracker::new(mem::take(deps))),
        };
        let prev_tracker = self.tracker.replace(tracker);

        let prev_effect = self.current_effect.replace(Some(effect_id));

        let signal_id = effect.signal;

        let mut result = None;
        let mut effect_callback = None;
        let updated = if let Some(track_fn) = track_fn {
            drop(effect);
            result = Some(track_fn());
            false
        } else {
            let mut value = self.signal_mut(signal_id).value.take();
            let mut callback = effect.callback.take().unwrap();
            drop(effect);

            let updated = update_context.run(|| callback(&mut value));
            self.signal_mut(signal_id).value = value;
            effect_callback = Some(callback);
            updated
        };

        self.current_effect.replace(prev_effect);

        let mut effect = self.effect_mut(effect_id);
        effect.state = EffectState::Clean;
        if effect_callback.is_some() {
            effect.callback = effect_callback;
        }
        if let Some(mut tracker) = self.tracker.replace(prev_tracker) {
            // The callback may dispose children while the old dependencies are
            // temporarily held by this tracker instead of EffectInner.
            let mut signals = self.signals.borrow_mut();
            for signal_id in tracker.dependencies.drain(tracker.index..) {
                if let Some(signal) = signals.get_mut(signal_id) {
                    signal.subscribers.remove(&effect_id);
                }
            }
            tracker.dependencies.retain(|id| signals.contains_key(*id));
            tracker.dependencies.reserve(tracker.new_dependencies.len());
            for signal_id in tracker.new_dependencies {
                if let Some(signal) = signals.get_mut(signal_id) {
                    signal.subscribers.insert(effect_id);
                    tracker.dependencies.push(signal_id);
                }
            }
            effect.dependencies = Dependencies::Dynamic(tracker.dependencies);
        }
        drop(effect);

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
            let dependencies = mem::take(&mut effect.dependencies);
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
            effect.dependencies = dependencies;
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
        let depth = self.batch_depth.get();
        self.batch_depth.set(depth + 1);
        f();

        if depth == 0 {
            self.flush_updates();
        }

        self.batch_depth.set(depth);
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
