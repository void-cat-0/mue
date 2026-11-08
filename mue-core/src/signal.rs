use std::{any::Any, collections::HashSet, marker::PhantomData, ops::Deref, rc::Rc};

use slotmap::new_key_type;

use crate::{Disposable, effect::EffectId, prop::Prop, runtime::Runtime, scope::CURRENT_SCOPE};

new_key_type! {
    pub(crate) struct SignalId;
}

pub type Value = Box<dyn Any>;

pub type Comparator = fn(&dyn Any, &dyn Any) -> bool;

pub(crate) struct SignalInner {
    pub value: Option<Value>,
    pub effect: Option<EffectId>,
    pub subscribers: HashSet<EffectId>,
    pub comparator: Option<Comparator>,
    pub revision: u64,
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub location: &'static std::panic::Location<'static>,
}

impl SignalInner {
    pub fn new(
        value: Option<Value>,
        comparator: Option<Comparator>,
        #[cfg(debug_assertions)] location: &'static std::panic::Location<'static>,
    ) -> Self {
        Self {
            value,
            effect: None,
            subscribers: HashSet::new(),
            comparator,
            revision: 0,
            #[cfg(debug_assertions)]
            location,
        }
    }

    pub fn register(self, rt: &Runtime) -> SignalId {
        rt.assert_resource_creation_allowed();
        rt.signals.borrow_mut().insert(self)
    }
}

/// A copyable read-only handle. Tracked reads subscribe the current effect.
///
/// The handle is non-owning and bound to the thread that owns its runtime.
/// Fallible reads handle disposal of this signal or an unavailable runtime. They
/// do not make a still-live derived signal with disposed dependencies safe to
/// recompute: ordinary reads of those dependencies remain lifecycle misuse.
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::signal::ReadSignal<()>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<mue_core::signal::ReadSignal<()>>();
/// ```
pub struct ReadSignal<T> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl<T> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ReadSignal<T> {}

/// Read reactive values with or without subscribing to changes.
pub trait Access {
    type Value;

    fn get(&self) -> Self::Value
    where
        Self::Value: Copy,
    {
        self.get_clone()
    }

    fn get_untracked(&self) -> Self::Value
    where
        Self::Value: Copy,
    {
        self.get_clone_untracked()
    }

    fn track(&self);

    fn get_clone(&self) -> Self::Value
    where
        Self::Value: Clone,
    {
        self.track();
        self.get_clone_untracked()
    }

    fn get_clone_untracked(&self) -> Self::Value
    where
        Self::Value: Clone;
}

impl<T> ReadSignal<T> {
    pub(crate) fn new(id: SignalId) -> Self {
        CURRENT_SCOPE.with_borrow_mut(|scope| {
            scope.push_signal(Self {
                id,
                _marker: PhantomData,
                _thread_affinity: PhantomData,
            });
        });
        Self {
            id,
            _marker: PhantomData,
            _thread_affinity: PhantomData,
        }
    }

    pub fn null() -> Self {
        Self {
            id: Runtime::with(|rt| rt.null_signal),
            _marker: PhantomData,
            _thread_affinity: PhantomData,
        }
    }

    /// Notification revision after bringing this signal up to date.
    /// Does not subscribe; untracked writes do not advance the revision.
    #[doc(hidden)]
    pub fn revision_untracked(&self) -> u64 {
        Runtime::with(|rt| {
            rt.update_signal_if_necessary(self.id);
            rt.signal_mut(self.id).revision
        })
    }

    pub fn is_null(&self) -> bool {
        Runtime::with(|rt| self.id == rt.null_signal)
    }

    /// Whether this handle's owning scope or signal has been disposed.
    pub fn is_disposed(&self) -> bool {
        Runtime::with(|rt| !rt.signals.borrow().contains_key(self.id))
    }

    fn with_inner_mut<R>(self, rt: &Runtime, f: impl FnOnce(&mut SignalInner) -> R) -> R {
        f(&mut rt.signal_mut(self.id))
    }

    #[track_caller]
    /// Derive a value, notifying dependents only when the result changes.
    pub fn map<U: PartialEq + 'static>(self, mut f: impl FnMut(T) -> U + 'static) -> ReadSignal<U>
    where
        T: Clone + 'static,
    {
        crate::effect::computed(move |_| f(self.get_clone()))
    }
    #[track_caller]
    pub fn map_always<U: 'static>(self, mut f: impl FnMut(T) -> U + 'static) -> ReadSignal<U>
    where
        T: Clone + 'static,
    {
        crate::effect::computed_always(move |_| f(self.get_clone()))
    }
}

// Temporarily own the value so Clone, comparators, and value destructors can run
// without borrowing the signal arena. Restore only into the same live, empty slot.
struct SignalValue<'a> {
    runtime: &'a Runtime,
    id: SignalId,
    value: Option<Value>,
    comparator: Option<Comparator>,
}

impl<'a> SignalValue<'a> {
    fn take(runtime: &'a Runtime, id: SignalId) -> Option<Self> {
        let (value, comparator) = {
            let mut signals = runtime.signals.borrow_mut();
            let signal = signals.get_mut(id)?;
            (signal.value.take()?, signal.comparator)
        };
        Some(Self {
            runtime,
            id,
            value: Some(value),
            comparator,
        })
    }
}

impl Drop for SignalValue<'_> {
    fn drop(&mut self) {
        let mut value = self.value.take();
        {
            let mut signals = self.runtime.signals.borrow_mut();
            if let Some(signal) = signals.get_mut(self.id)
                && signal.value.is_none()
            {
                signal.value = value.take();
            }
        }
        // Disposal or a reentrant replacement must not drop user values while
        // the arena is borrowed, nor resurrect the original signal/value.
        drop(value);
    }
}

impl<T: 'static> ReadSignal<T> {
    /// Read and subscribe, returning None if this signal or runtime is unavailable.
    ///
    /// Like [`Self::try_get_clone`], this can still panic if a live derived
    /// signal's callback reads a disposed dependency.
    pub fn try_get(&self) -> Option<T>
    where
        T: Copy,
    {
        self.try_get_clone()
    }

    /// Clone and subscribe, returning None if this signal or runtime is unavailable.
    ///
    /// Derived signals are refreshed before reading and checked again afterwards,
    /// because recomputation can dispose this signal. A value temporarily taken
    /// by an in-progress callback, clone, or comparison is also unavailable.
    /// This does not catch user panics or ordinary lifecycle misuse: a still-live
    /// derived signal whose callback reads a disposed dependency can still panic.
    pub fn try_get_clone(&self) -> Option<T>
    where
        T: Clone,
    {
        self.try_get_clone_inner(true)
    }

    /// Read without subscribing, returning None when the signal/runtime is unavailable.
    /// See [`Self::try_get_clone`] for derived-signal lifecycle limits.
    pub fn try_get_untracked(&self) -> Option<T>
    where
        T: Copy,
    {
        self.try_get_clone_untracked()
    }

    /// Clone without subscribing, returning None when the signal/runtime is unavailable.
    /// See [`Self::try_get_clone`] for derived-signal lifecycle limits.
    pub fn try_get_clone_untracked(&self) -> Option<T>
    where
        T: Clone,
    {
        self.try_get_clone_inner(false)
    }

    fn try_get_clone_inner(&self, tracked: bool) -> Option<T>
    where
        T: Clone,
    {
        Runtime::try_with(|rt| {
            if !rt.signals.borrow().contains_key(self.id) {
                return None;
            }
            rt.update_signal_if_necessary(self.id);
            if !rt.signals.borrow().contains_key(self.id) {
                return None;
            }
            let value = SignalValue::take(rt, self.id)?;
            let cloned = value
                .value
                .as_ref()
                .unwrap()
                .downcast_ref::<T>()
                .unwrap()
                .clone();
            drop(value);
            // Clone (or dropping the original after disposal) can also run user code.
            if !rt.signals.borrow().contains_key(self.id) {
                return None;
            }
            if tracked {
                rt.track(self.id);
            }
            Some(cloned)
        })
        .ok()
        .flatten()
    }
}

impl ReadSignal<bool> {
    /// Choose between two values whenever this boolean changes.
    /// For example, `expanded.select(480., 180.)` supplies a reactive pixel width.
    pub fn select<T: Clone + PartialEq + 'static>(
        self,
        when_true: T,
        when_false: T,
    ) -> ReadSignal<T> {
        self.map(move |value| {
            if value {
                when_true.clone()
            } else {
                when_false.clone()
            }
        })
    }

    /// Choose between two properties whenever this boolean changes.
    ///
    /// Unlike [`ReadSignal::select`], the branches are properties, so a reactive
    /// branch keeps updating while it is selected.
    pub fn pick<T, A, B>(self, when_true: A, when_false: B) -> Prop<T>
    where
        T: Clone + PartialEq + 'static,
        A: Into<Prop<T>>,
        B: Into<Prop<T>>,
    {
        Prop::from(self).pick(when_true, when_false)
    }
}

impl<T> Disposable for ReadSignal<T> {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| rt.dispose_signal(self.id));
    }
}

impl<T: 'static> Access for ReadSignal<T> {
    type Value = T;

    fn track(&self) {
        Runtime::with(|rt| rt.track(self.id));
    }

    fn get_clone_untracked(&self) -> T
    where
        T: Clone,
    {
        Runtime::with(|rt| {
            rt.update_signal_if_necessary(self.id);
            self.with_inner_mut(rt, |inner| {
                inner
                    .value
                    .as_ref()
                    .unwrap()
                    .downcast_ref::<T>()
                    .unwrap()
                    .clone()
            })
        })
    }
}

/// A copyable writable handle whose updates notify subscribed effects.
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::signal::Signal<()>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<mue_core::signal::Signal<()>>();
/// ```
#[repr(transparent)]
pub struct Signal<T>(ReadSignal<T>);

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl Signal<bool> {
    /// Invert the value and notify dependents.
    pub fn toggle(self) {
        self.set_with(|value| !value);
    }
}
impl<T> Copy for Signal<T> {}

impl<T: 'static> Signal<T> {
    #[track_caller]
    pub fn new(value: T) -> Self
    where
        T: PartialEq,
    {
        Self::new_by(value, |a, b| a.downcast_ref::<T>() == b.downcast_ref::<T>())
    }

    #[track_caller]
    pub fn new_by(value: T, comparator: Comparator) -> Self {
        create_signal(SignalInner::new(
            Some(Box::new(value)),
            Some(comparator),
            #[cfg(debug_assertions)]
            std::panic::Location::caller(),
        ))
    }

    /// Create a signal without a comparator: every set notifies, even for equal values.
    ///
    /// Use this for payloads that are not `PartialEq` (handles, `Rc<dyn Trait>`, closures),
    /// or when an expensive or unreliable comparison is undesirable.
    #[track_caller]
    pub fn new_always(value: T) -> Self {
        create_signal(SignalInner::new(
            Some(Box::new(value)),
            None,
            #[cfg(debug_assertions)]
            std::panic::Location::caller(),
        ))
    }

    pub fn null() -> Self {
        Self(ReadSignal::null())
    }

    pub fn force_trigger(self) {
        Runtime::with(|rt| {
            rt.on_update(self.id);
        });
    }

    pub fn set_with(self, f: impl FnOnce(&T) -> T) -> bool {
        let updated = self.set_with_untracked(f);
        if updated {
            Runtime::with(|rt| rt.on_update(self.id));
        }
        updated
    }

    pub fn set_with_untracked(self, f: impl FnOnce(&T) -> T) -> bool {
        Runtime::with(|rt| {
            self.with_inner_mut(rt, |inner| {
                let new_value = f(inner.value.as_ref().unwrap().downcast_ref::<T>().unwrap());
                if inner
                    .comparator
                    .is_none_or(|cmp| !cmp(inner.value.as_ref().unwrap().as_ref(), &new_value))
                {
                    inner.value = Some(Box::new(new_value));
                    true
                } else {
                    false
                }
            })
        })
    }

    pub fn set(self, value: T) {
        self.set_with(|_| value);
    }

    /// Set and notify, returning the supplied value if the signal/runtime is unavailable.
    ///
    /// The comparator behaves as in set: equal values retain the old value without
    /// notifying, and signals without a comparator always notify. A value temporarily
    /// taken by an in-progress callback, clone, or comparison is unavailable.
    /// User panics are not caught. Once accepted, dropping the old or unused value
    /// may dispose this signal; the write still returns Ok(()).
    pub fn try_set(self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        Runtime::try_with(|rt| {
            let Some(current) = SignalValue::take(rt, self.id) else {
                return Err(value.take().unwrap());
            };
            let changed = current.comparator.is_none_or(|compare| {
                !compare(
                    current.value.as_ref().unwrap().as_ref(),
                    value.as_ref().unwrap(),
                )
            });
            if !rt.signals.borrow().contains_key(self.id) {
                drop(current);
                return Err(value.take().unwrap());
            }
            let replaced = if changed {
                // Publish the replacement before dropping either old value, so
                // destructors can safely reenter and observe the accepted write.
                rt.signal_mut(self.id)
                    .value
                    .replace(Box::new(value.take().unwrap()))
            } else {
                None
            };
            drop(current);
            drop(replaced);
            drop(value.take());
            // A value destructor can reenter and dispose the signal.
            if changed && rt.signals.borrow().contains_key(self.id) {
                rt.on_update(self.id);
            }
            Ok(())
        })
        .unwrap_or_else(|_| Err(value.take().unwrap()))
    }

    pub fn set_untracked(self, value: T) {
        self.set_with_untracked(|_| value);
    }

    pub fn update<R>(self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = self.update_untracked(f);
        Runtime::with(|rt| rt.on_update(self.id));
        result
    }

    pub fn update_untracked<R>(self, f: impl FnOnce(&mut T) -> R) -> R {
        Runtime::with(|rt| {
            self.with_inner_mut(rt, |inner| {
                f(inner.value.as_mut().unwrap().downcast_mut::<T>().unwrap())
            })
        })
    }
}

impl<T> Deref for Signal<T> {
    type Target = ReadSignal<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Disposable for Signal<T> {
    fn dispose(&self) {
        self.0.dispose();
    }
}

fn create_signal<T: 'static>(inner: SignalInner) -> Signal<T> {
    let id = Runtime::with(|rt| {
        rt.assert_resource_creation_allowed();
        inner.register(rt)
    });
    Signal(ReadSignal::new(id))
}

#[track_caller]
/// Create equality-checked state in the current scope.
pub fn signal<T: PartialEq + 'static>(value: T) -> Signal<T> {
    Signal::new(value)
}

#[cfg(test)]
mod test {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use crate::{prelude::*, runtime::Runtime};

    use super::ReadSignal;

    struct DropProbe {
        value: u32,
        on_drop: Option<Rc<dyn Fn()>>,
    }

    impl Clone for DropProbe {
        fn clone(&self) -> Self {
            Self {
                value: self.value,
                on_drop: None,
            }
        }
    }

    impl PartialEq for DropProbe {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            if let Some(on_drop) = &self.on_drop {
                on_drop();
            }
        }
    }

    #[test]
    fn new_always_notifies_even_when_the_value_is_equal() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let value = Signal::new_always(1u32);
            let read = *value;
            let runs = Rc::new(RefCell::new(0));
            watch_effect({
                let runs = runs.clone();
                move || {
                    read.get();
                    *runs.borrow_mut() += 1;
                }
            });
            assert_eq!(*runs.borrow(), 1);
            value.set(1);
            assert_eq!(*runs.borrow(), 2);
        });
    }

    #[test]
    fn notification_revisions_distinguish_equal_silent_and_computed_updates() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0);
            let initial = source.revision_untracked();
            source.set(0);
            assert_eq!(source.revision_untracked(), initial);
            source.set_untracked(1);
            assert_eq!(source.revision_untracked(), initial);
            source.force_trigger();
            assert_eq!(source.revision_untracked(), initial.wrapping_add(1));
            let parity = source.map(|n| n % 2);
            let revision = parity.revision_untracked();
            batch(|| {
                source.set(3);
                assert_eq!(parity.revision_untracked(), revision);
                source.set(4);
                assert_ne!(parity.revision_untracked(), revision);
            });
            let runs = Rc::new(RefCell::new(0));
            watch_effect({
                let runs = runs.clone();
                move || {
                    source.revision_untracked();
                    *runs.borrow_mut() += 1;
                }
            });
            source.set(5);
            assert_eq!(*runs.borrow(), 1);
        });
    }

    #[test]
    fn test_signal_basic() {
        let count = signal(0);
        assert_eq!(count.get(), 0);

        count.set(5);
        assert_eq!(count.get(), 5);

        count.set_with(|x| x + 1);
        assert_eq!(count.get(), 6);

        count.update(|x| *x += 1);
        assert_eq!(count.get(), 7);
    }

    #[test]
    fn test_read_signal() {
        let signal = signal(42);
        let read_only: ReadSignal<i32> = *signal;

        assert_eq!(read_only.get(), 42);

        // Can still write through original signal
        signal.set(100);
        assert_eq!(read_only.get(), 100);
    }

    #[test]
    fn test_get_untracked() {
        let count = signal(0);
        let tracked_reads = Rc::new(RefCell::new(0));
        let tracked_reads_clone = tracked_reads.clone();

        watch_effect(move || {
            count.get();
            *tracked_reads_clone.borrow_mut() += 1;
        });

        assert_eq!(*tracked_reads.borrow(), 1);

        // Untracked read should not trigger effect
        let _ = count.get_untracked();
        assert_eq!(*tracked_reads.borrow(), 1);

        // Tracked read inside effect should still work
        count.set(1);
        assert_eq!(*tracked_reads.borrow(), 2);
    }

    #[test]
    fn test_signal_dispose() {
        let count = signal(0);
        let runs = Rc::new(RefCell::new(0));
        let runs_clone = runs.clone();

        let effect = watch_effect(move || {
            count.get();
            *runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*runs.borrow(), 1);

        // Update should trigger effect
        count.set(1);
        assert_eq!(*runs.borrow(), 2);

        // Dispose the effect
        effect.dispose();

        // Update should not trigger effect anymore
        count.set(2);
        assert_eq!(*runs.borrow(), 2);
    }

    #[test]
    fn test_signal_dispose_removes_dependencies() {
        let count = signal(0);

        let effect = watch_effect(move || {
            count.get();
        });

        // Dispose the effect
        effect.dispose();

        // Signal should have no subscribers now
        Runtime::with(|rt| {
            let signals = rt.signals.borrow();
            let signal = signals.get(count.id).unwrap();
            assert!(signal.subscribers.is_empty());
        });
    }

    #[test]
    fn fallible_access_returns_none_for_disposed_handles_and_stale_slot_reuse() {
        let owner = OwnedScope::new();
        let (read, id) = owner.run(|| {
            let value = signal(21);
            (*value, value.id)
        });
        owner.dispose();
        assert_eq!(read.try_get(), None);
        assert_eq!(read.try_get_untracked(), None);
        assert_eq!(read.try_get_clone(), None);
        assert_eq!(read.try_get_clone_untracked(), None);

        let replacement_owner = OwnedScope::new();
        replacement_owner.run(|| {
            let replacement = signal(99);
            use slotmap::Key;
            assert_ne!(replacement.id, id);
            assert_eq!(
                replacement.id.data().as_ffi() as u32,
                id.data().as_ffi() as u32
            );
            let runs = Rc::new(Cell::new(0));
            let watch_runs = runs.clone();
            watch_effect(move || {
                assert_eq!(read.try_get(), None);
                watch_runs.set(watch_runs.get() + 1);
            });
            replacement.set(100);
            assert_eq!(runs.get(), 1);
            assert_eq!(read.try_get(), None);
            assert_eq!(replacement.try_get(), Some(100));
        });
    }

    #[test]
    fn fallible_reads_track_only_after_success_and_refresh_derived_signals() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(1);
            let derived = source.map(|value| value + 1);
            let reads = Rc::new(RefCell::new(0));
            let runs = reads.clone();
            watch_effect(move || {
                assert!(derived.try_get().is_some());
                *runs.borrow_mut() += 1;
            });
            assert_eq!(*reads.borrow(), 1);
            source.set(4);
            assert_eq!(*reads.borrow(), 2);
            assert_eq!(derived.try_get_untracked(), Some(5));
            assert_eq!(*reads.borrow(), 2);
            batch(|| {
                source.set(9);
                assert_eq!(derived.try_get_clone_untracked(), Some(10));
            });
            assert_eq!(*reads.borrow(), 3);

            let untracked_source = signal(String::from("first"));
            let untracked_runs = Rc::new(Cell::new(0));
            let runs = untracked_runs.clone();
            watch_effect(move || {
                assert!(untracked_source.try_get_clone_untracked().is_some());
                runs.set(runs.get() + 1);
            });
            untracked_source.set(String::from("second"));
            assert_eq!(untracked_runs.get(), 1);
        });
    }

    #[test]
    fn fallible_reads_return_none_when_recomputation_disposes_derived_signal() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(0);
            let derived_handle = Rc::new(Cell::new(None::<ReadSignal<i32>>));
            let callback_handle = derived_handle.clone();
            let derived = crate::effect::computed(move |_| {
                if let Some(handle) = callback_handle.get() {
                    handle.dispose();
                }
                source.get()
            });
            derived_handle.set(Some(derived));
            batch(|| {
                source.set(1);
                assert_eq!(derived.try_get(), None);
            });
        });
    }

    #[test]
    fn try_set_preserves_equality_and_notifies_once_for_changes() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let value = signal(1);
            let runs = Rc::new(RefCell::new(0));
            let watch_runs = runs.clone();
            watch_effect(move || {
                value.get();
                *watch_runs.borrow_mut() += 1;
            });
            assert_eq!(value.try_set(1), Ok(()));
            assert_eq!(*runs.borrow(), 1);
            assert_eq!(value.try_set(2), Ok(()));
            assert_eq!(*runs.borrow(), 2);
            assert_eq!(value.get_untracked(), 2);

            let always = Signal::new_always(3);
            let always_runs = Rc::new(RefCell::new(0));
            let watch_runs = always_runs.clone();
            watch_effect(move || {
                always.get();
                *watch_runs.borrow_mut() += 1;
            });
            assert_eq!(always.try_set(3), Ok(()));
            assert_eq!(*always_runs.borrow(), 2);
        });
    }

    #[test]
    fn try_set_drops_replaced_and_unused_values_without_arena_borrows() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let handle = Rc::new(Cell::new(None::<Signal<DropProbe>>));
            let hook_handle = handle.clone();
            let value = Signal::new(DropProbe {
                value: 1,
                on_drop: Some(Rc::new(move || {
                    if let Some(value) = hook_handle.get() {
                        let _ = value.try_set(DropProbe {
                            value: 3,
                            on_drop: None,
                        });
                    }
                })),
            });
            handle.set(Some(value));
            assert!(
                value
                    .try_set(DropProbe {
                        value: 2,
                        on_drop: None,
                    })
                    .is_ok()
            );
            assert_eq!(value.try_get_clone().map(|value| value.value), Some(3));

            let reentered = Rc::new(Cell::new(false));
            let reentered_by_drop = reentered.clone();
            let unused = Signal::new(DropProbe {
                value: 4,
                on_drop: None,
            });
            let read = *unused;
            assert!(
                unused
                    .try_set(DropProbe {
                        value: 4,
                        on_drop: Some(Rc::new(move || {
                            reentered_by_drop.set(read.try_get_clone().is_some());
                        })),
                    })
                    .is_ok()
            );
            assert!(reentered.get());
            assert_eq!(unused.try_get_clone().map(|value| value.value), Some(4));
        });
    }

    #[test]
    fn try_set_returns_value_when_disposed() {
        let owner = OwnedScope::new();
        let value = owner.run(|| signal(1));
        owner.dispose();
        assert_eq!(value.try_set(7), Err(7));
    }

    #[test]
    fn test_force_trigger() {
        let count = signal(0);
        let runs = Rc::new(RefCell::new(0));
        let runs_clone = runs.clone();

        watch_effect(move || {
            count.get();
            *runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*runs.borrow(), 1);

        // Force trigger should run effect even if value doesn't change
        count.force_trigger();
        assert_eq!(*runs.borrow(), 2);
    }
}
