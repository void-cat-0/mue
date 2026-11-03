//! Immediate-allocation computed reference, only for controlled unit benchmarks.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn enabled() -> bool {
    ENABLED.get()
}
pub(crate) fn run<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            ENABLED.set(self.0);
        }
    }
    let _restore = Restore(ENABLED.replace(true));
    f()
}
pub(super) fn bind<T: Interpolatable>(
    source: ReadSignal<T>,
    curve: Prop<Transition>,
) -> ReadSignal<T> {
    let initial = source.get_clone_untracked();
    let frame = signal(());
    let mut revision = source.revision_untracked();
    let tween = Rc::new(RefCell::new(Tween {
        from: initial.clone(),
        to: initial.clone(),
        elapsed: 0.,
        transition: None,
        value: initial,
        frame,
        registration: None,
    }));
    let driver: Rc<dyn Driver> = tween.clone();
    tween.borrow_mut().registration = Some(Rc::downgrade(&driver));
    register_driver(&driver);
    computed(move |_| {
        let destination = source.get_clone();
        let current = source.revision_untracked();
        frame.track();
        if revision != current {
            revision = current;
            let curve = curve.get_clone_untracked();
            let curve = (curve.duration().is_finite() && curve.duration() > 0.).then_some(curve);
            let mut tween = tween.borrow_mut();
            if tween.to != destination {
                tween.from = tween.value.clone();
                tween.to = destination.clone();
                tween.elapsed = 0.;
                tween.transition = curve.filter(|_| {
                    tween.from != destination && T::can_interpolate(&tween.from, &destination)
                });
                if tween.transition.is_none() {
                    tween.value = destination;
                }
            }
        }
        tween.borrow().value.clone()
    })
}
