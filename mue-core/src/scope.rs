use std::{cell::RefCell, ops::Deref};

use slotmap::{Key, SlotMap, new_key_type};
use type_map::TypeMap;

use crate::{
    Disposable,
    effect::{Effect, EffectId},
    runtime::Runtime,
    signal::{ReadSignal, SignalId},
};

new_key_type! {
    pub(crate) struct ScopeId;
}

thread_local! {
    pub(crate) static CURRENT_SCOPE: RefCell<Scope> = RefCell::new(Scope::null());
}

pub(crate) struct ScopeInner {
    pub(crate) effects: Vec<EffectId>,
    pub(crate) signals: Vec<SignalId>,
    pub(crate) subscopes: Vec<ScopeId>,

    parent: Scope,
    context_parent: Scope,
    context: TypeMap,

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub location: &'static std::panic::Location<'static>,
}

impl ScopeInner {
    pub fn new(#[cfg(debug_assertions)] location: &'static std::panic::Location<'static>) -> Self {
        Self {
            effects: Vec::new(),
            signals: Vec::new(),
            subscopes: Vec::new(),

            parent: Scope::null(),
            context_parent: Scope::null(),
            context: TypeMap::new(),

            #[cfg(debug_assertions)]
            location,
        }
    }
}

impl Disposable for ScopeInner {
    fn dispose(&self) {
        let ScopeInner {
            effects,
            signals,
            subscopes,
            ..
        } = self;
        Runtime::with(|rt| {
            for effect_id in effects {
                rt.dispose_effect(*effect_id);
            }
            for signal_id in signals {
                rt.dispose_signal(*signal_id);
            }
            for subscope_id in subscopes {
                let scope = rt.scopes.borrow_mut().remove(*subscope_id);
                if let Some(scope) = scope {
                    scope.dispose();
                }
            }
        });
    }
}

/// A copyable, non-owning handle to a reactive scope.
#[derive(Clone, Copy)]
pub struct Scope {
    id: ScopeId,
}

/// Owns a detached scope and disposes its resources when dropped.
///
/// Moving this guard transfers ownership without attaching the scope to the
/// ambient scope. Context is explicit with `new`, or inherited with `new_inherited`.
/// Copying its [`Scope`] handle does not keep resources alive.
pub struct OwnedScope {
    scope: Scope,
}

impl OwnedScope {
    #[track_caller]
    pub fn new() -> Self {
        Self {
            scope: Scope::new_detached(),
        }
    }

    /// Create a detached owner with context lookup inherited from the current scope.
    /// This does not give the current scope ownership of the new scope.
    #[track_caller]
    pub fn new_inherited() -> Self {
        let owner = Self::new();
        owner.set_context_parent(current_scope());
        owner
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Relinquish RAII ownership. The caller must dispose the returned scope or
    /// transfer it to a parent with [`Scope::push_subscope`].
    pub fn into_scope(self) -> Scope {
        let scope = self.scope;
        std::mem::forget(self);
        scope
    }
}

impl Default for OwnedScope {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for OwnedScope {
    type Target = Scope;

    fn deref(&self) -> &Scope {
        &self.scope
    }
}

impl Drop for OwnedScope {
    fn drop(&mut self) {
        self.scope.dispose();
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new_attached()
    }
}

impl Scope {
    /// Create a scope owned by the current scope; drop the current scope to dispose it.
    #[track_caller]
    pub fn new_attached() -> Self {
        let scope = Self::new_detached();
        current_scope().push_subscope(scope);
        scope
    }

    /// Create an unparented scope, independent of the current scope's lifetime
    /// and context lookup chain. Prefer [`OwnedScope`] for automatic cleanup.
    #[track_caller]
    pub fn new_detached() -> Self {
        Runtime::with(|rt| {
            let id = rt.scopes.borrow_mut().insert(ScopeInner::new(
                #[cfg(debug_assertions)]
                std::panic::Location::caller(),
            ));
            Self { id }
        })
    }

    pub fn null() -> Self {
        Self {
            id: ScopeId::null(),
        }
    }

    pub fn is_null(&self) -> bool {
        self.id.is_null()
    }

    /// Whether this non-null scope still exists in the current runtime.
    pub fn is_alive(&self) -> bool {
        Runtime::try_with(|rt| rt.scopes.borrow().contains_key(self.id)).unwrap_or(false)
    }

    pub fn push_signal<T>(&self, signal: ReadSignal<T>) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            rt.scopes.borrow_mut()[self.id].signals.push(signal.id);
        });
    }

    pub fn push_effect(&self, effect: Effect) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            rt.scopes.borrow_mut()[self.id].effects.push(effect.id);
        });
    }

    pub fn push_subscope(&self, subscope: Scope) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            let mut scopes = rt.scopes.borrow_mut();
            assert!(
                scopes.contains_key(self.id),
                "Parent scope has been disposed"
            );
            assert!(
                scopes.contains_key(subscope.id),
                "Subscope has been disposed"
            );
            if scopes[subscope.id].parent.id == self.id {
                return;
            }
            if !scopes[subscope.id].parent.is_null() {
                panic!("Subscope already has a parent");
            }
            let mut ancestor = *self;
            while !ancestor.is_null() {
                assert_ne!(ancestor.id, subscope.id, "Subscope ownership cycle");
                ancestor = scopes[ancestor.id].parent;
            }
            let mut ancestor = *self;
            while let Some(inner) = scopes.get(ancestor.id) {
                assert_ne!(ancestor.id, subscope.id, "Subscope context parent cycle");
                ancestor = inner.context_parent;
            }
            scopes[subscope.id].parent = *self;
            scopes[subscope.id].context_parent = *self;
            scopes[self.id].subscopes.push(subscope.id);
        });
    }

    /// Run with this current scope, restoring the previous scope afterwards.
    pub fn run<R>(self, f: impl FnOnce() -> R) -> R {
        assert!(self.is_null() || self.is_alive(), "Scope has been disposed");
        let previous = CURRENT_SCOPE.replace(self);
        let value = f();
        CURRENT_SCOPE.set(previous);
        value
    }

    pub fn provide<T: 'static>(&self, value: T) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            rt.scopes.borrow_mut()[self.id].context.insert::<T>(value);
        });
    }

    /// Change context lookup without transferring resource ownership.
    pub fn set_context_parent(&self, parent: Scope) {
        assert!(self.is_alive(), "Scope has been disposed");
        assert!(
            parent.is_null() || parent.is_alive(),
            "Context parent has been disposed"
        );
        Runtime::with(|rt| {
            let mut scopes = rt.scopes.borrow_mut();
            let mut ancestor = parent;
            while !ancestor.is_null() {
                assert_ne!(ancestor.id, self.id, "Scope context parent cycle");
                let Some(inner) = scopes.get(ancestor.id) else {
                    break;
                };
                ancestor = inner.context_parent;
            }
            scopes[self.id].context_parent = parent;
        });
    }

    fn inject_with<T: 'static>(self, scopes: &SlotMap<ScopeId, ScopeInner>) -> Option<&T> {
        let mut scope = self;
        while !scope.is_null() {
            let Some(inner) = scopes.get(scope.id) else {
                break;
            };
            if let Some(context) = inner.context.get::<T>() {
                return Some(context);
            }
            scope = inner.context_parent;
        }
        None
    }

    pub fn provide_with<T: 'static>(&self, f: impl FnOnce(Option<&T>) -> T) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            let mut scopes = rt.scopes.borrow_mut();
            let value = f(self.inject_with::<T>(&scopes));
            scopes[self.id].context.insert::<T>(value);
        });
    }

    pub fn inject<T: Clone + 'static>(&self) -> Option<T> {
        Runtime::with(|rt| {
            let scopes = rt.scopes.borrow();
            self.inject_with(&scopes).cloned()
        })
    }
}

pub fn provide<T: 'static>(value: T) {
    current_scope().provide(value);
}

pub fn provide_with<T: 'static>(f: impl FnOnce(Option<&T>) -> T) {
    current_scope().provide_with(f);
}

pub fn inject<T: Clone + 'static>() -> Option<T> {
    current_scope().inject()
}

impl Disposable for Scope {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| {
            let scope = rt.scopes.borrow_mut().remove(self.id);
            if let Some(scope) = scope {
                if let Some(parent) = rt.scopes.borrow_mut().get_mut(scope.parent.id) {
                    parent.subscopes.retain(|id| *id != self.id);
                }
                scope.dispose();
            }
        });
    }
}

pub fn current_scope() -> Scope {
    CURRENT_SCOPE.with_borrow(|s| *s)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn detached_owner_survives_configuration_scope_and_moves() {
        let configuration = OwnedScope::new();
        let owner = configuration.run(OwnedScope::new);
        let scope = owner.scope();
        let value = owner.run(|| signal(42));
        let observer = value;
        drop(configuration);
        assert!(scope.is_alive());
        assert_eq!(observer.get(), 42);
        let node_owner = owner;
        assert!(scope.is_alive());
        drop(node_owner);
        assert!(!scope.is_alive());
        assert!(observer.is_disposed());
    }

    #[test]
    fn detached_context_inheritance_does_not_transfer_ownership() {
        let parent = OwnedScope::new();
        parent.provide(7u32);
        let detached = parent.run(OwnedScope::new);
        let inherited = parent.run(OwnedScope::new_inherited);
        let attached = parent.run(Scope::new_attached);
        assert_eq!(detached.inject::<u32>(), None);
        assert_eq!(inherited.inject::<u32>(), Some(7));
        assert_eq!(attached.inject::<u32>(), Some(7));
        let value = inherited.run(|| signal(1));
        drop(parent);
        assert!(!attached.is_alive());
        assert!(inherited.is_alive());
        assert_eq!(inherited.inject::<u32>(), None);
        assert_eq!(value.get(), 1);
        drop(inherited);
        assert!(value.is_disposed());
    }

    #[test]
    fn context_parent_rebinding_rejects_cycles() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let first = OwnedScope::new();
        let second = OwnedScope::new();
        first.provide(12u32);
        second.set_context_parent(first.scope());
        assert_eq!(second.inject::<u32>(), Some(12));
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                first.set_context_parent(second.scope());
            }))
            .is_err()
        );
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                second.push_subscope(first.scope());
            }))
            .is_err()
        );
        second.set_context_parent(Scope::null());
        assert_eq!(second.inject::<u32>(), None);
        first.push_subscope(second.into_scope());
    }

    use crate::prelude::*;

    #[test]
    fn test_scope_disposes_on_drop() {
        let count = signal(0);
        let runs = Rc::new(RefCell::new(0));
        let runs_clone = runs.clone();

        let scope = OwnedScope::new();
        scope.run(|| {
            watch_effect(move || {
                count.get();
                *runs_clone.borrow_mut() += 1;
            });

            // Effect runs once inside scope
            assert_eq!(*runs.borrow(), 1);

            // Update triggers effect
            count.set(1);
            assert_eq!(*runs.borrow(), 2);
        });
        drop(scope);

        // Update should not trigger effect anymore
        count.set(2);
        assert_eq!(*runs.borrow(), 2);
    }

    #[test]
    fn test_nested_scopes() {
        let outer_runs = Rc::new(RefCell::new(0));
        let inner_runs = Rc::new(RefCell::new(0));
        let outer_clone = outer_runs.clone();
        let inner_clone = inner_runs.clone();

        let count = signal(0);

        let scope_outer = OwnedScope::new();
        scope_outer.run(|| {
            watch_effect(move || {
                count.get();
                *outer_clone.borrow_mut() += 1;
            });

            let scope_inner = OwnedScope::new();
            scope_inner.run(|| {
                watch_effect(move || {
                    count.get();
                    *inner_clone.borrow_mut() += 1;
                });
            });
            // Inner effect runs
            assert_eq!(*inner_runs.borrow(), 1);
            drop(scope_inner);

            // Update triggers only outer effect
            count.set(1);
            assert_eq!(*outer_runs.borrow(), 2);
            assert_eq!(*inner_runs.borrow(), 1);
        });
        drop(scope_outer);

        count.set(2);
        // Neither effect should run
        assert_eq!(*outer_runs.borrow(), 2);
        assert_eq!(*inner_runs.borrow(), 1);
    }

    #[test]
    fn test_scope_cascade_dispose() {
        let runs = Rc::new(RefCell::new(0));
        let runs_clone = runs.clone();

        let count = signal(0);

        let scope_outer = OwnedScope::new();
        scope_outer.run(|| {
            watch_effect(move || {
                count.get();
                *runs_clone.borrow_mut() += 1;
            });

            let scope_inner = OwnedScope::new();
            scope_inner.run({
                let runs = runs.clone();
                move || {
                    watch_effect(move || {
                        count.get();
                        *runs.borrow_mut() += 1;
                    });
                }
            });
            // Both effects run
            assert_eq!(*runs.borrow(), 2);
            drop(scope_inner);
        });
        drop(scope_outer);

        count.set(1);
        // No effects should run
        assert_eq!(*runs.borrow(), 2);
    }
}
