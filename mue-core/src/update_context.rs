//! Metadata carried by reactive updates, including deferred and computed updates.
use std::{any::Any, cell::RefCell, rc::Rc};

thread_local! {
    static CURRENT: RefCell<UpdateContext> = RefCell::default();
}

/// A snapshot of update metadata. Values are looked up by their concrete type.
#[derive(Clone, Default)]
pub struct UpdateContext(Vec<Rc<dyn Any>>);

impl UpdateContext {
    /// Capture the metadata of the current reactive update.
    pub fn current() -> Self {
        CURRENT.with_borrow(Clone::clone)
    }

    /// Read the innermost value of type `T`.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.0.iter().rev().find_map(|value| value.downcast_ref())
    }

    /// Run a callback with this snapshot, restoring the previous context on unwind.
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        struct Restore(UpdateContext);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.set(std::mem::take(&mut self.0));
            }
        }
        let _restore = Restore(CURRENT.replace(self.clone()));
        f()
    }
}

/// Attach metadata to updates made by `f`. Nested values override the same type.
/// The runtime preserves this context through computed signals and batches.
pub fn with_update_context<T: 'static, R>(value: T, f: impl FnOnce() -> R) -> R {
    let mut context = UpdateContext::current();
    context.0.push(Rc::new(value));
    context.run(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn metadata_survives_deferred_computed_updates() {
        let state = signal(0);
        let derived = state.map(|n| n * 2).map(|n| n + 1);
        let observed = Rc::new(RefCell::new(None));
        watch(derived, {
            let observed = observed.clone();
            move |_| *observed.borrow_mut() = UpdateContext::current().get::<u32>().copied()
        });
        batch(|| with_update_context(42u32, || state.set(1)));
        assert_eq!(*observed.borrow(), Some(42));
        state.set(2);
        assert_eq!(*observed.borrow(), None);
    }

    #[test]
    fn nested_context_restores_metadata_after_unwind() {
        with_update_context(1u32, || {
            with_update_context("other type", || {
                assert_eq!(UpdateContext::current().get::<u32>(), Some(&1));
            });
            let result = std::panic::catch_unwind(|| {
                with_update_context(2u32, || panic!("test unwind"));
            });
            assert!(result.is_err());
            assert_eq!(UpdateContext::current().get::<u32>(), Some(&1));
        });
        assert!(UpdateContext::current().get::<u32>().is_none());
    }
}
