use std::{ops::Deref, slice};

use slotmap::{new_key_type, Key};
use smallvec::SmallVec;

use crate::{
    prop::Prop,
    runtime::Runtime,
    scope::CURRENT_SCOPE,
    signal::{ReadSignal, SignalId, SignalInner, Value},
    Disposable,
};

new_key_type! {
    /// Unique identifier for an effect in the reactive system.
    /// Can be used to dispose effects created by `watch` or `watch_effect`.
    pub(crate) struct EffectId;
}

pub(crate) type EffectCallback = Box<dyn FnMut(&mut Option<Value>) -> bool>;
pub(crate) type CleanupCallback = Box<dyn FnOnce()>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum EffectState {
    Clean,
    Check,
    Dirty,
}

pub(crate) type DependencyList = SmallVec<[SignalId; 2]>;

pub(crate) enum Dependencies {
    Dynamic(DependencyList),
    Static(SignalId),
}

impl Default for Dependencies {
    fn default() -> Self {
        Self::Dynamic(SmallVec::new())
    }
}

impl Deref for Dependencies {
    type Target = [SignalId];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Dynamic(signal_ids) => signal_ids,
            Self::Static(signal_id) => slice::from_ref(signal_id),
        }
    }
}

pub(crate) struct EffectInner {
    pub callback: Option<EffectCallback>,
    pub cleanups: Vec<CleanupCallback>,
    pub signal: SignalId,
    pub dependencies: Dependencies,
    pub state: EffectState,
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub location: &'static std::panic::Location<'static>,
}

impl EffectInner {
    #[track_caller]
    pub fn new(
        callback: EffectCallback,
        signal: SignalId,
        dependencies: Dependencies,
        state: EffectState,
        #[cfg(debug_assertions)] location: &'static std::panic::Location<'static>,
    ) -> Self {
        Self {
            callback: Some(callback),
            cleanups: Vec::new(),
            signal,
            dependencies,
            state,
            #[cfg(debug_assertions)]
            location,
        }
    }

    pub fn cleanup(&mut self) {
        for cleanup in self.cleanups.drain(..) {
            cleanup();
        }
    }

    pub fn register(self, rt: &Runtime) -> EffectId {
        rt.effects.borrow_mut().insert(self)
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Effect {
    pub(crate) id: EffectId,
}

impl Effect {
    pub(crate) fn new(id: EffectId) -> Self {
        CURRENT_SCOPE.with_borrow_mut(|scope| {
            scope.push_effect(Self { id });
        });
        Self { id }
    }

    pub fn null() -> Self {
        Self {
            id: EffectId::null(),
        }
    }

    pub fn is_null(&self) -> bool {
        self.id.is_null()
    }

    pub fn force_trigger(&self) {
        Runtime::with(|rt| rt.update(self.id));
    }
}

impl Disposable for Effect {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| rt.dispose_effect(self.id));
    }
}

#[track_caller]
pub fn watch<T: 'static>(
    signal: ReadSignal<T>,
    mut f: impl FnMut(ReadSignal<T>) + 'static,
) -> Effect {
    #[cfg(debug_assertions)]
    let location = std::panic::Location::caller();
    Runtime::with(|rt| {
        let effect_id = EffectInner::new(
            Box::new(move |_value| {
                f(signal);
                false
            }),
            rt.null_signal,
            Dependencies::Static(signal.id),
            EffectState::Clean,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        rt.signal_mut(signal.id).subscribers.insert(effect_id);
        Effect::new(effect_id)
    })
}

#[track_caller]
pub fn watch_immediate<T: 'static>(
    signal: ReadSignal<T>,
    mut f: impl FnMut(Prop<T>) + 'static,
) -> Effect {
    #[cfg(debug_assertions)]
    let location = std::panic::Location::caller();
    Runtime::with(|rt| {
        let effect_id = EffectInner::new(
            Box::new(move |_value| {
                f(Prop::Dynamic(signal));
                false
            }),
            rt.null_signal,
            Dependencies::Static(signal.id),
            EffectState::Dirty,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        rt.signal_mut(signal.id).subscribers.insert(effect_id);
        let effect = Effect::new(effect_id);
        effect.force_trigger();
        effect
    })
}

#[track_caller]
pub fn watch_effect(mut f: impl FnMut() + 'static) -> Effect {
    #[cfg(debug_assertions)]
    let location = std::panic::Location::caller();
    Runtime::with(|rt| {
        let effect_id = EffectInner::new(
            Box::new(move |_value| {
                f();
                false
            }),
            rt.null_signal,
            Dependencies::default(),
            EffectState::Dirty,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        rt.update(effect_id);
        Effect::new(effect_id)
    })
}

/// Register a cleanup function to be called when the current effect is re-run or disposed.
/// This function should only be called within an effect callback.
///
/// # Example
/// ```
/// use mue_core::prelude::*;
///
/// let count = signal(0);
/// watch_effect(move || {
///     let id = 42; // Some resource ID
///     on_cleanup(move || {
///         println!("Cleaning up resource {}", id);
///     });
///     count.get();
/// });
/// ```
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    Runtime::with(|rt| {
        if let Some(effect_id) = *rt.current_effect.borrow() {
            if let Some(effect) = rt.effects.borrow_mut().get_mut(effect_id) {
                effect.cleanups.push(Box::new(f));
            }
        }
    });
}

#[track_caller]
fn create_computed<T: 'static>(callback: EffectCallback) -> ReadSignal<T> {
    #[cfg(debug_assertions)]
    let location = std::panic::Location::caller();
    Runtime::with(|rt| {
        let signal_id = SignalInner::new(
            None,
            None,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        let effect_id = EffectInner::new(
            callback,
            signal_id,
            Dependencies::default(),
            EffectState::Dirty,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        rt.signal_mut(signal_id).effect = Some(effect_id);
        rt.update(effect_id);
        ReadSignal::new(signal_id)
    })
}

#[track_caller]
pub fn computed<T: PartialEq + 'static>(
    mut f: impl FnMut(Option<&T>) -> T + 'static,
) -> ReadSignal<T> {
    create_computed(Box::new(move |value| {
        let new_value = f(value.as_mut().and_then(|v| v.downcast_ref::<T>()));
        if value
            .as_ref()
            .is_some_and(|value| value.downcast_ref::<T>() == Some(&new_value))
        {
            false
        } else {
            *value = Some(Box::new(new_value));
            true
        }
    }))
}

#[track_caller]
pub fn computed_always<T: 'static>(mut f: impl FnMut(Option<&T>) -> T + 'static) -> ReadSignal<T> {
    create_computed(Box::new(move |value| {
        let new_value = f(value.as_mut().and_then(|v| v.downcast_ref::<T>()));
        *value = Some(Box::new(new_value));
        true
    }))
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Reaction {
    pub(crate) effect_id: EffectId,
}

impl Reaction {
    fn new(effect_id: EffectId) -> Self {
        Self { effect_id }
    }

    pub fn track<R>(&self, f: impl FnOnce() -> R) -> R {
        Runtime::with(|rt| rt.update_with(self.effect_id, f))
    }
}

#[track_caller]
pub fn create_reaction(mut f: impl FnMut() + 'static) -> Reaction {
    #[cfg(debug_assertions)]
    let location = std::panic::Location::caller();
    Runtime::with(|rt| {
        let effect_id = EffectInner::new(
            Box::new(move |_value| {
                f();
                false
            }),
            rt.null_signal,
            Dependencies::default(),
            EffectState::Dirty,
            #[cfg(debug_assertions)]
            location,
        )
        .register(rt);
        Reaction::new(effect_id)
    })
}

#[cfg(test)]
mod test {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use crate::prelude::*;

    #[test]
    fn test_computed_basic() {
        let count = signal(2);
        let doubled = computed(move |_| count.get() * 2);

        assert_eq!(doubled.get(), 4);

        count.set(5);
        assert_eq!(doubled.get(), 10);
    }

    /// 回归测试：batch 里读一条 computed 链时，处于 Check 状态的 effect
    /// 会递归去更新依赖的处于 Dirty 状态的 computed，重入 Runtime::signal_mut，
    /// 以前会 panic「RefCell already borrowed」。
    #[test]
    fn test_computed_chain_in_batch() {
        let a = signal(1);
        let b = computed(move |_| a.get() + 1);
        let c = computed(move |_| b.get() + 1);

        assert_eq!(c.get(), 3);

        batch(|| {
            a.set(2);
            assert_eq!(c.get(), 4);
        });
    }

    #[test]
    fn test_effect_tracking() {
        let count = signal(0);
        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch_effect(move || {
            count.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*effect_runs.borrow(), 1);

        count.set(1);
        assert_eq!(*effect_runs.borrow(), 2);

        count.set(2);
        assert_eq!(*effect_runs.borrow(), 3);
    }

    #[test]
    fn test_watch() {
        let a = signal(1);
        let b = signal(10);

        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch(*a, move |_a| {
            b.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*effect_runs.borrow(), 0);
        a.set(2);
        assert_eq!(*effect_runs.borrow(), 1);
        b.set(20);
        assert_eq!(*effect_runs.borrow(), 1);
    }

    #[test]
    fn test_watch_prop() {
        let a = signal(1);
        let b = signal(10);

        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch_immediate(*a, move |_a| {
            b.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*effect_runs.borrow(), 1);
        a.set(2);
        assert_eq!(*effect_runs.borrow(), 2);
        b.set(20);
        assert_eq!(*effect_runs.borrow(), 2);

        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();
        let effect = watch_immediate(*signal(1), move |_a| {
            *effect_runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*effect_runs.borrow(), 1);
        effect.dispose();
    }

    #[test]
    fn test_watch_computed() {
        let a = signal(1);
        let b = computed(move |_| a.get() * 2);

        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch(b, move |_b| {
            *effect_runs_clone.borrow_mut() += 1;
        });

        a.set(2);
        assert_eq!(*effect_runs.borrow(), 1);
    }

    #[test]
    fn test_computed_dependency_chain() {
        let a = signal(1);
        let b = signal(2);

        let sum = computed(move |_| a.get() + b.get());
        let product = computed(move |_| sum.get() * 2);

        assert_eq!(sum.get(), 3);
        assert_eq!(product.get(), 6);

        a.set(5);
        assert_eq!(sum.get(), 7);
        assert_eq!(product.get(), 14);

        b.set(3);
        assert_eq!(sum.get(), 8);
        assert_eq!(product.get(), 16);
    }

    #[test]
    fn test_batch_defers_effects() {
        let a = signal(1);
        let b = signal(2);
        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch_effect(move || {
            a.get();
            b.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        // Initial run
        assert_eq!(*effect_runs.borrow(), 1);

        // Multiple updates in batch should only trigger effect once
        batch(|| {
            a.set(10);
            b.set(20);
        });
        assert_eq!(*effect_runs.borrow(), 2);
    }

    #[test]
    fn test_batch_nested() {
        let a = signal(1);
        let b = signal(2);
        let c = signal(3);
        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch_effect(move || {
            a.get();
            b.get();
            c.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        // Initial run
        assert_eq!(*effect_runs.borrow(), 1);

        // Nested batches should still only trigger effect once at the end
        batch(|| {
            a.set(10);
            batch(|| {
                b.set(20);
                c.set(30);
            });
        });
        assert_eq!(*effect_runs.borrow(), 2);
    }

    #[test]
    fn test_batch_with_memo() {
        let a = signal(1);
        let b = signal(2);

        let sum = computed(move |_| a.get() + b.get());

        // Initial value
        assert_eq!(sum.get(), 3);

        // Update in batch
        batch(|| {
            a.set(10);
            b.set(20);
            // Memo should not be recalculated yet during batch
        });

        // After batch, memo should reflect new values
        assert_eq!(sum.get(), 30);
    }

    #[test]
    fn test_nested_watch_immediate() {
        let a = signal(0);
        let b = computed(move |_| a.get() + 1);

        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        watch_immediate(b, move |_| {
            b.get();
            *effect_runs_clone.borrow_mut() += 1;
        });
    }

    #[test]
    fn test_force_trigger() {
        let count = signal(0);
        let effect_runs = Rc::new(RefCell::new(0));
        let effect_runs_clone = effect_runs.clone();

        let effect = watch_effect(move || {
            count.get();
            *effect_runs_clone.borrow_mut() += 1;
        });

        // Initial run
        assert_eq!(*effect_runs.borrow(), 1);

        // Force trigger should run effect even if dependencies haven't changed
        effect.force_trigger();
        assert_eq!(*effect_runs.borrow(), 2);

        // Update should also trigger effect
        count.set(1);
        assert_eq!(*effect_runs.borrow(), 3);
    }

    #[test]
    fn test_cleanup_on_rerun() {
        let count = signal(0);
        let cleanup_runs = Rc::new(RefCell::new(0));
        let cleanup_runs_clone = cleanup_runs.clone();

        watch_effect(move || {
            count.get();
            let cleanup_clone = cleanup_runs_clone.clone();
            on_cleanup(move || {
                *cleanup_clone.borrow_mut() += 1;
            });
        });

        // Initial run - cleanup not called yet
        assert_eq!(*cleanup_runs.borrow(), 0);

        // Trigger re-run - cleanup should be called
        count.set(1);
        assert_eq!(*cleanup_runs.borrow(), 1);

        // Another re-run
        count.set(2);
        assert_eq!(*cleanup_runs.borrow(), 2);
    }

    #[test]
    fn test_cleanup_on_dispose() {
        let count = signal(0);
        let cleanup_runs = Rc::new(RefCell::new(0));
        let cleanup_runs_clone = cleanup_runs.clone();

        let effect = watch_effect(move || {
            count.get();
            let cleanup_runs_clone = cleanup_runs_clone.clone();
            on_cleanup(move || {
                *cleanup_runs_clone.borrow_mut() += 1;
            });
        });

        // Initial run - cleanup not called yet
        assert_eq!(*cleanup_runs.borrow(), 0);

        // Dispose effect - cleanup should be called
        effect.dispose();
        assert_eq!(*cleanup_runs.borrow(), 1);

        // Update signal - cleanup should not be called again
        count.set(1);
        assert_eq!(*cleanup_runs.borrow(), 1);
    }

    #[test]
    fn test_cleanup_with_computed() {
        let count = signal(1);
        let doubled = computed(move |_| count.get() * 2);
        let cleanup_runs = Rc::new(RefCell::new(0));
        let cleanup_runs_clone = cleanup_runs.clone();

        watch_effect(move || {
            doubled.get();
            let cleanup_runs_clone = cleanup_runs_clone.clone();
            on_cleanup(move || {
                *cleanup_runs_clone.borrow_mut() += 1;
            });
        });

        // Initial run
        assert_eq!(*cleanup_runs.borrow(), 0);

        // Update signal - cleanup should run before re-run
        count.set(2);
        assert_eq!(*cleanup_runs.borrow(), 1);

        // Another update
        count.set(3);
        assert_eq!(*cleanup_runs.borrow(), 2);
    }

    #[test]
    fn test_cleanup_with_resources() {
        struct Resource {
            cleaned_up: Rc<Cell<bool>>,
        }

        impl Resource {
            fn new(cleaned_up: Rc<Cell<bool>>) -> Self {
                Self { cleaned_up }
            }

            fn cleanup(&mut self) {
                self.cleaned_up.set(true);
            }
        }

        let count = signal(0);
        let cleaned_up = Rc::new(Cell::new(false));
        let cleaned_up_clone = cleaned_up.clone();

        watch_effect(move || {
            let _ = count.get();
            let mut resource = Resource::new(cleaned_up_clone.clone());
            on_cleanup(move || {
                resource.cleanup();
            });
        });

        // Initial run
        assert!(!cleaned_up.get());

        // Trigger re-run - resource should be cleaned up
        count.set(1);
        assert!(cleaned_up.get());
    }

    #[test]
    fn test_cleanup_replacement() {
        let count = signal(0);
        let cleanup_values = Rc::new(RefCell::new(Vec::new()));
        let cleanup_values_clone = cleanup_values.clone();

        watch_effect(move || {
            let current = count.get();
            let values = cleanup_values_clone.clone();
            on_cleanup(move || {
                values.borrow_mut().push(current);
            });
        });

        // Initial run
        assert_eq!(*cleanup_values.borrow(), Vec::<i32>::new());

        // Each re-run should call the previous cleanup with the old value
        count.set(1);
        assert_eq!(*cleanup_values.borrow(), vec![0]);

        count.set(2);
        assert_eq!(*cleanup_values.borrow(), vec![0, 1]);

        count.set(3);
        assert_eq!(*cleanup_values.borrow(), vec![0, 1, 2]);
    }

    #[test]
    fn test_reaction() {
        let count = signal(0);
        let reaction_runs = Rc::new(RefCell::new(0));
        let reaction_runs_clone = reaction_runs.clone();

        let reaction = create_reaction(move || {
            *reaction_runs_clone.borrow_mut() += 1;
        });

        reaction.track(|| {
            count.get();
        });

        assert_eq!(*reaction_runs.borrow(), 0);

        count.set(1);
        assert_eq!(*reaction_runs.borrow(), 1);

        // Tracking the same reaction again should not cause it to run
        count.set(2);
        assert_eq!(*reaction_runs.borrow(), 1);

        reaction.track(|| {
            count.get();
        });
        count.set(3);
        assert_eq!(*reaction_runs.borrow(), 2);
    }
}
