use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, OnceLock},
    task::{Context, Poll},
    thread::{self, JoinHandle},
};

use async_task::{Runnable, Task as AsyncTask};

/// A caller-owned task running on the background worker pool.
///
/// The future and its output are Send. Dropping the handle requests
/// cancellation; detach lets the executor run it without an observer.
/// A completed output is owned by the handle and dropped on the thread that
/// drops it. Polling after completion, or awaiting a task cancelled by pool
/// shutdown, follows async-task's panic behavior.
///
/// ```
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::BackgroundTask<u32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::BackgroundTask<std::rc::Rc<()>>>();
/// ```
#[must_use = "dropping a background task cancels it; use detach() to run without observing its output"]
pub struct BackgroundTask<T> {
    task: Option<AsyncTask<T>>,
}

impl<T> BackgroundTask<T> {
    /// Run to completion without observing the output.
    pub fn detach(mut self) {
        self.task
            .take()
            .expect("background task handle already consumed")
            .detach();
    }

    /// Whether the task has completed or been closed.
    pub fn is_finished(&self) -> bool {
        self.task
            .as_ref()
            .expect("background task handle already consumed")
            .is_finished()
    }
}

impl<T> Future for BackgroundTask<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        Pin::new(
            self.task
                .as_mut()
                .expect("background task handle already consumed"),
        )
        .poll(cx)
    }
}

/// A handle to a fixed-size background worker pool.
///
/// Cloning shares the pool. Dropping the last handle closes the queue and
/// joins the workers; queued tasks are cancelled, and tasks still held by a
/// BackgroundTask handle are destroyed when that handle is dropped. The global
/// pool used by spawn_background lives for the process lifetime.
#[derive(Clone)]
pub struct BackgroundExecutor {
    pool: Arc<Pool>,
}

impl BackgroundExecutor {
    /// Create a pool with at least one worker. The workers live until the last
    /// executor handle is dropped.
    pub fn new(worker_count: usize) -> Self {
        Self {
            pool: Arc::new(Pool::new(worker_count.max(1))),
        }
    }

    /// Spawn a Send future on this pool.
    pub fn spawn<F>(&self, future: F) -> BackgroundTask<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.pool.spawn(future)
    }
}

impl Default for BackgroundExecutor {
    fn default() -> Self {
        Self::new(default_worker_count())
    }
}

struct Pool {
    queue: Arc<Queue>,
    workers: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new(worker_count: usize) -> Self {
        let queue = Arc::new(Queue::new());
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let queue = queue.clone();
            let worker = thread::Builder::new()
                .name(format!("mue-background-{index}"))
                .spawn(move || {
                    super::IS_BACKGROUND_WORKER.with(|flag| flag.set(true));
                    while let Some(runnable) = queue.pop() {
                        runnable.run();
                    }
                })
                .expect("failed to spawn mue background worker");
            workers.push(worker);
        }
        Self { queue, workers }
    }

    fn spawn<F>(&self, future: F) -> BackgroundTask<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let queue = self.queue.clone();
        let (runnable, task) = async_task::Builder::new()
            // Keep the worker alive and rethrow the panic on await.
            .propagate_panic(true)
            .spawn(move |_| future, move |runnable| queue.push(runnable));
        runnable.schedule();
        BackgroundTask { task: Some(task) }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.queue.close();
        // Cancel queued work before joining so a blocked worker cannot delay
        // cleanup. This also breaks the queue -> runnable -> schedule closure ->
        // queue cycle for work queued after close or left by a dead worker.
        // The futures are Send, so dropping them on this thread is safe.
        let pending = {
            let mut state = self.queue.state.lock().unwrap();
            state.ready.drain(..).collect::<Vec<_>>()
        };
        drop(pending);
        let current = thread::current().id();
        for worker in self.workers.drain(..) {
            // A worker can hold the last executor handle. Joining itself would
            // deadlock; let that thread exit after its current task instead.
            if worker.thread().id() == current {
                continue;
            }
            let _ = worker.join();
        }
    }
}

struct Queue {
    state: Mutex<QueueState>,
    available: Condvar,
}

struct QueueState {
    ready: VecDeque<Runnable>,
    closing: bool,
}

impl Queue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                ready: VecDeque::new(),
                closing: false,
            }),
            available: Condvar::new(),
        }
    }

    fn push(&self, runnable: Runnable) {
        let mut state = self.state.lock().unwrap();
        if state.closing {
            drop(state);
            // The future is Send, so dropping it on the caller thread is safe.
            drop(runnable);
            return;
        }
        state.ready.push_back(runnable);
        self.available.notify_one();
    }

    fn pop(&self) -> Option<Runnable> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(runnable) = state.ready.pop_front() {
                return Some(runnable);
            }
            if state.closing {
                return None;
            }
            state = self.available.wait(state).unwrap();
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closing = true;
        self.available.notify_all();
    }
}

fn default_worker_count() -> usize {
    thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).clamp(1, 4))
        .unwrap_or(2)
}

static DEFAULT_EXECUTOR: OnceLock<BackgroundExecutor> = OnceLock::new();

fn default_executor() -> &'static BackgroundExecutor {
    DEFAULT_EXECUTOR.get_or_init(BackgroundExecutor::default)
}

/// Spawn a Send future on the global background pool.
///
/// The future is polled and destroyed on a worker thread. It must not access
/// reactive state, scopes, or UI-thread resources. Return owned Send data, then
/// publish it from a foreground task (or use AsyncResource::new_background).
/// Dropping the handle cancels the task; detach lets it run to completion.
pub fn spawn_background<F>(future: F) -> BackgroundTask<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    default_executor().spawn(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Access;
    use futures_channel::oneshot;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::mpsc,
        task::{Wake, Waker},
        time::{Duration, Instant},
    };

    fn block_on<T>(mut task: BackgroundTask<T>) -> T {
        struct Unpark(thread::Thread);
        impl Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.unpark();
            }
        }

        let waker = Waker::from(Arc::new(Unpark(thread::current())));
        let mut cx = Context::from_waker(&waker);
        loop {
            match Pin::new(&mut task).poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => thread::park(),
            }
        }
    }

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
                "timed out waiting for background task"
            );
            super::super::tick(8);
            thread::sleep(Duration::from_millis(1));
        }
    }

    struct DropSignal(Option<mpsc::Sender<thread::ThreadId>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(thread::current().id());
            }
        }
    }

    #[test]
    fn background_result_reaches_foreground_task() {
        let executor = BackgroundExecutor::new(1);
        let ui_thread = thread::current().id();
        let (started_tx, started_rx) = mpsc::channel();
        let background = executor.spawn(async move {
            assert_ne!(thread::current().id(), ui_thread);
            started_tx.send(()).unwrap();
            21u32 * 2
        });
        let mut publisher = super::super::spawn(async move { background.await });
        assert_eq!(super::super::tick(1), 1);
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        tick_until(|| publisher.is_finished());
        assert_eq!(poll_task(&mut publisher), Poll::Ready(42));
    }

    #[test]
    fn dropping_background_task_cancels_and_drops_future_on_worker() {
        let executor = BackgroundExecutor::new(1);
        let (started_tx, started_rx) = mpsc::channel();
        let (drop_tx, drop_rx) = mpsc::channel();
        let task = executor.spawn(async move {
            let _drop = DropSignal(Some(drop_tx));
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(task);
        let drop_thread = drop_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(drop_thread, thread::current().id());
    }

    #[test]
    fn detached_background_task_runs_to_completion() {
        let executor = BackgroundExecutor::new(1);
        let (tx, rx) = mpsc::channel();
        executor
            .spawn(async move {
                tx.send(9u32).unwrap();
            })
            .detach();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 9);
    }

    #[test]
    fn background_panic_propagates_on_await_and_pool_survives() {
        let executor = BackgroundExecutor::new(1);
        let bad = executor.spawn(async { panic!("background boom") });
        let result = catch_unwind(AssertUnwindSafe(|| block_on(bad)));
        assert!(result.is_err());
        let good = executor.spawn(async { 5u32 });
        assert_eq!(block_on(good), 5);
    }

    #[test]
    fn spawn_background_uses_default_pool() {
        assert_eq!(block_on(spawn_background(async { 7u32 })), 7);
    }

    #[test]
    fn background_task_can_spawn_nested_background_task() {
        let executor = BackgroundExecutor::new(2);
        let outer = executor.spawn(async {
            let inner = spawn_background(async { 3u32 });
            inner.await + 4
        });
        assert_eq!(block_on(outer), 7);
    }

    #[test]
    fn async_resource_background_publishes_state() {
        let resource = super::super::AsyncResource::<u32, ()>::new_background(async { Ok(42) });
        let state = resource.state();
        tick_until(|| state.get_clone_untracked() == super::super::AsyncState::Ready(Ok(42)));
        drop(resource);
        assert!(state.is_disposed());
    }

    #[test]
    fn async_resource_background_with_executor() {
        let executor = BackgroundExecutor::new(1);
        let resource =
            super::super::AsyncResource::<u32, ()>::new_background_with(&executor, async { Ok(7) });
        tick_until(|| {
            resource.state().get_clone_untracked() == super::super::AsyncState::Ready(Ok(7))
        });
        drop(resource);
        drop(executor);
    }

    #[test]
    fn async_resource_background_cancels_on_drop() {
        let (drop_tx, drop_rx) = mpsc::channel();
        let (sender, receiver) = oneshot::channel::<u32>();
        // Capture the probe before the future starts so cancellation before the
        // first poll still reports the future's destruction.
        let drop_signal = DropSignal(Some(drop_tx));
        let resource = super::super::AsyncResource::<u32, ()>::new_background(async move {
            let _drop = drop_signal;
            receiver.await.map_err(|_| ())
        });
        super::super::tick(1);
        drop(resource);
        drop_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(sender.send(42).is_err());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn reactive_access_on_background_worker_panics_in_debug() {
        let executor = BackgroundExecutor::new(1);
        let task = executor.spawn(async {
            let _ = crate::signal::signal(1u32);
        });
        let result = catch_unwind(AssertUnwindSafe(|| block_on(task)));
        assert!(result.is_err());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn foreground_spawn_on_background_worker_panics_in_debug() {
        let executor = BackgroundExecutor::new(1);
        let task = executor.spawn(async {
            let _ = super::super::spawn(async {});
        });
        let result = catch_unwind(AssertUnwindSafe(|| block_on(task)));
        assert!(result.is_err());
    }

    #[test]
    fn dropping_executor_drops_queued_detached_task_before_joining() {
        let executor = BackgroundExecutor::new(1);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = executor.spawn(async move {
            started_tx.send(()).unwrap();
            // Block the only worker so the next task stays queued.
            release_rx.recv().unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let (drop_tx, drop_rx) = mpsc::channel();
        let drop_signal = DropSignal(Some(drop_tx));
        executor
            .spawn(async move {
                let _drop = drop_signal;
                std::future::pending::<()>().await;
            })
            .detach();

        let dropping = thread::spawn(move || drop(executor));
        drop_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        dropping.join().unwrap();
        drop(blocker);
    }

    #[test]
    fn dropping_executor_stops_workers_and_cleans_up_on_handle_drop() {
        let executor = BackgroundExecutor::new(1);
        let (started_tx, started_rx) = mpsc::channel();
        let (drop_tx, drop_rx) = mpsc::channel();
        let task = executor.spawn(async move {
            let _drop = DropSignal(Some(drop_tx));
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(executor);
        drop(task);
        drop_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
