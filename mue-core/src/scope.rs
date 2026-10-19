use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

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
            let mut first_panic = None;
            let mut dispose = |f: &mut dyn FnMut()| {
                if let Err(error) = catch_unwind(AssertUnwindSafe(f))
                    && first_panic.is_none()
                {
                    first_panic = Some(error);
                }
            };
            for effect_id in effects.iter().rev() {
                dispose(&mut || rt.dispose_effect(*effect_id));
            }
            for signal_id in signals {
                dispose(&mut || rt.dispose_signal(*signal_id));
            }
            for subscope_id in subscopes {
                dispose(&mut || {
                    let scope = rt.scopes.borrow_mut().remove(*subscope_id);
                    if let Some(scope) = scope {
                        scope.dispose();
                    }
                });
            }
            if let Some(error) = first_panic {
                resume_unwind(error);
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct Scope {
    id: ScopeId,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    #[track_caller]
    pub fn new() -> Self {
        #[cfg(debug_assertions)]
        let location = std::panic::Location::caller();
        Runtime::with(|rt| {
            let id = rt.scopes.borrow_mut().insert(ScopeInner::new(
                #[cfg(debug_assertions)]
                location,
            ));
            CURRENT_SCOPE.with_borrow_mut(|parent| {
                if !parent.is_null() {
                    parent.push_subscope(Self { id });
                }
            });
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

    pub(crate) fn is_disposed(&self) -> bool {
        !self.is_null() && Runtime::with(|rt| !rt.scopes.borrow().contains_key(self.id))
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
            let mut ancestor = *self;
            while !ancestor.is_null() {
                assert!(ancestor.id != subscope.id, "Subscope would create a cycle");
                ancestor = scopes[ancestor.id].parent;
            }
            if scopes[subscope.id].parent.id == self.id {
                return;
            }
            if !scopes[subscope.id].parent.is_null() {
                panic!("Subscope already has a parent");
            }
            scopes[subscope.id].parent = *self;
            scopes[self.id].subscopes.push(subscope.id);
        });
    }

    #[track_caller]
    pub fn run<R>(self, f: impl FnOnce() -> R) -> R {
        assert!(!self.is_disposed(), "Cannot run a disposed scope");

        struct RestoreScope(Scope);

        impl Drop for RestoreScope {
            fn drop(&mut self) {
                CURRENT_SCOPE.replace(self.0);
            }
        }

        let _restore = RestoreScope(CURRENT_SCOPE.replace(self));
        f()
    }

    pub fn provide<T: 'static>(&self, value: T) {
        if self.is_null() {
            return;
        }
        Runtime::with(|rt| {
            rt.scopes.borrow_mut()[self.id].context.insert::<T>(value);
        });
    }

    fn inject_with<T: 'static>(self, scopes: &SlotMap<ScopeId, ScopeInner>) -> Option<&T> {
        let mut scope = self;
        while !scope.is_null() {
            if let Some(context) = scopes[scope.id].context.get::<T>() {
                return Some(context);
            }
            scope = scopes[scope.id].parent;
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
        Runtime::with(|rt| {
            rt.batch(|| {
                let scope = rt.scopes.borrow_mut().remove(self.id);
                if let Some(scope) = scope {
                    scope.dispose();
                }
            });
        });
    }
}

pub fn current_scope() -> Scope {
    CURRENT_SCOPE.with_borrow(|s| *s)
}

pub fn create_scope(f: impl FnOnce()) -> Scope {
    let scope = Scope::new();
    scope.run(f);
    scope
}

#[cfg(test)]
mod test {
    use std::{
        cell::{Cell, RefCell},
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use super::{inject, provide, provide_with};
    use crate::{prelude::*, runtime::Runtime};

    #[test]
    fn test_scope_dispose_continues_after_cleanup_panics() {
        let trigger = signal(0).owned();
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let root = Scope::new();
        let register = |name, panics| {
            let cleanups = cleanups.clone();
            let trigger = *trigger;
            watch_effect(move || {
                trigger.get();
                let cleanups = cleanups.clone();
                on_cleanup(move || {
                    cleanups.borrow_mut().push(name);
                    if panics {
                        std::panic::panic_any(name);
                    }
                });
            });
        };
        let (child, grandchild, sibling, locals) = root.run(|| {
            let local = signal(1);
            register("root remaining", false);
            // Effects are disposed in reverse order, so this panic comes first.
            register("first cleanup panic", true);
            let child = Scope::new();
            let (child_local, grandchild, grandchild_local) = child.run(|| {
                register("child cleanup panic", true);
                let child_local = signal(2);
                let grandchild = Scope::new();
                let grandchild_local = grandchild.run(|| {
                    register("grandchild", false);
                    signal(3)
                });
                (child_local, grandchild, grandchild_local)
            });
            let sibling = Scope::new();
            let sibling_local = sibling.run(|| {
                register("sibling", false);
                signal(4)
            });
            (
                child,
                grandchild,
                sibling,
                [local, child_local, grandchild_local, sibling_local],
            )
        });
        let (effects, signals) = Runtime::with(|rt| {
            let scopes = rt.scopes.borrow();
            let owned = [root, child, grandchild, sibling];
            (
                owned
                    .iter()
                    .flat_map(|s| scopes[s.id].effects.iter().copied())
                    .collect::<Vec<_>>(),
                owned
                    .iter()
                    .flat_map(|s| scopes[s.id].signals.iter().copied())
                    .collect::<Vec<_>>(),
            )
        });

        let error = catch_unwind(AssertUnwindSafe(|| root.dispose()))
            .expect_err("scope must propagate the first cleanup panic");
        assert_eq!(error.downcast_ref::<&str>(), Some(&"first cleanup panic"));
        assert_eq!(
            *cleanups.borrow(),
            [
                "first cleanup panic",
                "root remaining",
                "child cleanup panic",
                "grandchild",
                "sibling"
            ]
        );
        for scope in [root, child, grandchild, sibling] {
            assert!(scope.is_disposed());
        }
        assert!(locals.iter().all(|signal| signal.is_disposed()));
        Runtime::with(|rt| {
            assert!(
                effects
                    .iter()
                    .all(|id| !rt.effects.borrow().contains_key(*id))
            );
            assert!(
                signals
                    .iter()
                    .all(|id| !rt.signals.borrow().contains_key(*id))
            );
        });
        trigger.set(1);
        assert_eq!(cleanups.borrow().len(), 5);
        root.dispose();
    }

    #[test]
    fn test_scope_dispose_batches_cleanup_writes_until_children_are_gone() {
        let trigger = signal(0).owned();
        let runs = Rc::new(Cell::new(0));
        let root = Scope::new();
        let child = root.run(|| {
            provide(String::from("root context"));
            let trigger = *trigger;
            watch_effect(move || {
                on_cleanup(move || trigger.set(1));
            });
            let child = Scope::new();
            child.run(|| {
                let runs = runs.clone();
                watch_effect(move || {
                    trigger.get();
                    // Re-running during teardown would traverse the removed root.
                    assert_eq!(inject::<String>().as_deref(), Some("root context"));
                    runs.set(runs.get() + 1);
                });
            });
            child
        });

        assert_eq!(runs.get(), 1);
        root.dispose();
        assert_eq!(trigger.get(), 1);
        assert_eq!(runs.get(), 1);
        assert!(child.is_disposed());
    }

    #[test]
    fn test_scope_run_restores_after_panic() {
        let previous = current_scope();
        let outer = Scope::new().owned();
        let inner = outer.run(Scope::new);

        outer.run(|| {
            let result = catch_unwind(AssertUnwindSafe(|| {
                inner.run(|| {
                    assert_eq!(current_scope().id, inner.id);
                    panic!("scope callback failed");
                });
            }));
            assert!(result.is_err());
            assert_eq!(current_scope().id, outer.id);

            // Newly created scopes must belong to the restored outer scope.
            let sibling = Scope::new();
            outer.provide(42_u32);
            assert_eq!(sibling.inject::<u32>(), Some(42));
        });
        assert_eq!(current_scope().id, previous.id);

        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                outer.run(|| panic!("outer callback failed"));
            }))
            .is_err()
        );
        assert_eq!(current_scope().id, previous.id);
    }

    #[test]
    fn test_scope_run_rejects_disposed_scope() {
        let outer = Scope::new().owned();
        let disposed = Scope::new();
        assert!(!disposed.is_disposed());
        disposed.dispose();
        assert!(disposed.is_disposed());

        let called = Cell::new(false);
        outer.run(|| {
            let result = catch_unwind(AssertUnwindSafe(|| {
                disposed.run(|| called.set(true));
            }));
            let panic = result.expect_err("disposed scope must panic");
            assert_eq!(
                panic.downcast_ref::<&str>().copied(),
                Some("Cannot run a disposed scope")
            );
            assert!(!called.get());
            assert_eq!(current_scope().id, outer.id);
        });
    }

    #[test]
    fn test_null_scope_can_run_and_restore() {
        let outer = Scope::new().owned();
        assert!(!Scope::null().is_disposed());
        outer.run(|| {
            assert_eq!(
                Scope::null().run(|| {
                    assert!(current_scope().is_null());
                    42
                }),
                42
            );
            assert_eq!(current_scope().id, outer.id);
        });
    }

    #[test]
    fn test_owned_scope_disposes_on_drop() {
        let count = signal(0).owned();
        let runs = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let scope = Scope::new();
        let local = scope.run(|| {
            let runs = runs.clone();
            let cleanups = cleanups.clone();
            let count = *count;
            watch_effect(move || {
                count.get();
                runs.set(runs.get() + 1);
                let cleanups = cleanups.clone();
                on_cleanup(move || cleanups.set(cleanups.get() + 1));
            });
            signal(10)
        });

        let owned = scope.owned();
        assert!(!local.is_disposed());
        assert_eq!(runs.get(), 1);
        drop(owned);
        assert!(scope.is_disposed());
        assert!(local.is_disposed());
        assert_eq!(cleanups.get(), 1);
        count.set(1);
        assert_eq!(runs.get(), 1);
        scope.dispose();
        assert_eq!(cleanups.get(), 1);
    }

    #[test]
    fn test_scope_cascades_to_live_children() {
        let count = signal(0).owned();
        let runs = Rc::new(Cell::new(0));
        let outer = Scope::new();
        let register_effect = || {
            let runs = runs.clone();
            let count = *count;
            watch_effect(move || {
                count.get();
                runs.set(runs.get() + 1);
            });
        };
        let (child, grandchild, local) = outer.run(|| {
            register_effect();
            let child = Scope::new();
            let (grandchild, local) = child.run(|| {
                register_effect();
                let grandchild = Scope::new();
                let local = grandchild.run(|| {
                    register_effect();
                    signal(10)
                });
                (grandchild, local)
            });
            (child, grandchild, local)
        });

        assert_eq!(runs.get(), 3);
        count.set(1);
        assert_eq!(runs.get(), 6);
        assert!(!child.is_disposed());
        assert!(!grandchild.is_disposed());
        outer.dispose();
        assert!(outer.is_disposed());
        assert!(child.is_disposed());
        assert!(grandchild.is_disposed());
        assert!(local.is_disposed());
        count.set(2);
        assert_eq!(runs.get(), 6);
        child.dispose();
        grandchild.dispose();
    }

    #[test]
    fn test_scope_context_inheritance_and_shadowing() {
        let outer = Scope::new().owned();
        outer.provide(String::from("outer"));
        outer.provide(10_usize);
        let child = outer.run(Scope::new);
        let sibling = outer.run(Scope::new);
        let grandchild = child.run(Scope::new);

        assert_eq!(grandchild.inject::<String>().as_deref(), Some("outer"));
        child.run(|| {
            assert_eq!(inject::<String>().as_deref(), Some("outer"));
            provide_with::<String>(|parent| format!("{}/child", parent.unwrap()));
            provide(20_usize);
        });
        grandchild.run(|| {
            assert_eq!(inject::<String>().as_deref(), Some("outer/child"));
            assert_eq!(inject::<usize>(), Some(20));
            provide_with::<usize>(|parent| parent.unwrap() + 1);
            assert_eq!(inject::<usize>(), Some(21));
            assert_eq!(inject::<bool>(), None);
        });
        assert_eq!(child.inject::<usize>(), Some(20));
        assert_eq!(sibling.inject::<usize>(), Some(10));
        assert_eq!(sibling.inject::<String>().as_deref(), Some("outer"));
        assert_eq!(outer.inject::<String>().as_deref(), Some("outer"));
        assert_eq!(Scope::null().inject::<String>(), None);
    }

    #[test]
    #[should_panic(expected = "Subscope would create a cycle")]
    fn test_scope_rejects_self_parent() {
        let scope = Scope::new().owned();
        scope.push_subscope(*scope);
    }

    #[test]
    #[should_panic(expected = "Subscope would create a cycle")]
    fn test_scope_rejects_ancestor_cycle() {
        let outer = Scope::new().owned();
        let child = outer.run(Scope::new);
        let grandchild = child.run(Scope::new);
        // Reattaching the same child is harmless, but reversing ancestry is not.
        outer.push_subscope(child);
        grandchild.push_subscope(*outer);
    }

    #[test]
    fn test_scope_disposes_on_drop() {
        let count = signal(0);
        let runs = Rc::new(RefCell::new(0));
        let runs_clone = runs.clone();

        let scope = create_scope(|| {
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
        scope.dispose();

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

        let scope_outer = create_scope(|| {
            watch_effect(move || {
                count.get();
                *outer_clone.borrow_mut() += 1;
            });

            let scope_inner = create_scope(|| {
                watch_effect(move || {
                    count.get();
                    *inner_clone.borrow_mut() += 1;
                });
            });
            // Inner effect runs
            assert_eq!(*inner_runs.borrow(), 1);
            scope_inner.dispose();

            // Update triggers only outer effect
            count.set(1);
            assert_eq!(*outer_runs.borrow(), 2);
            assert_eq!(*inner_runs.borrow(), 1);
        });
        scope_outer.dispose();

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

        let scope_outer = create_scope(|| {
            watch_effect(move || {
                count.get();
                *runs_clone.borrow_mut() += 1;
            });

            let scope_inner = create_scope({
                let runs_clone = runs.clone();
                || {
                    watch_effect(move || {
                        count.get();
                        *runs_clone.borrow_mut() += 1;
                    });
                }
            });
            // Both effects run
            assert_eq!(*runs.borrow(), 2);
            scope_inner.dispose();
        });
        scope_outer.dispose();

        count.set(1);
        // No effects should run
        assert_eq!(*runs.borrow(), 2);
    }
}
