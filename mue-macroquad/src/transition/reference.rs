//! Pre-optimization watch -> signal binding, compiled only for paired tests.
use super::*;
use std::cell::Cell;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

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

struct ReferenceTween<T> {
    from: T,
    to: T,
    elapsed: f32,
    transition: Option<Transition>,
    value: Signal<T>,
    _running: Signal<bool>,
    registration: Option<Weak<dyn Driver>>,
}

impl<T> Drop for ReferenceTween<T> {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            let _ = DRIVERS.try_with(|drivers| {
                if let Ok(mut drivers) = drivers.try_borrow_mut() {
                    drivers.retain(|driver| !Weak::ptr_eq(driver, &registration));
                }
            });
        }
    }
}

impl<T: Interpolatable> Driver for RefCell<ReferenceTween<T>> {
    fn advance(&self, dt: f32) {
        let mut tween = self.borrow_mut();
        if tween.value.is_disposed() {
            return;
        }
        let Some(transition) = tween.transition else {
            return;
        };
        tween.elapsed = (tween.elapsed + dt.max(0.)).min(transition.duration());
        let progress = tween.elapsed / transition.duration();
        let value = if progress >= 1. {
            tween.to.clone()
        } else {
            T::interpolate(&tween.from, &tween.to, transition.sample(progress))
        };
        if progress >= 1. {
            tween.transition = None;
        }
        let presentation = tween.value;
        drop(tween);
        presentation.set(value);
    }
}

pub(super) fn transition_value<T: Interpolatable>(
    source: ReadSignal<T>,
    curve: Prop<Transition>,
) -> ReadSignal<T> {
    let initial = source.get_clone_untracked();
    let value = signal(initial.clone());
    let running = signal(false);
    let tween = Rc::new(RefCell::new(ReferenceTween {
        from: initial.clone(),
        to: initial,
        elapsed: 0.,
        transition: None,
        value,
        _running: running,
        registration: None,
    }));
    let driver: Rc<dyn Driver> = tween.clone();
    tween.borrow_mut().registration = Some(Rc::downgrade(&driver));
    register_driver(&driver);
    mue_core::effect::watch(source, move |source| {
        let destination = source.get_clone_untracked();
        let curve = curve.get_clone_untracked();
        let curve = (curve.duration().is_finite() && curve.duration() > 0.).then_some(curve);
        let mut tween = tween.borrow_mut();
        if tween.to == destination {
            return;
        }
        tween.from = value.get_clone_untracked();
        tween.to = destination.clone();
        tween.elapsed = 0.;
        tween.transition = curve
            .filter(|_| tween.from != destination && T::can_interpolate(&tween.from, &destination));
        let is_running = tween.transition.is_some();
        drop(tween);
        if !is_running {
            value.set(destination);
        }
    });
    *value
}
