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

    /// Run a callback with this snapshot, restoring the previous context afterwards.
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        let previous = CURRENT.replace(self.clone());
        let value = f();
        CURRENT.set(previous);
        value
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
}
