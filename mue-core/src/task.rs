use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    thread::{self, ThreadId},
};

use async_task::{Runnable, Task as AsyncTask};

use crate::{
    Disposable,
    runtime::Runtime,
    scope::{OwnedScope, Scope, current_scope},
    signal::{ReadSignal, Signal},
};

type LocalRunnable = Runnable<u64>;

static NEXT_TASK_KEY: AtomicU64 = AtomicU64::new(1);

fn next_task_key() -> u64 {
    let mut key = NEXT_TASK_KEY.load(Ordering::Relaxed);
    loop {
        let next = key
            .checked_add(1)
            .expect("foreground task key space exhausted");
        match NEXT_TASK_KEY.compare_exchange_weak(key, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return key,
            Err(current) => key = current,
        }
    }
}

// Identity and cleanup state are shared; reactive scope ownership is opt-in.
// No TaskId is interpreted through the caller's thread-local executor.
struct TaskToken {
    key: u64,
    owner_thread: ThreadId,
    queue: Arc<ReadyQueue>,
    finished: AtomicBool,
}

impl TaskToken {
    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn retire(self: &Arc<Self>) {
        // During TLS destruction Executor::drop removes registrations itself.
        let registration =
            EXECUTOR.try_with(|executor| executor.borrow_mut().tasks.remove(&self.key));
        // Dropping a waker may schedule work. Do it outside the executor borrow.
        drop(registration);
    }

    fn clean_cancelled(self: &Arc<Self>) {
        if thread::current().id() != self.owner_thread {
            return;
        }
        // async_task::Task::drop closed this task before scheduling it for cleanup.
        // Dropping its queued Runnable destroys the future on its original thread.
        let runnable = self.queue.take_key(self.key);
        drop(runnable);
    }
}

struct QueueState {
    ready: VecDeque<LocalRunnable>,
    closing: bool,
}

struct ReadyQueue {
    state: Mutex<QueueState>,
    available: Condvar,
    #[cfg(test)]
    enqueue_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl ReadyQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                ready: VecDeque::new(),
                closing: false,
            }),
            available: Condvar::new(),
            #[cfg(test)]
            enqueue_hook: Mutex::new(None),
        }
    }

    fn schedule(&self, runnable: LocalRunnable) -> bool {
        #[cfg(test)]
        {
            let hook = self.enqueue_hook.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        let mut state = self.state.lock().unwrap();
        state.ready.push_back(runnable);
        self.available.notify_one();
        !state.closing
    }

    fn take_key(&self, key: u64) -> Option<LocalRunnable> {
        let mut state = self.state.lock().unwrap();
        let position = state.ready.iter().position(|r| *r.metadata() == key)?;
        state.ready.remove(position)
    }
}

struct Registration {
    token: Arc<TaskToken>,
    // A sleeping future needs a wake source for shutdown. Keep this outside the
    // token so its allocation can be released when the registration is retired.
    waker: Waker,
}

struct Executor {
    tasks: HashMap<u64, Registration>,
    queue: Arc<ReadyQueue>,
    in_tick: bool,
}

impl Executor {
    fn new() -> Self {
        // tick() can initialize the executor before the first spawn. Keep reactive TLS
        // alive through executor teardown in case future destructors access it.
        Runtime::with(|_| {});
        current_scope();
        Self {
            tasks: HashMap::new(),
            queue: Arc::new(ReadyQueue::new()),
            in_tick: false,
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        {
            let mut state = self.queue.state.lock().unwrap();
            state.closing = true;
            self.queue.available.notify_all();
        }
        // Wake sleeping tasks, including detached futures without their own wake source.
        // Every Runnable is destroyed here on the foreground thread, never in schedule().
        for registration in self.tasks.values() {
            if !registration.token.is_finished() {
                registration.waker.wake_by_ref();
            }
        }
        loop {
            self.tasks
                .retain(|_, registration| !registration.token.is_finished());
            let runnable = {
                let mut state = self.queue.state.lock().unwrap();
                // A remote wake sets async-task's SCHEDULED bit before enqueueing.
                // Queue emptiness alone therefore cannot prove shutdown is complete.
                while state.ready.is_empty() && !self.tasks.is_empty() {
                    state = self.queue.available.wait(state).unwrap();
                }
                state.ready.pop_front()
            };
            match runnable {
                Some(runnable) => drop(runnable),
                None => break,
            }
        }
    }
}

thread_local! {
    static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
    static WAKE_HOST: RefCell<Option<Arc<dyn Fn() + Send + Sync>>> = RefCell::new(None);
}

struct TaskFuture<F> {
    future: Option<Pin<Box<F>>>,
    token: Arc<TaskToken>,
}

impl<F: Future> Future for TaskFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let future = this
            .future
            .as_mut()
            .expect("task future polled after completion");
        Runtime::with(|rt| rt.without_effect(|| Scope::null().run(|| future.as_mut().poll(cx))))
    }
}

impl<F> Drop for TaskFuture<F> {
    fn drop(&mut self) {
        Runtime::with(|rt| rt.without_effect(|| Scope::null().run(|| drop(self.future.take()))));
        self.token.finished.store(true, Ordering::Release);
        self.token.retire();
    }
}

/// Install the host callback used to request another frame after a task wake.
pub fn set_wake_callback(callback: Option<Arc<dyn Fn() + Send + Sync>>) {
    WAKE_HOST.set(callback);
}

/// A caller-owned task yielding T directly. Dropping the handle requests cancellation;
/// detach lets the executor run it without an observer. The creator scope does not own it.
///
/// The handle is Send when T is Send, even for a foreground non-Send future. Moving
/// the handle never moves polling or future destruction away from the foreground thread.
/// Cross-thread cancellation is cleaned up by that thread's next tick or shutdown.
/// Completed output is owned by the handle and may be consumed/dropped on its thread.
/// Polling after completion, or awaiting a task stopped by executor shutdown, panics.
///
/// ```
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::Task<u32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::Task<std::rc::Rc<()>>>();
/// ```
///
#[must_use = "dropping a task cancels it; use detach() to run without observing its output"]
pub struct Task<T> {
    task: Option<AsyncTask<T, u64>>,
    token: Arc<TaskToken>,
}

impl<T> Task<T> {
    pub fn detach(mut self) {
        self.task
            .take()
            .expect("task handle already consumed")
            .detach();
    }

    /// Whether the future has been destroyed on its owner thread.
    /// This does not consume or dispose the output held by the result handle.
    pub fn is_finished(&self) -> bool {
        self.token.is_finished()
    }
}

impl<T: 'static, E: 'static> Task<Result<T, E>> {
    /// Detach while explicitly handling a business error once, on the foreground thread.
    #[track_caller]
    pub fn detach_and_report(
        self,
        report: impl FnOnce(E, &'static std::panic::Location<'static>) + 'static,
    ) {
        let location = std::panic::Location::caller();
        spawn(async move {
            if let Err(error) = self.await {
                report(error, location);
            }
        })
        .detach();
    }
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        Pin::new(self.task.as_mut().expect("task handle already consumed")).poll(cx)
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            // async-task closes its own allocation; no lookup in the dropping thread's TLS.
            drop(task);
            self.token.clean_cancelled();
        }
    }
}

/// Spawn a non-Send foreground future without implicitly inheriting reactive scope.
/// Its handle owns the task and awaits to its output. Each poll and future destructor
/// runs without an ambient scope or effect tracker, restoring both on normal return.
/// Captured signals keep their existing owner. For new task-owned reactive resources
/// or inherited context, use [`crate::scope::scope_future`]; otherwise dispose new
/// resources explicitly or provide an owner.
pub fn spawn<F: Future + 'static>(future: F) -> Task<F::Output>
where
    F::Output: 'static,
{
    let queue = EXECUTOR.with_borrow(|executor| executor.queue.clone());
    let token = Arc::new(TaskToken {
        key: next_task_key(),
        owner_thread: thread::current().id(),
        queue: queue.clone(),
        finished: AtomicBool::new(false),
    });
    let wrapped = TaskFuture {
        future: Some(Box::pin(future)),
        token: token.clone(),
    };
    let wake_callback = WAKE_HOST.with_borrow(Clone::clone);
    let (runnable, task) = async_task::Builder::new()
        .metadata(token.key)
        .propagate_panic(false)
        .spawn_local(
            move |_| wrapped,
            move |runnable| {
                if queue.schedule(runnable) {
                    if let Some(callback) = &wake_callback {
                        callback();
                    }
                }
            },
        );
    let registration = Registration {
        token: token.clone(),
        waker: runnable.waker(),
    };
    EXECUTOR.with_borrow_mut(|executor| {
        executor.tasks.insert(token.key, registration);
    });
    runnable.schedule();
    Task {
        task: Some(task),
        token,
    }
}

/// Poll at most limit tasks ready at the start of this tick. Wakes during polling
/// are deferred to the next tick. One poll cannot be preempted by this budget.
pub fn tick(limit: usize) -> usize {
    let snapshot = EXECUTOR.with_borrow_mut(|executor| {
        if executor.in_tick {
            return None;
        }
        executor.in_tick = true;
        let queue = executor.queue.clone();
        let ready = queue
            .state
            .lock()
            .unwrap()
            .ready
            .iter()
            .take(limit)
            .map(|r| *r.metadata())
            .collect::<Vec<_>>();
        Some((queue, ready))
    });
    let Some((queue, ready)) = snapshot else {
        return 0;
    };
    let mut ran = 0;
    for key in ready {
        let Some(runnable) = queue.take_key(key) else {
            continue;
        };
        runnable.run();
        ran += 1;
    }
    EXECUTOR.with_borrow_mut(|executor| executor.in_tick = false);
    ran
}

/// A renderable result state for one asynchronous operation. Dropping the
/// resource cancels its task and disposes its state signal.
#[derive(Clone, Debug, PartialEq)]
pub enum AsyncState<T, E> {
    Loading,
    Ready(Result<T, E>),
}

pub struct AsyncResource<T: 'static, E: 'static> {
    state: Signal<AsyncState<T, E>>,
    state_owner: Option<OwnedScope>,
    task: Option<Task<()>>,
}

impl<T: 'static, E: 'static> AsyncResource<T, E> {
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let state_owner = OwnedScope::new_inherited();
        let state = state_owner.run(|| Signal::new_always(AsyncState::Loading));
        let task_state = state;
        let task = spawn(async move {
            let output = future.await;
            let _ = task_state.try_set(AsyncState::Ready(output));
        });
        Self {
            state,
            state_owner: Some(state_owner),
            task: Some(task),
        }
    }

    pub fn state(&self) -> ReadSignal<AsyncState<T, E>> {
        *self.state
    }
}

impl<T: 'static, E: 'static> Drop for AsyncResource<T, E> {
    fn drop(&mut self) {
        self.task.take();
        self.state.dispose();
        self.state_owner.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prelude::{Access, computed, on_cleanup, signal, watch_effect},
        runtime::resource_counts,
        scope::{OwnedScope, current_scope, scope_future},
    };
    use futures_channel::oneshot;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::Waker,
    };

    fn poll_task<T>(task: &mut Task<T>) -> Poll<T> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        Pin::new(task).poll(&mut cx)
    }

    #[test]
    fn tick_obeys_poll_limit_and_task_returns_value() {
        let owner = OwnedScope::new();
        let count = Rc::new(Cell::new(0));
        let mut tasks = Vec::new();
        owner.run(|| {
            for _ in 0..3 {
                let count = count.clone();
                tasks.push(spawn(async move {
                    count.set(count.get() + 1);
                    count.get()
                }));
            }
        });
        assert_eq!(tick(2), 2);
        assert_eq!(count.get(), 2);
        assert_eq!(tick(1), 1);
        assert_eq!(count.get(), 3);
        let outputs = tasks
            .iter_mut()
            .map(|task| poll_task(task))
            .collect::<Vec<_>>();
        assert_eq!(
            outputs,
            vec![Poll::Ready(1), Poll::Ready(2), Poll::Ready(3)]
        );
        drop(tasks);
        drop(owner);
    }

    #[test]
    fn dropping_handle_cancels_before_next_tick() {
        let owner = OwnedScope::new();
        let task = owner.run(|| spawn(std::future::pending::<()>()));
        assert_eq!(tick(1), 1);
        drop(task);
        assert_eq!(tick(8), 0);
        drop(owner);
    }

    #[test]
    fn scope_disposal_runs_attached_child_cleanup_before_parent_signals_drop() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let scope = owner.scope();
        let parent_source = owner.run(|| signal(11u32));
        let child = owner.run(Scope::new_attached);
        let child_source = child.run(|| signal(17u32));
        let reads = Rc::new(Cell::new(0));
        owner.run(|| {
            let reads = reads.clone();
            watch_effect(move || {
                let reads = reads.clone();
                on_cleanup(move || {
                    assert_eq!(child_source.get(), 17);
                    reads.set(reads.get() + 1);
                });
            });
        });
        child.run(|| {
            let reads = reads.clone();
            watch_effect(move || {
                parent_source.get();
                let reads = reads.clone();
                on_cleanup(move || {
                    assert_eq!(parent_source.get(), 11);
                    assert_eq!(child_source.get(), 17);
                    reads.set(reads.get() + 1);
                });
            });
        });
        scope.dispose();
        scope.dispose();
        drop(owner);
        assert_eq!(reads.get(), 2);
        assert!(parent_source.is_disposed());
        assert!(child_source.is_disposed());
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scope_context_outlives_signal_destructors_and_child_cleanup() {
        struct ContextValue(Rc<Cell<bool>>);
        impl Drop for ContextValue {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        struct SignalValue {
            context_dropped: Rc<Cell<bool>>,
            dropped: Rc<Cell<bool>>,
        }
        impl Drop for SignalValue {
            fn drop(&mut self) {
                assert!(!self.context_dropped.get());
                self.dropped.set(true);
            }
        }
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let context_dropped = Rc::new(Cell::new(false));
        let signal_dropped = Rc::new(Cell::new(false));
        owner.provide(Rc::new(ContextValue(context_dropped.clone())));
        owner.run(|| {
            Signal::new_always(SignalValue {
                context_dropped: context_dropped.clone(),
                dropped: signal_dropped.clone(),
            });
            Scope::new_attached().run(|| {
                watch_effect(|| {
                    on_cleanup(|| {
                        // Lookup uses the parent while all teardown scopes still exist.
                        let value = current_scope().inject::<Rc<ContextValue>>().unwrap();
                        assert!(!value.0.get());
                    })
                })
            });
        });
        drop(owner);
        assert!(signal_dropped.get());
        assert!(context_dropped.get());
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scope_disposal_runs_computed_cleanup_before_source_signal_drop() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let cleanup_reads = Rc::new(Cell::new(0));
        owner.run(|| {
            let source = signal(5u32);
            let cleanup_reads = cleanup_reads.clone();
            let _derived = computed(move |_| {
                on_cleanup({
                    let cleanup_reads = cleanup_reads.clone();
                    move || {
                        assert_eq!(source.get(), 5);
                        cleanup_reads.set(cleanup_reads.get() + 1);
                    }
                });
                source.get() * 2
            });
        });
        drop(owner);
        assert_eq!(cleanup_reads.get(), 1);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scope_cleanup_can_read_dirty_computed_and_drains_its_new_cleanup() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let cleanup_reads = Rc::new(Cell::new(0));
        let computed_cleanups = Rc::new(Cell::new(0));
        let source = owner.run(|| {
            let source = signal(3u32);
            let cleanups = computed_cleanups.clone();
            let derived = computed(move |_| {
                let cleanups = cleanups.clone();
                on_cleanup(move || {
                    assert_eq!(source.get(), 4);
                    cleanups.set(cleanups.get() + 1);
                });
                source.get() * 2
            });
            let reads = cleanup_reads.clone();
            watch_effect(move || {
                let reads = reads.clone();
                on_cleanup(move || {
                    assert_eq!(derived.get(), 8);
                    reads.set(reads.get() + 1);
                });
            });
            source
        });
        crate::batch(|| {
            source.set(4);
            assert_eq!(computed_cleanups.get(), 0);
            drop(owner);
        });
        assert_eq!(cleanup_reads.get(), 1);
        assert_eq!(computed_cleanups.get(), 2);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn cleanup_creation_in_disposing_scope_is_rejected_before_registration() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let source = signal(9u32);
            watch_effect(move || {
                on_cleanup(move || {
                    use std::panic::{AssertUnwindSafe, catch_unwind};
                    // The attempted factories panic before installing anything; the
                    // outer disposal itself still returns normally.
                    let before = resource_counts();
                    assert!(catch_unwind(AssertUnwindSafe(|| signal(1u32))).is_err());
                    assert!(catch_unwind(AssertUnwindSafe(|| computed(|_| 1u32))).is_err());
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| crate::effect::watch(*source, |_| {})))
                            .is_err()
                    );
                    assert!(catch_unwind(AssertUnwindSafe(|| watch_effect(|| {}))).is_err());
                    assert!(catch_unwind(AssertUnwindSafe(Scope::new_attached)).is_err());
                    assert_eq!(resource_counts(), before);
                })
            });
        });
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn plain_spawn_does_not_inherit_or_own_reactive_scope() {
        let baseline = resource_counts();
        let creator = OwnedScope::new();
        creator.provide(99u32);
        let driver = OwnedScope::new();
        driver.provide(42u32);
        let before_spawn = resource_counts();
        let (sender, receiver) = oneshot::channel::<()>();
        let mut task = creator.run(|| {
            spawn(async move {
                assert!(current_scope().is_null());
                assert_eq!(current_scope().inject::<u32>(), None);
                let local = Signal::new(41);
                assert_eq!(local.get(), 41);
                local.dispose(); // Plain tasks do not supply an owner for new resources.
                receiver.await.unwrap();
                assert!(current_scope().is_null());
            })
        });
        assert_eq!(resource_counts(), before_spawn);
        driver.run(|| {
            assert_eq!(tick(1), 1);
            assert_eq!(current_scope().inject::<u32>(), Some(42));
        });
        drop(creator);
        sender.send(()).unwrap();
        assert_eq!(tick(1), 1);
        assert_eq!(poll_task(&mut task), Poll::Ready(()));
        drop(driver);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn task_poll_and_drop_do_not_inherit_the_driving_effect() {
        use crate::effect::{on_cleanup, watch_effect};

        struct Probe {
            source: Signal<u32>,
            unexpected_cleanups: Rc<Cell<usize>>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                assert!(current_scope().is_null());
                self.source.get();
                let calls = self.unexpected_cleanups.clone();
                on_cleanup(move || calls.set(calls.get() + 1));
            }
        }
        let owner = OwnedScope::new();
        owner.provide(7u32);
        let (source, trigger) = owner.run(|| (Signal::new(0u32), Signal::new(0u32)));
        let runs = Rc::new(Cell::new(0));
        let unexpected_cleanups = Rc::new(Cell::new(0));
        let probe = Probe {
            source,
            unexpected_cleanups: unexpected_cleanups.clone(),
        };
        let calls = unexpected_cleanups.clone();
        let task = spawn(async move {
            let _probe = probe;
            assert!(current_scope().is_null());
            source.get();
            on_cleanup(move || calls.set(calls.get() + 1));
            std::future::pending::<()>().await;
        });
        let mut task = Some(task);
        let effect_runs = runs.clone();
        owner.run(|| {
            watch_effect(move || {
                trigger.get();
                effect_runs.set(effect_runs.get() + 1);
                tick(1);
                drop(task.take());
                assert_eq!(current_scope().inject::<u32>(), Some(7));
                trigger.get();
            })
        });
        source.set(1);
        assert_eq!(runs.get(), 1); // Task reads must not subscribe the driving effect.
        owner.run(|| trigger.set(1));
        assert_eq!(runs.get(), 2); // The outer tracker and scope are restored after poll/drop.
        drop(owner);
        assert_eq!(unexpected_cleanups.get(), 0);
    }

    #[test]
    fn scoped_future_disposal_runs_attached_child_cleanup_before_local_signal_drop() {
        use crate::runtime::resource_counts;

        let baseline = resource_counts();
        let reads = Rc::new(Cell::new(0));
        let reads_in_future = reads.clone();
        let task = spawn(scope_future(async move {
            let source = signal(23u32);
            let child = Scope::new_attached();
            child.run(|| {
                watch_effect(move || {
                    source.get();
                    let reads_in_future = reads_in_future.clone();
                    on_cleanup(move || {
                        assert_eq!(source.get(), 23);
                        reads_in_future.set(reads_in_future.get() + 1);
                    });
                })
            });
            std::future::pending::<()>().await;
        }));
        assert_eq!(tick(1), 1);
        drop(task);
        assert_eq!(reads.get(), 1);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scoped_future_ready_runs_child_cleanup_before_owned_signal_drop() {
        let baseline = resource_counts();
        let reads = Rc::new(Cell::new(0));
        let reads_in_future = reads.clone();
        let mut task = spawn(scope_future(async move {
            let source = signal(29u32);
            let child = Scope::new_attached();
            child.run(|| {
                watch_effect(move || {
                    source.get();
                    let reads = reads_in_future.clone();
                    on_cleanup(move || {
                        assert_eq!(source.get(), 29);
                        reads.set(reads.get() + 1);
                    });
                })
            });
        }));
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(()));
        assert_eq!(reads.get(), 1);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scoped_effect_updates_while_task_is_pending_and_stops_on_completion() {
        use crate::effect::watch_effect;

        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let source = owner.run(|| Signal::new(1u32));
        let observed = Rc::new(Cell::new(0));
        let observer = observed.clone();
        let (sender, receiver) = oneshot::channel::<()>();
        let mut task = spawn(scope_future(async move {
            watch_effect(move || observer.set(source.get()));
            receiver.await.unwrap();
        }));
        assert_eq!(tick(1), 1);
        source.set(2);
        assert_eq!(observed.get(), 2);
        assert!(!task.is_finished());
        sender.send(()).unwrap();
        assert_eq!(tick(1), 1);
        assert_eq!(poll_task(&mut task), Poll::Ready(()));
        source.set(3);
        assert_eq!(observed.get(), 2);
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn async_resource_does_not_implicitly_scope_its_operation() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        owner.provide(99u32);
        let before_resource = resource_counts();
        let resource = owner.run(|| {
            AsyncResource::<u32, ()>::new(async {
                assert!(current_scope().is_null());
                assert_eq!(current_scope().inject::<u32>(), None);
                Ok(42)
            })
        });
        assert_eq!(resource_counts().scopes, before_resource.scopes + 1);
        assert_eq!(tick(1), 1);
        assert_eq!(resource.state().get_clone(), AsyncState::Ready(Ok(42)));
        drop(resource);
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn async_resource_accepts_an_explicitly_scoped_operation() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let local_observer = Rc::new(Cell::new(None::<ReadSignal<u32>>));
        let observer = local_observer.clone();
        let resource = owner.run(|| {
            AsyncResource::<u32, ()>::new(scope_future(async move {
                let local = Signal::new(7u32);
                observer.set(Some(*local));
                Ok(local.get() * 6)
            }))
        });
        assert_eq!(tick(1), 1);
        assert!(local_observer.get().unwrap().is_disposed());
        assert_eq!(resource.state().get_clone(), AsyncState::Ready(Ok(42)));
        assert_eq!(resource_counts().signals, baseline.signals + 1);
        drop(resource);
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn scope_owned_handle_still_cancels_through_drop() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let (sender, receiver) = oneshot::channel::<()>();
        owner.run(|| {
            let task = spawn(async move {
                let _ = receiver.await;
            });
            let _task_owner = Signal::new_always(task);
        });
        assert_eq!(tick(1), 1);
        drop(owner);
        assert!(sender.send(()).is_err());
        assert_eq!(tick(1), 0);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn task_can_finish_without_publishing_to_disposed_signal() {
        let owner = OwnedScope::new();
        let target = owner.run(|| Signal::new(0u32));
        let (sender, receiver) = oneshot::channel::<u32>();
        let mut task = owner.run(|| {
            spawn(async move {
                let value = receiver.await.unwrap();
                assert_eq!(target.try_get(), None);
                target.try_set(value)
            })
        });
        assert_eq!(tick(1), 1);
        drop(owner);
        assert!(!task.is_finished());
        sender.send(42).unwrap();
        assert_eq!(tick(1), 1);
        assert_eq!(poll_task(&mut task), Poll::Ready(Err(42)));
    }

    #[test]
    fn dropping_handle_during_poll_defers_future_drop() {
        struct DropProbe {
            polling: Rc<Cell<bool>>,
            dropped_during_poll: Rc<Cell<bool>>,
        }
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.dropped_during_poll.set(self.polling.get());
            }
        }
        let owner = OwnedScope::new();
        let task_slot = Rc::new(RefCell::new(None::<Task<()>>));
        let polling = Rc::new(Cell::new(false));
        let dropped_during_poll = Rc::new(Cell::new(false));
        let polls = Rc::new(Cell::new(0));
        let dropped_during_poll_probe = dropped_during_poll.clone();
        owner.run(|| {
            let polling = polling.clone();
            let polls = polls.clone();
            let task_slot_for_poll = task_slot.clone();
            let task = spawn(async move {
                let _probe = DropProbe {
                    polling: polling.clone(),
                    dropped_during_poll: dropped_during_poll_probe.clone(),
                };
                polls.set(polls.get() + 1);
                std::future::poll_fn(move |_cx| {
                    polling.set(true);
                    task_slot_for_poll.borrow_mut().take();
                    polling.set(false);
                    Poll::<()>::Pending
                })
                .await;
            });
            *task_slot.borrow_mut() = Some(task);
        });
        assert_eq!(tick(1), 1);
        assert_eq!(polls.get(), 1);
        assert!(!dropped_during_poll.get());
        assert_eq!(tick(4), 0);
        task_slot.borrow_mut().take();
        drop(owner);
    }

    #[test]
    fn async_resource_exposes_result_state() {
        let owner = OwnedScope::new();
        let resource =
            owner.run(|| AsyncResource::<u32, &'static str>::new(async { Err("failed") }));
        assert_eq!(resource.state().get_clone_untracked(), AsyncState::Loading);
        tick(1);
        assert_eq!(
            resource.state().get_clone_untracked(),
            AsyncState::Ready(Err("failed"))
        );
        drop(resource);
        drop(owner);
    }

    #[test]
    fn wakes_are_deduplicated_by_async_task() {
        let owner = OwnedScope::new();
        let waker_slot = Rc::new(RefCell::new(None));
        let task = owner.run(|| {
            let waker_slot = waker_slot.clone();
            spawn(std::future::poll_fn(move |cx| {
                *waker_slot.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            }))
        });
        assert_eq!(tick(1), 1);
        let waker = waker_slot.borrow().as_ref().unwrap().clone();
        waker.wake_by_ref();
        waker.wake_by_ref();
        assert_eq!(tick(1), 1);
        drop(task);
        drop(owner);
    }

    #[test]
    fn wake_callback_is_called_when_a_task_is_scheduled() {
        let calls = Arc::new(AtomicUsize::new(0));
        set_wake_callback(Some({
            let calls = calls.clone();
            Arc::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
            })
        }));
        let owner = OwnedScope::new();
        let task = owner.run(|| spawn(std::future::pending::<()>()));
        assert!(calls.load(Ordering::SeqCst) > 0);
        drop(task);
        drop(owner);
        set_wake_callback(None);
    }

    #[test]
    fn cross_thread_wake_uses_spawn_thread_callback() {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        let callbacks = Arc::new(AtomicUsize::new(0));
        set_wake_callback(Some({
            let callbacks = callbacks.clone();
            Arc::new(move || {
                callbacks.fetch_add(1, Ordering::SeqCst);
            })
        }));
        let owner = OwnedScope::new();
        let waker = Rc::new(RefCell::new(None));
        let task = owner.run(|| {
            let waker = waker.clone();
            spawn(std::future::poll_fn(move |cx| {
                *waker.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            }))
        });
        tick(1);
        let waker = waker.borrow_mut().take().unwrap();
        let before = callbacks.load(Ordering::SeqCst);
        std::thread::spawn(move || {
            waker.wake();
            tx.send(()).unwrap();
        })
        .join()
        .unwrap();
        rx.recv().unwrap();
        assert_eq!(callbacks.load(Ordering::SeqCst), before + 1);
        drop(task);
        drop(owner);
        set_wake_callback(None);
    }

    #[test]
    fn self_wakes_are_deferred_to_next_tick() {
        let owner = OwnedScope::new();
        let polls = Rc::new(Cell::new(0));
        let task = owner.run(|| {
            let polls = polls.clone();
            spawn(std::future::poll_fn(move |cx| {
                polls.set(polls.get() + 1);
                cx.waker().wake_by_ref();
                Poll::<()>::Pending
            }))
        });
        assert_eq!(tick(16), 1);
        assert_eq!(polls.get(), 1);
        assert_eq!(tick(16), 1);
        assert_eq!(polls.get(), 2);
        drop(task);
        drop(owner);
    }

    #[test]
    fn cancelled_resource_does_not_publish_after_disposal() {
        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let (sender, receiver) = oneshot::channel();
        let resource = owner
            .run(|| AsyncResource::<u32, ()>::new(async move { receiver.await.map_err(|_| ()) }));
        let state = resource.state();
        tick(1);
        drop(resource);
        assert!(sender.send(42).is_err());
        assert_eq!(tick(8), 0);
        assert!(state.is_disposed());
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn resource_disposed_during_poll_does_not_publish() {
        let slot = Rc::new(RefCell::new(None::<AsyncResource<u32, ()>>));
        let future_slot = slot.clone();
        let resource = AsyncResource::new(async move {
            future_slot.borrow_mut().take();
            Ok(42)
        });
        let state = resource.state();
        *slot.borrow_mut() = Some(resource);
        assert_eq!(tick(1), 1);
        assert!(state.is_disposed());
        assert_eq!(tick(8), 0);
    }

    #[test]
    fn detach_keeps_task_alive_and_poll_restores_scope() {
        let parent = OwnedScope::new();
        parent.provide(81u32);
        let inner = Rc::new(Cell::new(None::<u32>));
        parent.run(|| {
            let inner = inner.clone();
            spawn(scope_future(async move {
                inner.set(current_scope().inject::<u32>());
                9usize
            }))
            .detach();
        });
        assert_eq!(tick(1), 1);
        assert_eq!(inner.get(), Some(81));
        assert!(current_scope().is_null());
        drop(parent);
    }

    #[test]
    fn sleeping_task_keeps_its_scope_after_creator_disposal() {
        let baseline = resource_counts();
        let creator = OwnedScope::new();
        creator.provide(99u64);
        let (sender, receiver) = oneshot::channel::<()>();
        let local_value = Rc::new(Cell::new(None::<u32>));
        let local_value_in_task = local_value.clone();
        let task = creator.run(|| {
            let local_value = local_value_in_task.clone();
            spawn(scope_future(async move {
                current_scope().provide(123u32);
                let local = Signal::new(41);
                receiver.await.unwrap();
                assert_eq!(local.get(), 41);
                assert_eq!(current_scope().inject::<u32>(), Some(123));
                assert_eq!(current_scope().inject::<u64>(), None);
                local_value.set(Some(local.get()));
            }))
        });

        assert_eq!(tick(1), 1);
        assert!(resource_counts().signals > baseline.signals);
        drop(creator);
        assert!(!task.is_finished());
        assert!(resource_counts().signals > baseline.signals);
        sender.send(()).unwrap();
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(local_value.get(), Some(41));
        assert_eq!(resource_counts(), baseline);
        assert_eq!(poll_task(&mut { task }), Poll::Ready(()));
    }

    #[test]
    fn parent_disposal_during_poll_does_not_cancel_task() {
        let owner_slot = Rc::new(RefCell::new(None::<OwnedScope>));
        let completed = Rc::new(Cell::new(false));
        let outer = OwnedScope::new();
        let task = outer.run(|| {
            let owner_slot = owner_slot.clone();
            let completed = completed.clone();
            spawn(std::future::poll_fn(move |cx| {
                if owner_slot.borrow_mut().take().is_some() {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    completed.set(true);
                    Poll::Ready(())
                }
            }))
        });
        *owner_slot.borrow_mut() = Some(outer);
        assert_eq!(tick(1), 1);
        assert!(!task.is_finished());
        assert!(!completed.get());
        assert_eq!(tick(1), 1);
        assert!(completed.get());
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut { task }), Poll::Ready(()));
    }

    #[test]
    fn detached_child_survives_parent_task_completion() {
        let owner = OwnedScope::new();
        let child_started = Rc::new(Cell::new(false));
        let child_finished = Rc::new(Cell::new(false));
        let (sender, receiver) = oneshot::channel::<()>();
        let child_started_in_parent = child_started.clone();
        let child_finished_in_parent = child_finished.clone();
        let parent = owner.run(|| {
            spawn(async move {
                spawn({
                    let child_started = child_started_in_parent.clone();
                    let child_finished = child_finished_in_parent.clone();
                    async move {
                        child_started.set(true);
                        let _ = receiver.await;
                        child_finished.set(true);
                    }
                })
                .detach();
            })
        });
        assert_eq!(tick(1), 1);
        assert!(parent.is_finished());
        assert!(!child_started.get());
        assert_eq!(tick(1), 1);
        assert!(child_started.get());
        drop(parent);
        sender.send(()).unwrap();
        assert_eq!(tick(1), 1);
        assert!(child_finished.get());
        drop(owner);
    }

    #[test]
    fn explicitly_scoped_future_drop_runs_in_its_scope() {
        struct Probe {
            marker: Rc<()>,
            signal: ReadSignal<u32>,
            dropped_in_task_scope: Rc<Cell<bool>>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                let same_scope = current_scope()
                    .inject::<Rc<()>>()
                    .is_some_and(|marker| Rc::ptr_eq(&marker, &self.marker));
                self.dropped_in_task_scope
                    .set(same_scope && self.signal.try_get() == Some(7));
            }
        }

        let baseline = resource_counts();
        let owner = OwnedScope::new();
        let owner_counts = resource_counts();
        let dropped_in_task_scope = Rc::new(Cell::new(false));
        let task = owner.run(|| {
            let dropped_in_task_scope = dropped_in_task_scope.clone();
            spawn(scope_future(async move {
                let marker = Rc::new(());
                current_scope().provide(marker.clone());
                let signal = Signal::new(7u32);
                let _probe = Probe {
                    marker,
                    signal: *signal,
                    dropped_in_task_scope,
                };
                std::future::pending::<()>().await;
            }))
        });
        assert_eq!(tick(1), 1);
        assert!(resource_counts().signals > baseline.signals);
        drop(task);
        assert!(dropped_in_task_scope.get());
        assert_eq!(resource_counts(), owner_counts);
        drop(owner);
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn tick_before_spawn_still_destroys_detached_task_at_thread_exit() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let probe = Probe(drops.clone());
        std::thread::spawn(move || {
            assert_eq!(tick(1), 0);
            spawn(async move {
                let _probe = probe;
                std::future::pending::<()>().await;
            })
            .detach();
            tick(1);
        })
        .join()
        .unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn scope_dispose_leaves_creator_observer_and_executor_task_independent() {
        let owner = OwnedScope::new();
        let task = owner.run(|| spawn(async {}));
        let pending_owner = OwnedScope::new();
        let pending = pending_owner.run(|| spawn(std::future::pending::<()>()));
        assert!(!task.is_finished());
        assert!(!pending.is_finished());
        drop(pending_owner);
        assert!(!pending.is_finished());
        tick(1);
        assert!(task.is_finished());
        assert!(!pending.is_finished());
        drop(pending);
    }

    #[test]
    fn detached_errors_are_reported_once() {
        let owner = OwnedScope::new();
        let errors = Rc::new(RefCell::new(Vec::new()));
        owner.run(|| {
            let errors = errors.clone();
            spawn(async { Err::<(), _>("failed") }).detach_and_report(move |error, location| {
                assert!(location.file().ends_with("task.rs"));
                errors.borrow_mut().push(error);
            });
        });
        tick(32);
        tick(32);
        assert_eq!(&*errors.borrow(), &["failed"]);
        owner.run(|| {
            let errors = errors.clone();
            spawn(async { Err::<(), _>("failed again") })
                .detach_and_report(move |error, _| errors.borrow_mut().push(error));
        });
        tick(32);
        tick(32);
        assert_eq!(&*errors.borrow(), &["failed", "failed again"]);
    }

    #[test]
    fn detach_and_report_survives_both_creator_scopes() {
        let errors = Rc::new(RefCell::new(Vec::new()));
        let producer_owner = OwnedScope::new();
        let (sender, receiver) = oneshot::channel::<Result<(), &'static str>>();
        let producer = producer_owner
            .run(|| spawn(async move { receiver.await.unwrap_or(Err("sender dropped")) }));
        tick(1);

        let reporting_scope = OwnedScope::new();
        let errors_for_report = errors.clone();
        reporting_scope.run(|| {
            producer.detach_and_report(move |error, _| {
                errors_for_report.borrow_mut().push(error);
            });
        });
        drop(reporting_scope);
        drop(producer_owner);

        sender.send(Err("late failure")).unwrap();
        for _ in 0..4 {
            tick(8);
        }
        assert_eq!(&*errors.borrow(), &["late failure"]);
    }

    fn block_on_handle<T>(
        mut task: Task<T>,
        first_pending: Option<std::sync::mpsc::SyncSender<()>>,
    ) -> T {
        struct Unpark(thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(Unpark(thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut first_pending = first_pending;
        loop {
            match Pin::new(&mut task).poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => {
                    if let Some(sender) = first_pending.take() {
                        sender.send(()).unwrap();
                    }
                    thread::park();
                }
            }
        }
    }

    #[test]
    fn foreign_thread_can_await_non_send_foreground_future() {
        let local = Rc::new(Cell::new(40));
        let local_for_future = local.clone();
        let task = spawn(async move {
            local_for_future.set(42);
            42u32
        });
        let (pending_sender, pending_receiver) = std::sync::mpsc::sync_channel(0);
        let observer = thread::spawn(move || block_on_handle(task, Some(pending_sender)));
        pending_receiver.recv().unwrap();
        assert_eq!(local.get(), 40);
        assert_eq!(tick(1), 1);
        assert_eq!(observer.join().unwrap(), 42);
        assert_eq!(local.get(), 42);
    }

    #[test]
    fn foreign_drop_cancels_only_original_task_and_drops_future_on_owner() {
        struct Probe {
            thread: ThreadId,
            dropped: Rc<Cell<bool>>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                assert_eq!(thread::current().id(), self.thread);
                self.dropped.set(true);
            }
        }
        let baseline = resource_counts();
        let dropped = Rc::new(Cell::new(false));
        let future_dropped = dropped.clone();
        let task = spawn(scope_future(async move {
            let _probe = Probe {
                thread: thread::current().id(),
                dropped: future_dropped,
            };
            let _local = Signal::new(7u32);
            std::future::pending::<()>().await;
        }));
        assert_eq!(tick(1), 1);
        let token = task.token.clone();
        thread::spawn(move || {
            let other = spawn(std::future::pending::<()>());
            tick(1);
            drop(task);
            assert!(!other.is_finished());
            drop(other);
        })
        .join()
        .unwrap();
        assert!(!dropped.get());
        assert!(!token.is_finished());
        assert_eq!(tick(1), 1);
        assert!(dropped.get());
        assert!(token.is_finished());
        assert_eq!(resource_counts(), baseline);
    }

    #[test]
    fn finished_includes_scoped_cleanup_before_output_observation() {
        let baseline = resource_counts();
        let observer_slot = Rc::new(Cell::new(None::<ReadSignal<u32>>));
        let observer_slot_in_task = observer_slot.clone();
        let mut task = spawn(scope_future(async move {
            let local = Signal::new(7u32);
            observer_slot_in_task.set(Some(*local));
            42
        }));
        assert!(!task.is_finished());
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert!(observer_slot.get().unwrap().is_disposed());
        assert_eq!(resource_counts(), baseline);
        assert_eq!(poll_task(&mut task), Poll::Ready(42));
    }

    #[test]
    fn output_can_be_non_send_and_awaits_without_option() {
        let mut task = spawn(async { Rc::new(String::from("result")) });
        tick(1);
        let Poll::Ready(value) = poll_task(&mut task) else {
            panic!("task not ready")
        };
        assert_eq!(&*value, "result");
    }

    #[test]
    fn completed_send_output_can_be_dropped_on_observer_thread() {
        struct Output(std::sync::mpsc::Sender<ThreadId>);
        impl Drop for Output {
            fn drop(&mut self) {
                self.0.send(thread::current().id()).unwrap();
            }
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = spawn(async move { Output(sender) });
        tick(1);
        let observer = thread::spawn(move || {
            drop(task);
            thread::current().id()
        });
        assert_eq!(receiver.recv().unwrap(), observer.join().unwrap());
    }

    #[test]
    fn tick_before_spawn_still_cleans_detached_tasks_at_thread_exit() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let drops_in_task = drops.clone();
        let weak_token = thread::spawn(move || {
            assert_eq!(tick(1), 0);
            let task = spawn(scope_future(async move {
                let _probe = Probe(drops_in_task);
                let _local = Signal::new(7u32);
                std::future::pending::<()>().await;
            }));
            let token = Arc::downgrade(&task.token);
            task.detach();
            tick(1);
            token
        })
        .join()
        .unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(weak_token.upgrade().is_none());
    }

    #[test]
    #[should_panic(expected = "Task polled after completion")]
    fn task_stopped_by_foreground_thread_exit_panics_when_awaited() {
        let task = thread::spawn(|| {
            let local = Rc::new(());
            let task = spawn(async move {
                let _local = local;
                std::future::pending::<()>().await;
            });
            tick(1);
            task
        })
        .join()
        .unwrap();
        assert!(task.is_finished());
        let _ = block_on_handle(task, None);
    }

    #[test]
    #[should_panic(expected = "Task polled after completion")]
    fn polling_completed_task_again_panics() {
        let mut task = spawn(async { 42u32 });
        tick(1);
        assert_eq!(poll_task(&mut task), Poll::Ready(42));
        let _ = poll_task(&mut task);
    }

    #[test]
    fn unpolled_cancelled_task_releases_capture_and_registration() {
        struct Capture(Rc<Cell<bool>>);
        impl Drop for Capture {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let baseline = resource_counts();
        let dropped = Rc::new(Cell::new(false));
        let capture = Capture(dropped.clone());
        let task = spawn(async move {
            let _capture = capture;
            std::future::pending::<()>().await;
        });
        let token = Arc::downgrade(&task.token);
        drop(task);
        assert!(dropped.get());
        assert!(token.upgrade().is_none());
        assert_eq!(resource_counts(), baseline);
        assert_eq!(tick(1), 0);
    }

    #[test]
    fn shutdown_waits_for_a_remote_wake_before_enqueue() {
        struct Probe(Arc<AtomicUsize>, ThreadId);
        impl Drop for Probe {
            fn drop(&mut self) {
                assert_eq!(thread::current().id(), self.1);
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let drops_in_owner = drops.clone();
        let (setup_sender, setup_receiver) = std::sync::mpsc::sync_channel(0);
        let (exit_sender, exit_receiver) = std::sync::mpsc::sync_channel(0);
        let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let release_in_hook = release.clone();
        let owner = thread::spawn(move || {
            let task = spawn(async move {
                let _probe = Probe(drops_in_owner, thread::current().id());
                std::future::pending::<()>().await;
            });
            tick(1);
            let queue = task.token.queue.clone();
            let waker = EXECUTOR
                .with_borrow(|executor| executor.tasks.get(&task.token.key).unwrap().waker.clone());
            *queue.enqueue_hook.lock().unwrap() = Some(Arc::new(move || {
                entered_sender.send(()).unwrap();
                let (lock, condvar) = &*release_in_hook;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = condvar.wait(released).unwrap();
                }
            }));
            task.detach();
            setup_sender.send((queue, waker)).unwrap();
            exit_receiver.recv().unwrap();
        });
        let (queue, waker) = setup_receiver.recv().unwrap();
        let waking = thread::spawn(move || waker.wake());
        entered_receiver.recv().unwrap(); // SCHEDULED is set; runnable is not yet queued.
        exit_sender.send(()).unwrap();
        {
            let mut state = queue.state.lock().unwrap();
            while !state.closing {
                state = queue.available.wait(state).unwrap();
            }
            assert!(state.ready.is_empty());
        }
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        {
            let (lock, condvar) = &*release;
            *lock.lock().unwrap() = true;
            condvar.notify_one();
        }
        waking.join().unwrap();
        owner.join().unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        *queue.enqueue_hook.lock().unwrap() = None;
    }

    #[test]
    fn resource_owner_survives_configuration_scope() {
        let owner = OwnedScope::new();
        let resource = owner.run(|| AsyncResource::<u32, ()>::new(async { Ok(42) }));
        drop(owner);
        assert_eq!(tick(1), 1);
        assert_eq!(
            resource.state().get_clone_untracked(),
            AsyncState::Ready(Ok(42))
        );
    }
}
