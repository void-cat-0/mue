//! Previous always-notify policy, compiled only for comparison tests.
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
