use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    thread,
};

static NEXT_BLOCKING_THREAD: AtomicU64 = AtomicU64::new(1);

struct BlockingInner<T> {
    result: Option<T>,
    waker: Option<Waker>,
    panicked: bool,
}

struct BlockingState<T> {
    inner: Mutex<BlockingInner<T>>,
    finished: AtomicBool,
}

/// A blocking closure running on its own OS thread.
///
/// Dropping the handle does not cancel or join the thread; the closure runs to
/// completion and its result is discarded. `.await` yields the closure's output.
/// If the closure panics, awaiting the handle panics on the awaiting thread.
///
/// ```
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::BlockingTask<u32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::BlockingTask<std::rc::Rc<()>>>();
/// ```
#[must_use = "dropping a blocking task detaches it; the closure still runs to completion"]
pub struct BlockingTask<T> {
    state: Arc<BlockingState<T>>,
}

impl<T> BlockingTask<T> {
    /// Whether the closure has finished, successfully or by panicking.
    pub fn is_finished(&self) -> bool {
        self.state.finished.load(Ordering::Acquire)
    }

    /// Stop observing the result. The closure still runs to completion on its thread.
    pub fn detach(self) {}
}

impl<T> fmt::Debug for BlockingTask<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingTask")
            .field("finished", &self.is_finished())
            .finish()
    }
}

impl<T> Future for BlockingTask<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        let mut inner = this.state.inner.lock().unwrap();
        if let Some(value) = inner.result.take() {
            return Poll::Ready(value);
        }
        if inner.panicked {
            panic!("blocking task panicked");
        }
        if this.state.finished.load(Ordering::Acquire) {
            panic!("blocking task polled after completion");
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// Run a blocking closure on a fresh OS thread and return a handle for its output.
///
/// Use this for infrequent blocking work such as file I/O or decoding. Each
/// call starts a new thread. Use the host's blocking pool if you need to bound
/// concurrency. The closure must avoid reactive APIs and UI-thread resources.
/// Dropping the handle discards the result while the closure keeps running.
///
/// ```
/// use mue_core::task::{block_on, spawn, spawn_blocking};
///
/// let worker = spawn_blocking(|| 21 * 2);
/// let publisher = spawn(async move { worker.await });
/// assert_eq!(block_on(publisher), 42);
/// ```
///
/// # Panics
///
/// Panics if the OS thread cannot be spawned.
pub fn spawn_blocking<F, T>(f: F) -> BlockingTask<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let state = Arc::new(BlockingState {
        inner: Mutex::new(BlockingInner {
            result: None,
            waker: None,
            panicked: false,
        }),
        finished: AtomicBool::new(false),
    });
    let task = BlockingTask {
        state: state.clone(),
    };
    let thread_state = state;
    let name = format!(
        "mue-blocking-{}",
        NEXT_BLOCKING_THREAD.fetch_add(1, Ordering::Relaxed)
    );
    thread::Builder::new()
        .name(name)
        .spawn(move || {
            super::IS_BACKGROUND_WORKER.with(|flag| flag.set(true));
            struct Guard<T>(Arc<BlockingState<T>>);
            impl<T> Drop for Guard<T> {
                fn drop(&mut self) {
                    if !self.0.finished.load(Ordering::Acquire) {
                        let waker = {
                            let mut inner = self.0.inner.lock().unwrap();
                            inner.panicked = true;
                            inner.waker.take()
                        };
                        self.0.finished.store(true, Ordering::Release);
                        if let Some(waker) = waker {
                            waker.wake();
                        }
                    }
                }
            }

            let guard = Guard(thread_state.clone());
            let value = f();
            let waker = {
                let mut inner = thread_state.inner.lock().unwrap();
                inner.result = Some(value);
                inner.waker.take()
            };
            thread_state.finished.store(true, Ordering::Release);
            drop(guard);
            if let Some(waker) = waker {
                waker.wake();
            }
        })
        .expect("failed to spawn mue blocking thread");
    task
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn poll_task<T>(task: &mut super::super::Task<T>) -> Poll<T> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        Pin::new(task).poll(&mut cx)
    }

    fn tick_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for blocking task"
            );
            super::super::tick(8);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn blocking_task_runs_off_thread_and_returns_value() {
        let ui_thread = thread::current().id();
        let (tx, rx) = mpsc::channel();
        let task = spawn_blocking(move || {
            tx.send(thread::current().id()).unwrap();
            42u32
        });
        let thread_id = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(thread_id, ui_thread);
        let mut publisher = super::super::spawn(async move { task.await });
        tick_until(|| publisher.is_finished());
        assert_eq!(poll_task(&mut publisher), Poll::Ready(42));
    }

    #[test]
    fn blocking_task_drop_does_not_join_the_thread() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let task = spawn_blocking(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            done_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(task);
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn blocking_task_panic_propagates_on_await() {
        let task = spawn_blocking(|| -> u32 { panic!("blocking boom") });
        let publisher = super::super::spawn(async move { task.await });
        let result = catch_unwind(AssertUnwindSafe(|| {
            tick_until(|| publisher.is_finished());
        }));
        assert!(result.is_err());
    }
}
