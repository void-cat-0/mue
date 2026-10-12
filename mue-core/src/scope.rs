use std::cell::RefCell;

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

    pub fn run<R>(self, f: impl FnOnce() -> R) -> R {
        let prev_scope = CURRENT_SCOPE.replace(self);
        let result = f();
        CURRENT_SCOPE.replace(prev_scope);
        result
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
            let scope = rt.scopes.borrow_mut().remove(self.id);
            if let Some(scope) = scope {
                scope.dispose();
            }
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
    use std::{cell::RefCell, rc::Rc};

    use crate::prelude::*;

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
