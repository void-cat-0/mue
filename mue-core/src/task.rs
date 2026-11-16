use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    future::Future,
    marker::PhantomData,
    mem,
    panic::Location,
    pin::{Pin, pin},
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use async_task::{Runnable, Task as AsyncTask};

use crate::{
    Disposable,
    runtime::Runtime,
    scope::{OwnedScope, Scope, current_scope},
    signal::{ReadSignal, Signal},
};

mod blocking;
pub use blocking::{BlockingTask, spawn_blocking};

/// Metadata attached to every foreground runnable for debugging.
#[derive(Clone, Copy, Debug)]
struct RunnableMeta {
    key: u64,
    location: &'static Location<'static>,
    spawned: Instant,
}

type LocalRunnable = Runnable<RunnableMeta>;

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
    meta: RunnableMeta,
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
        let position = state.ready.iter().position(|r| r.metadata().key == key)?;
        state.ready.remove(position)
    }
}

struct Registration {
    token: Arc<TaskToken>,
    // A sleeping future needs a wake source for shutdown. Keep this outside the
    // token so its allocation can be released when the registration is retired.
    waker: Waker,
}

struct WakeSlot {
    callback: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl WakeSlot {
    fn new() -> Self {
        Self {
            callback: Mutex::new(None),
        }
    }

    fn set(&self, callback: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self.callback.lock().unwrap() = callback;
    }

    fn wake(&self) {
        // Clone before calling so the callback may re-enter set_wake_callback.
        let callback = self.callback.lock().unwrap().clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

struct Executor {
    tasks: HashMap<u64, Registration>,
    queue: Arc<ReadyQueue>,
    wake: Arc<WakeSlot>,
    timers: BTreeMap<Instant, Vec<Waker>>,
    clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>,
    in_tick: bool,
    quitting: bool,
}

impl Executor {
    fn new() -> Self {
        // tick() can initialize the executor before the first spawn. Keep reactive TLS
        // alive through executor teardown in case future destructors access it.
        // UpdateContext is initialized here so its TLS outlives the executor too.
        Runtime::with(|_| {});
        current_scope();
        crate::update_context::UpdateContext::current();
        Self {
            tasks: HashMap::new(),
            queue: Arc::new(ReadyQueue::new()),
            wake: Arc::new(WakeSlot::new()),
            timers: BTreeMap::new(),
            clock: None,
            in_tick: false,
            quitting: false,
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.quitting = true;
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
    static IS_BACKGROUND_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Whether the current thread is a background worker. Reactive access and
/// foreground task APIs are rejected there in debug builds.
pub(crate) fn is_background_worker() -> bool {
    IS_BACKGROUND_WORKER.with(Cell::get)
}

struct TaskFuture<F> {
    future: Option<Pin<Box<F>>>,
    token: Arc<TaskToken>,
}

impl<F: Future> Future for TaskFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let future = this.future.as_mut().unwrap_or_else(|| {
            panic!(
                "task future polled after completion; task spawned at {}",
                this.token.meta.location
            )
        });
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
///
/// The callback is stored in the current thread's executor, so a foreground
/// task spawned earlier observes a callback installed later. It may be called
/// from any thread.
pub fn set_wake_callback(callback: Option<Arc<dyn Fn() + Send + Sync>>) {
    EXECUTOR.with_borrow(|executor| executor.wake.set(callback));
}

/// The current thread's task executor is shutting down or already destroyed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutorClosed;

impl fmt::Display for ExecutorClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("task executor is shutting down")
    }
}

impl std::error::Error for ExecutorClosed {}

/// Stop accepting new tasks on the current thread's executor.
///
/// Existing tasks continue until they complete or the executor is dropped. After
/// this call, [`try_spawn`] returns `Err(ExecutorClosed)` and [`spawn`] panics.
/// The flag is per-thread and cannot be cleared.
pub fn shutdown() {
    let _ = EXECUTOR.try_with(|cell| cell.borrow_mut().quitting = true);
}

/// Whether the current thread's executor is shutting down or already destroyed.
pub fn is_shutting_down() -> bool {
    EXECUTOR
        .try_with(|cell| cell.borrow().quitting)
        .unwrap_or(true)
}

/// The current time used by timers: the installed clock, or `Instant::now()`.
pub fn now() -> Instant {
    let clock = EXECUTOR
        .try_with(|cell| cell.borrow().clock.clone())
        .ok()
        .flatten();
    clock.map_or_else(Instant::now, |clock| clock())
}

/// Install a clock for timers. `None` restores `Instant::now()`.
///
/// Install the clock before creating timers; existing timers keep their absolute
/// deadlines. The closure must be monotonic and must not call task APIs.
pub fn set_clock(clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>) {
    let _ = EXECUTOR.try_with(|cell| cell.borrow_mut().clock = clock);
}

/// A future that completes after `duration` on the executor's clock.
///
/// Foreground-only: it registers with the current thread's executor, so it is
/// `!Send` and cannot be used from a background task. The timer fires during
/// [`tick`]; a host that stops ticking (for example an idle blocking event loop)
/// must arrange a wake at the deadline.
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<mue_core::task::Sleep>();
/// ```
#[derive(Debug)]
#[must_use = "futures do nothing unless polled or spawned"]
pub struct Sleep {
    deadline: Instant,
    waker: Option<Waker>,
    _not_send: PhantomData<Rc<()>>,
}

/// A future that completes after `duration` on the executor's clock.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        deadline: now() + duration,
        waker: None,
        _not_send: PhantomData,
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if now() >= this.deadline {
            return Poll::Ready(());
        }
        if this
            .waker
            .as_ref()
            .is_none_or(|waker| !waker.will_wake(cx.waker()))
        {
            this.waker = Some(cx.waker().clone());
            let registered = EXECUTOR.try_with(|cell| {
                let mut executor = cell.borrow_mut();
                executor
                    .timers
                    .entry(this.deadline)
                    .or_default()
                    .push(cx.waker().clone());
            });
            if registered.is_err() {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}

/// The error returned when a [`timeout`] expires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Elapsed;

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("future timed out")
    }
}

impl std::error::Error for Elapsed {}

/// A future that completes with `Ok` when the inner future finishes, or
/// `Err(Elapsed)` when `duration` elapses first.
#[must_use = "futures do nothing unless polled or spawned"]
pub struct Timeout<F> {
    sleep: Sleep,
    future: Pin<Box<F>>,
}

/// Run `future` with a deadline on the executor's clock.
pub fn timeout<F: Future>(duration: Duration, future: F) -> Timeout<F> {
    Timeout {
        sleep: sleep(duration),
        future: Box::pin(future),
    }
}

impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output, Elapsed>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Poll::Ready(value) = this.future.as_mut().poll(cx) {
            return Poll::Ready(Ok(value));
        }
        match Pin::new(&mut this.sleep).poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(Elapsed)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A caller-owned task yielding T directly. Dropping the handle requests cancellation;
/// detach lets the executor run it without an observer. The creator scope does not own it.
///
/// The handle is Send when T is Send, even for a foreground non-Send future. Moving
/// the handle never moves polling or future destruction away from the foreground thread.
/// Cross-thread cancellation is cleaned up by that thread's next tick or shutdown.
/// Completed output is owned by the handle and may be consumed/dropped on its thread.
/// Polling after completion panics; use [`Task::fallible`] to observe a task stopped
/// by the executor as `None` instead of a panic. Dropping the handle is cancellation,
/// but it consumes the handle, so it cannot be awaited afterwards.
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
    state: TaskState<T>,
    token: Option<Arc<TaskToken>>,
}

// The task handle never pins its output; the inner async-task handle is Unpin.
impl<T> Unpin for Task<T> {}

enum TaskState<T> {
    Ready(Option<T>),
    Spawned(Option<AsyncTask<T, RunnableMeta>>),
}

impl<T> Task<T> {
    /// A task that is already complete with `value`.
    ///
    /// ```
    /// use mue_core::task::Task;
    /// let task = Task::ready(7u32);
    /// assert!(task.is_finished());
    /// ```
    pub fn ready(value: T) -> Self {
        Self {
            state: TaskState::Ready(Some(value)),
            token: None,
        }
    }

    pub fn detach(mut self) {
        match &mut self.state {
            TaskState::Ready(value) => {
                value.take();
            }
            TaskState::Spawned(task) => {
                task.take().expect("task handle already consumed").detach();
            }
        }
    }

    /// Whether the future has been destroyed on its owner thread.
    /// This does not consume or dispose the output held by the result handle.
    pub fn is_finished(&self) -> bool {
        match &self.state {
            TaskState::Ready(_) => true,
            TaskState::Spawned(_) => self.token().is_finished(),
        }
    }

    /// Poll once without blocking. Returns `Some(output)` if the task is ready.
    /// After the output is taken, later calls return `None`.
    ///
    /// This is a polling API for migrating synchronous `take()` patterns; new
    /// code should prefer `await` or [`AsyncResource`]. The task is still driven
    /// by [`tick`]; `try_take` does not run it. If the executor stopped the task,
    /// this panics like `await`; use [`Task::fallible`] and
    /// [`FallibleTask::try_take`] when `None` is preferred.
    ///
    /// ```
    /// use mue_core::task::{spawn, tick};
    /// let mut task = spawn(async { 7u32 });
    /// assert_eq!(task.try_take(), None);
    /// tick(1);
    /// assert_eq!(task.try_take(), Some(7));
    /// assert_eq!(task.try_take(), None);
    /// ```
    pub fn try_take(&mut self) -> Option<T> {
        let value = match &mut self.state {
            TaskState::Ready(value) => return value.take(),
            TaskState::Spawned(task) => {
                let handle = task.as_mut().expect("task handle already consumed");
                let waker = Waker::noop();
                let mut context = Context::from_waker(waker);
                match Pin::new(handle).poll(&mut context) {
                    Poll::Ready(value) => value,
                    Poll::Pending => return None,
                }
            }
        };
        self.state = TaskState::Ready(None);
        Some(value)
    }

    /// The source location where this task was spawned, or `None` for a ready task.
    pub fn location(&self) -> Option<&'static Location<'static>> {
        self.token.as_ref().map(|token| token.meta.location)
    }

    /// When this task was spawned, or `None` for a ready task.
    pub fn spawned_at(&self) -> Option<Instant> {
        self.token.as_ref().map(|token| token.meta.spawned)
    }

    /// Convert this task into a fallible task that yields `None` instead of
    /// panicking when the executor stops the task (for example when its thread
    /// exits). Dropping the handle is cancellation, but it consumes the handle,
    /// so it cannot be awaited afterwards.
    ///
    /// ```
    /// use mue_core::task::Task;
    /// let task = Task::ready(7u32).fallible();
    /// assert!(task.is_finished());
    /// ```
    pub fn fallible(mut self) -> FallibleTask<T> {
        let token = self.token.take();
        let state = match &mut self.state {
            TaskState::Ready(value) => FallibleTaskState::Ready(value.take()),
            TaskState::Spawned(task) => {
                FallibleTaskState::Spawned(task.take().map(|task| task.fallible()))
            }
        };
        FallibleTask { state, token }
    }

    fn token(&self) -> &Arc<TaskToken> {
        self.token
            .as_ref()
            .expect("ready task has no executor token")
    }
}

impl<T: 'static, E: 'static> Task<Result<T, E>> {
    /// Detach while explicitly handling a business error once, on the foreground thread.
    /// Cancellation is silent: it yields `None` and does not invoke the reporter.
    /// If the executor is already shutting down, the reporter is not spawned.
    #[track_caller]
    pub fn detach_and_report(self, report: impl FnOnce(E, &'static Location<'static>) + 'static) {
        let location = Location::caller();
        if let Ok(task) = try_spawn(async move {
            if let Some(Err(error)) = self.fallible().await {
                report(error, location);
            }
        }) {
            task.detach();
        }
    }
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        match &mut this.state {
            TaskState::Ready(value) => {
                Poll::Ready(value.take().expect("task polled after completion"))
            }
            TaskState::Spawned(task) => {
                Pin::new(task.as_mut().expect("task handle already consumed")).poll(cx)
            }
        }
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let TaskState::Spawned(task) = &mut self.state
            && let Some(task) = task.take()
        {
            // async-task closes its own allocation; no lookup in the dropping thread's TLS.
            drop(task);
            if let Some(token) = &self.token {
                token.clean_cancelled();
            }
        }
    }
}

impl<T> fmt::Debug for Task<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("Task");
        match &self.state {
            TaskState::Ready(_) => {
                debug.field("state", &"ready");
            }
            TaskState::Spawned(_) => {
                debug.field("state", &"spawned");
                if let Some(token) = &self.token {
                    debug.field("location", &token.meta.location);
                    debug.field("spawned", &token.meta.spawned);
                    debug.field("finished", &token.is_finished());
                }
            }
        }
        debug.finish()
    }
}

enum FallibleTaskState<T> {
    Ready(Option<T>),
    Spawned(Option<async_task::FallibleTask<T, RunnableMeta>>),
    Stopped,
}

/// A task that yields `Option<T>` instead of panicking when the executor stops it.
///
/// `Some(value)` means the task completed; `None` means the executor closed the task
/// without completing it, for example when its thread exits. Dropping the handle still
/// requests cancellation, but it consumes the handle, so it cannot be awaited afterwards.
#[must_use = "dropping a fallible task cancels it; use detach() to run without observing its output"]
pub struct FallibleTask<T> {
    state: FallibleTaskState<T>,
    token: Option<Arc<TaskToken>>,
}

// The task handle never pins its output; the inner async-task handle is Unpin.
impl<T> Unpin for FallibleTask<T> {}

impl<T> FallibleTask<T> {
    pub fn detach(mut self) {
        match &mut self.state {
            FallibleTaskState::Ready(value) => {
                value.take();
            }
            FallibleTaskState::Spawned(task) => {
                task.take()
                    .expect("fallible task handle already consumed")
                    .detach();
            }
            FallibleTaskState::Stopped => {}
        }
    }

    /// Poll once without blocking.
    ///
    /// Returns `Some(Some(value))` when the task completed, `Some(None)` when
    /// the executor stopped it, and `None` while it is pending or after the
    /// value/stop has already been taken. Like [`Task::try_take`], this does
    /// not drive the task; the executor's `tick` does.
    pub fn try_take(&mut self) -> Option<Option<T>> {
        let polled = match &mut self.state {
            FallibleTaskState::Ready(value) => return value.take().map(Some),
            FallibleTaskState::Spawned(task) => {
                let handle = task
                    .as_mut()
                    .expect("fallible task handle already consumed");
                let waker = Waker::noop();
                let mut context = Context::from_waker(waker);
                Pin::new(handle).poll(&mut context)
            }
            FallibleTaskState::Stopped => return None,
        };
        match polled {
            Poll::Ready(Some(value)) => {
                self.state = FallibleTaskState::Ready(None);
                Some(Some(value))
            }
            Poll::Ready(None) => {
                self.state = FallibleTaskState::Stopped;
                Some(None)
            }
            Poll::Pending => None,
        }
    }

    /// Whether the task has completed, been cancelled, or been stopped by shutdown.
    pub fn is_finished(&self) -> bool {
        match &self.state {
            FallibleTaskState::Ready(_) | FallibleTaskState::Stopped => true,
            FallibleTaskState::Spawned(task) => {
                task.as_ref().is_some_and(|task| task.is_finished())
            }
        }
    }
}

impl<T> Future for FallibleTask<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let polled = match &mut this.state {
            FallibleTaskState::Ready(value) => return Poll::Ready(value.take()),
            FallibleTaskState::Spawned(task) => Pin::new(
                task.as_mut()
                    .expect("fallible task handle already consumed"),
            )
            .poll(cx),
            FallibleTaskState::Stopped => return Poll::Ready(None),
        };
        match &polled {
            Poll::Ready(Some(_)) => this.state = FallibleTaskState::Ready(None),
            Poll::Ready(None) => this.state = FallibleTaskState::Stopped,
            Poll::Pending => {}
        }
        polled
    }
}

impl<T> Drop for FallibleTask<T> {
    fn drop(&mut self) {
        if let FallibleTaskState::Spawned(task) = &mut self.state
            && let Some(task) = task.take()
        {
            drop(task);
            if let Some(token) = &self.token {
                token.clean_cancelled();
            }
        }
    }
}

impl<T> fmt::Debug for FallibleTask<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("FallibleTask");
        match &self.state {
            FallibleTaskState::Ready(_) => {
                debug.field("state", &"ready");
            }
            FallibleTaskState::Spawned(_) => {
                debug.field("state", &"spawned");
                if let Some(token) = &self.token {
                    debug.field("location", &token.meta.location);
                    debug.field("spawned", &token.meta.spawned);
                    debug.field("finished", &token.is_finished());
                }
            }
            FallibleTaskState::Stopped => {
                debug.field("state", &"stopped");
            }
        }
        debug.finish()
    }
}

/// Spawn a non-Send foreground future without implicitly inheriting reactive scope.
/// Its handle owns the task and awaits to its output. Each poll and future destructor
/// runs without an ambient scope, effect tracker, or `UpdateContext`, restoring them
/// on normal return. Captured signals keep their existing owner. For new task-owned reactive resources
/// or inherited context, use [`crate::scope::scope_future`]; otherwise dispose new
/// resources explicitly or provide an owner.
///
/// Panics if the current thread's executor is shutting down; use [`try_spawn`] to
/// observe that as `Err(ExecutorClosed)` instead.
#[track_caller]
pub fn spawn<F: Future + 'static>(future: F) -> Task<F::Output>
where
    F::Output: 'static,
{
    try_spawn(future).unwrap_or_else(|_| panic!("cannot spawn a task after executor shutdown"))
}

/// Spawn a non-Send foreground future, returning `Err(ExecutorClosed)` instead of
/// panicking when the current thread's executor is shutting down or already destroyed.
///
/// This is safe to call from a future destructor during executor shutdown. The future
/// is dropped on the calling thread when the executor rejects it.
#[track_caller]
pub fn try_spawn<F: Future + 'static>(future: F) -> Result<Task<F::Output>, ExecutorClosed>
where
    F::Output: 'static,
{
    assert!(
        !is_background_worker(),
        "foreground tasks must be spawned on the foreground thread"
    );
    let location = Location::caller();
    EXECUTOR
        .try_with(move |cell| {
            let (queue, wake) = {
                let executor = cell.borrow();
                if executor.quitting {
                    return Err(ExecutorClosed);
                }
                (executor.queue.clone(), executor.wake.clone())
            };
            let key = next_task_key();
            let meta = RunnableMeta {
                key,
                location,
                spawned: Instant::now(),
            };
            let token = Arc::new(TaskToken {
                key,
                owner_thread: thread::current().id(),
                queue: queue.clone(),
                finished: AtomicBool::new(false),
                meta,
            });
            let wrapped = TaskFuture {
                future: Some(Box::pin(future)),
                token: token.clone(),
            };
            let (runnable, task) = async_task::Builder::new()
                .metadata(meta)
                .propagate_panic(false)
                .spawn_local(
                    move |_| wrapped,
                    move |runnable| {
                        if queue.schedule(runnable) {
                            wake.wake();
                        }
                    },
                );
            let registration = Registration {
                token: token.clone(),
                waker: runnable.waker(),
            };
            cell.borrow_mut().tasks.insert(token.key, registration);
            runnable.schedule();
            Ok(Task {
                state: TaskState::Spawned(Some(task)),
                token: Some(token),
            })
        })
        .unwrap_or(Err(ExecutorClosed))
}

struct TickGuard<'a> {
    executor: &'a RefCell<Executor>,
}

impl Drop for TickGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut executor) = self.executor.try_borrow_mut() {
            executor.in_tick = false;
        }
    }
}

/// Poll at most limit tasks ready at the start of this tick. Wakes during polling
/// are deferred to the next tick. One poll cannot be preempted by this budget.
///
/// Panics if the current thread's executor is shutting down; use [`try_tick`] to
/// observe that as `Err(ExecutorClosed)` instead.
pub fn tick(limit: usize) -> usize {
    try_tick(limit).unwrap_or_else(|_| panic!("cannot tick tasks after executor shutdown"))
}

/// Like [`tick`], but returns `Err(ExecutorClosed)` instead of panicking when the
/// current thread's executor is shutting down or already destroyed.
pub fn try_tick(limit: usize) -> Result<usize, ExecutorClosed> {
    assert!(
        !is_background_worker(),
        "foreground tasks must be ticked on the foreground thread"
    );
    EXECUTOR
        .try_with(|cell| {
            let clock = {
                let mut executor = cell.borrow_mut();
                if executor.in_tick {
                    return 0;
                }
                executor.in_tick = true;
                executor.clock.clone()
            };
            let _guard = TickGuard { executor: cell };
            let now = clock.map_or_else(Instant::now, |clock| clock());
            let (queue, due) = {
                let mut executor = cell.borrow_mut();
                let mut due = Vec::new();
                executor.timers.retain(|deadline, wakers| {
                    if *deadline <= now {
                        due.push(mem::take(wakers));
                        false
                    } else {
                        true
                    }
                });
                (executor.queue.clone(), due)
            };
            for wakers in due {
                for waker in wakers {
                    waker.wake();
                }
            }
            let ready = queue
                .state
                .lock()
                .unwrap()
                .ready
                .iter()
                .take(limit)
                .map(|r| r.metadata().key)
                .collect::<Vec<_>>();
            let mut ran = 0;
            for key in ready {
                let Some(runnable) = queue.take_key(key) else {
                    continue;
                };
                runnable.run();
                ran += 1;
            }
            ran
        })
        .map_err(|_| ExecutorClosed)
}

/// Block the current thread until `future` completes, driving ready tasks and
/// timers with [`tick`] while waiting.
///
/// This is intended for tests and headless hosts. Do not call it on the UI thread
/// while the frame loop is running: it parks the thread and can deadlock the host.
/// It is not reentrant with `tick`. With a fake clock installed, advance it from
/// another thread; `block_on` cannot advance it.
pub fn block_on<F: Future>(future: F) -> F::Output {
    assert!(
        !is_background_worker(),
        "block_on must run on the foreground thread"
    );
    let (waker, notify) = block_waker();
    let _restore = install_block_wake(notify);
    let mut future = pin!(future);
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        let ran = try_tick(usize::MAX).unwrap_or(0);
        if ran == 0 {
            match next_timer_deadline() {
                Some(deadline) => {
                    let current = now();
                    thread::park_timeout(deadline.saturating_duration_since(current));
                }
                None => thread::park(),
            }
        }
    }
}

/// Like [`block_on`], but return `None` if `future` does not complete within
/// `duration` of wall-clock time. The future is dropped on timeout.
///
/// This uses `std::time::Instant` for the deadline; executor timers still use
/// the executor's clock.
pub fn block_with_timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    assert!(
        !is_background_worker(),
        "block_with_timeout must run on the foreground thread"
    );
    let (waker, notify) = block_waker();
    let _restore = install_block_wake(notify);
    let deadline = Instant::now() + duration;
    let mut future = pin!(future);
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return Some(value);
        }
        let _ = try_tick(usize::MAX);
        let wall_now = Instant::now();
        if wall_now >= deadline {
            return None;
        }
        let mut park_until = deadline;
        if let Some(timer) = next_timer_deadline() {
            let timer_wall = wall_now + timer.saturating_duration_since(now());
            if timer_wall < park_until {
                park_until = timer_wall;
            }
        }
        thread::park_timeout(park_until - wall_now);
    }
}

fn block_waker() -> (Waker, Arc<dyn Fn() + Send + Sync>) {
    struct Unpark(thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let unpark = Arc::new(Unpark(thread::current()));
    let waker = Waker::from(unpark.clone());
    let notify: Arc<dyn Fn() + Send + Sync> = {
        let unpark = unpark.clone();
        Arc::new(move || unpark.0.unpark())
    };
    (waker, notify)
}

fn install_block_wake(notify: Arc<dyn Fn() + Send + Sync>) -> WakeRestore {
    let previous = EXECUTOR
        .try_with(|cell| cell.borrow().wake.callback.lock().unwrap().clone())
        .ok()
        .flatten();
    let _ = EXECUTOR.try_with(|cell| cell.borrow().wake.set(Some(notify)));
    WakeRestore { previous }
}

struct WakeRestore {
    previous: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Drop for WakeRestore {
    fn drop(&mut self) {
        let previous = self.previous.take();
        let _ = EXECUTOR.try_with(|cell| {
            if let Ok(executor) = cell.try_borrow() {
                executor.wake.set(previous);
            }
        });
    }
}

/// The earliest pending timer deadline on the current thread's executor.
///
/// A host that stops calling [`tick`] (for example an idle blocking event loop)
/// can park until this deadline and then tick. Returns `None` when no timer is
/// registered. The value is an absolute [`Instant`] on the executor's clock.
pub fn next_timer_deadline() -> Option<Instant> {
    EXECUTOR
        .try_with(|cell| cell.borrow().timers.keys().next().copied())
        .ok()
        .flatten()
}

/// A set of foreground tasks joined together.
///
/// The set owns every spawned `Task`; dropping the set cancels the remaining
/// tasks. `join_next` yields outputs as tasks complete; `join_all` drains the
/// set and returns outputs in completion order. Foreground-only: it uses
/// [`spawn`], so it must be driven by `tick` on the thread that created it.
#[derive(Debug, Default)]
pub struct JoinSet<T> {
    tasks: Vec<Task<T>>,
}

impl<T: 'static> JoinSet<T> {
    /// An empty set.
    pub const fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Spawn a foreground task and add it to the set.
    ///
    /// Panics if the current thread's executor is shutting down.
    #[track_caller]
    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = T> + 'static,
    {
        self.tasks.push(spawn(future));
    }

    /// Number of tasks that have not completed yet.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the set has no pending tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Wait for the next task to complete, or return `None` when the set is empty.
    ///
    /// Dropping the returned future leaves the set and its tasks intact.
    pub fn join_next(&mut self) -> JoinNext<'_, T> {
        JoinNext { set: self }
    }

    /// Wait for every task and return outputs in completion order.
    ///
    /// Dropping the returned future cancels the remaining tasks.
    pub async fn join_all(mut self) -> Vec<T> {
        let mut outputs = Vec::with_capacity(self.tasks.len());
        while let Some(value) = self.join_next().await {
            outputs.push(value);
        }
        outputs
    }

    /// Cancel and remove all tasks.
    pub fn clear(&mut self) {
        self.tasks.clear();
    }
}

/// Future returned by [`JoinSet::join_next`].
#[must_use = "futures do nothing unless polled or spawned"]
pub struct JoinNext<'a, T> {
    set: &'a mut JoinSet<T>,
}

impl<T> Future for JoinNext<'_, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let mut index = 0;
        while index < this.set.tasks.len() {
            match Pin::new(&mut this.set.tasks[index]).poll(cx) {
                Poll::Ready(value) => {
                    drop(this.set.tasks.swap_remove(index));
                    return Poll::Ready(Some(value));
                }
                Poll::Pending => index += 1,
            }
        }
        if this.set.tasks.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

/// A renderable result state for one asynchronous operation. Dropping the
/// resource cancels its task and disposes its state signal.
#[derive(Clone, Debug, PartialEq)]
pub enum AsyncState<T, E> {
    Loading,
    Ready(Result<T, E>),
}

impl<T, E> AsyncState<T, E> {
    /// Whether the operation is still running.
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// The successful value, if the operation completed with `Ok`.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Ready(Ok(value)) => Some(value),
            _ => None,
        }
    }

    /// The error, if the operation completed with `Err`.
    pub fn error(&self) -> Option<&E> {
        match self {
            Self::Ready(Err(error)) => Some(error),
            _ => None,
        }
    }

    /// Map the successful value, preserving `Loading` and errors.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> AsyncState<U, E> {
        match self {
            Self::Loading => AsyncState::Loading,
            Self::Ready(Ok(value)) => AsyncState::Ready(Ok(f(value))),
            Self::Ready(Err(error)) => AsyncState::Ready(Err(error)),
        }
    }

    /// Map the error, preserving `Loading` and successful values.
    pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> AsyncState<T, F> {
        match self {
            Self::Loading => AsyncState::Loading,
            Self::Ready(Ok(value)) => AsyncState::Ready(Ok(value)),
            Self::Ready(Err(error)) => AsyncState::Ready(Err(f(error))),
        }
    }
}

pub struct AsyncResource<T: 'static, E: 'static> {
    state: Signal<AsyncState<T, E>>,
    state_owner: Option<OwnedScope>,
    task: Option<Task<()>>,
    generation: Rc<Cell<u64>>,
}

impl<T: 'static, E: 'static> AsyncResource<T, E> {
    /// Run the operation on the foreground executor. The future is polled on the
    /// UI thread; if it only awaits a foreign handle (for example a host I/O task),
    /// the actual work happens on the host's runtime. Use [Self::new_blocking] for
    /// blocking closures, and the host's runtime for I/O.
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let state_owner = OwnedScope::new_inherited();
        let state = state_owner.run(|| Signal::new_always(AsyncState::Loading));
        let generation = Rc::new(Cell::new(0));
        let task = Self::spawn_publisher(state, Rc::clone(&generation), 0, future);
        Self {
            state,
            state_owner: Some(state_owner),
            task: Some(task),
            generation,
        }
    }

    /// Replace the current operation. The previous publisher is cancelled, the
    /// state returns to [`AsyncState::Loading`], and only the latest operation
    /// publishes its result.
    pub fn reload_with<F>(&mut self, future: F)
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        self.task.take();
        let _ = self.state.try_set(AsyncState::Loading);
        self.task = Some(Self::spawn_publisher(
            self.state,
            Rc::clone(&self.generation),
            next,
            future,
        ));
    }

    fn spawn_publisher<F>(
        state: Signal<AsyncState<T, E>>,
        generation: Rc<Cell<u64>>,
        expected: u64,
        future: F,
    ) -> Task<()>
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        spawn(async move {
            let output = future.await;
            if generation.get() == expected {
                let _ = state.try_set(AsyncState::Ready(output));
            }
        })
    }

    pub fn state(&self) -> ReadSignal<AsyncState<T, E>> {
        *self.state
    }
}

impl<T: 'static + Send, E: 'static + Send> AsyncResource<T, E> {
    /// Run a blocking closure on a fresh OS thread and publish its result on the
    /// UI thread. Use for low-frequency file I/O or decoding; for frequent
    /// blocking work, use the host's blocking pool.
    ///
    /// ```
    /// use mue_core::prelude::*;
    /// use mue_core::task::{AsyncResource, AsyncState};
    /// let resource = AsyncResource::<u32, ()>::new_blocking(|| Ok(7));
    /// assert_eq!(resource.state().get_clone_untracked(), AsyncState::Loading);
    /// ```
    pub fn new_blocking<F>(f: F) -> Self
    where
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        Self::new(async move { spawn_blocking(f).await })
    }

    /// Replace the current operation with a blocking closure. See
    /// [`Self::reload_with`].
    pub fn reload_blocking<F>(&mut self, f: F)
    where
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        self.reload_with(async move { spawn_blocking(f).await });
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
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::Waker,
        time::{Duration, Instant},
    };

    fn poll_task<T>(task: &mut Task<T>) -> Poll<T> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        Pin::new(task).poll(&mut cx)
    }

    fn poll_fallible<T>(task: &mut FallibleTask<T>) -> Poll<Option<T>> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        Pin::new(task).poll(&mut cx)
    }

    #[test]
    fn ready_task_is_finished_and_yields_its_value() {
        let mut task = Task::ready(7u32);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(7));
    }

    #[test]
    fn try_take_ready_task_yields_once() {
        let mut task = Task::ready(7u32);
        assert_eq!(task.try_take(), Some(7));
        assert_eq!(task.try_take(), None);
        assert!(task.is_finished());
        task.detach();
    }

    #[test]
    fn try_take_pending_task_returns_none() {
        let mut task = spawn(std::future::pending::<u32>());
        assert_eq!(task.try_take(), None);
        assert!(!task.is_finished());
        task.detach();
    }

    #[test]
    fn try_take_completed_spawned_task_yields_output() {
        let mut task = spawn(async { 42u32 });
        assert_eq!(tick(1), 1);
        assert_eq!(task.try_take(), Some(42));
        assert_eq!(task.try_take(), None);
    }

    #[test]
    fn try_take_after_wake_yields_output() {
        let (sender, receiver) = oneshot::channel::<u32>();
        let mut task = spawn(async move { receiver.await.unwrap() });
        assert_eq!(task.try_take(), None);
        sender.send(9).unwrap();
        assert_eq!(tick(1), 1);
        assert_eq!(task.try_take(), Some(9));
    }

    #[test]
    fn detach_after_try_take_is_noop() {
        let mut task = spawn(async { 1u32 });
        assert_eq!(tick(1), 1);
        assert_eq!(task.try_take(), Some(1));
        task.detach();
    }

    #[test]
    fn fallible_task_yields_some_for_a_completed_task() {
        let mut task = spawn(async { 42u32 }).fallible();
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_fallible(&mut task), Poll::Ready(Some(42)));
    }

    #[test]
    fn fallible_task_yields_none_when_executor_stops_it() {
        let task = thread::spawn(|| {
            let task = spawn(std::future::pending::<()>()).fallible();
            tick(1);
            task
        })
        .join()
        .unwrap();
        assert!(task.is_finished());
        let mut task = task;
        assert_eq!(poll_fallible(&mut task), Poll::Ready(None));
    }

    #[test]
    fn fallible_try_take_ready_task_yields_once() {
        let mut task = Task::ready(7u32).fallible();
        assert_eq!(task.try_take(), Some(Some(7)));
        assert_eq!(task.try_take(), None);
        assert!(task.is_finished());
        task.detach();
    }

    #[test]
    fn fallible_try_take_pending_task_returns_none() {
        let mut task = spawn(std::future::pending::<u32>()).fallible();
        assert_eq!(task.try_take(), None);
        assert!(!task.is_finished());
        task.detach();
    }

    #[test]
    fn fallible_try_take_completed_task_yields_output() {
        let mut task = spawn(async { 42u32 }).fallible();
        assert_eq!(tick(1), 1);
        assert_eq!(task.try_take(), Some(Some(42)));
        assert_eq!(task.try_take(), None);
    }

    #[test]
    fn fallible_try_take_does_not_drive_the_task() {
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_in_task = ran.clone();
        let mut task = spawn(async move {
            ran_in_task.store(1, Ordering::SeqCst);
            5u32
        })
        .fallible();
        assert_eq!(task.try_take(), None);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        assert_eq!(tick(1), 1);
        assert_eq!(ran.load(Ordering::SeqCst), 1);
        assert_eq!(task.try_take(), Some(Some(5)));
    }

    #[test]
    fn fallible_try_take_stopped_task_reports_none() {
        let task = thread::spawn(|| {
            let task = spawn(std::future::pending::<()>()).fallible();
            tick(1);
            task
        })
        .join()
        .unwrap();
        let mut task = task;
        assert!(task.is_finished());
        assert_eq!(task.try_take(), Some(None));
        assert_eq!(task.try_take(), None);
    }

    #[test]
    fn fallible_try_take_after_shutdown_reports_stopped() {
        let task = thread::spawn(|| {
            let task = spawn(std::future::pending::<()>()).fallible();
            tick(1);
            shutdown();
            task
        })
        .join()
        .unwrap();
        let mut task = task;
        assert!(task.is_finished());
        assert_eq!(task.try_take(), Some(None));
    }

    #[test]
    fn task_debug_and_metadata_report_spawn_site() {
        let task = spawn(async {});
        let debug = format!("{task:?}");
        assert!(debug.contains("spawned"));
        assert!(debug.contains("task.rs"));
        assert!(
            task.location()
                .is_some_and(|location| location.file().ends_with("task.rs"))
        );
        assert!(task.spawned_at().is_some());
        drop(task);

        let ready = Task::ready(1u32);
        assert!(ready.location().is_none());
        assert!(ready.spawned_at().is_none());
        assert!(format!("{ready:?}").contains("ready"));
    }

    #[test]
    fn try_spawn_is_rejected_after_shutdown() {
        shutdown();
        assert!(is_shutting_down());
        assert!(try_spawn(async {}).is_err());
        assert!(std::panic::catch_unwind(|| spawn(async {})).is_err());
        // Keep later tests on this thread unaffected.
        EXECUTOR.with_borrow_mut(|executor| executor.quitting = false);
    }

    #[test]
    fn try_tick_still_runs_existing_tasks_after_shutdown() {
        let task = spawn(async { 7u32 });
        shutdown();
        assert_eq!(try_tick(1), Ok(1));
        assert!(task.is_finished());
        assert!(try_spawn(async {}).is_err());
        EXECUTOR.with_borrow_mut(|executor| executor.quitting = false);
        drop(task);
    }

    #[test]
    fn try_spawn_from_future_destructor_during_shutdown_is_rejected() {
        let rejected = Arc::new(Mutex::new(None));
        let rejected_in_thread = rejected.clone();
        thread::spawn(move || {
            struct Probe(Arc<Mutex<Option<bool>>>);
            impl Drop for Probe {
                fn drop(&mut self) {
                    *self.0.lock().unwrap() = Some(try_spawn(async {}).is_err());
                }
            }

            let probe = Probe(rejected_in_thread);
            spawn(async move {
                let _probe = probe;
                std::future::pending::<()>().await;
            })
            .detach();
            tick(1);
        })
        .join()
        .unwrap();
        assert_eq!(*rejected.lock().unwrap(), Some(true));
    }

    #[test]
    fn tick_resets_in_tick_after_poll_panic() {
        let result = std::panic::catch_unwind(|| {
            let task = spawn(async { panic!("task poll panic") });
            tick(1);
            drop(task);
        });
        assert!(result.is_err());

        let task = spawn(async { 7u32 });
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        drop(task);
    }

    #[test]
    fn tick_restores_the_driving_effect_after_a_poll_panic() {
        let owner = OwnedScope::new();
        let source = owner.run(|| Signal::new(0u32));
        let runs = Rc::new(Cell::new(0));
        let runs_in_effect = runs.clone();
        owner.run(|| {
            watch_effect(move || {
                source.get();
                runs_in_effect.set(runs_in_effect.get() + 1);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let task = spawn(async { panic!("task poll panic") });
                    tick(1);
                    drop(task);
                }));
                assert!(result.is_err());
            });
        });
        assert_eq!(runs.get(), 1);
        source.set(1);
        assert_eq!(runs.get(), 2);
        drop(owner);
    }

    fn fake_clock() -> Arc<Mutex<Instant>> {
        let clock = Arc::new(Mutex::new(Instant::now()));
        let clock_for_closure = clock.clone();
        set_clock(Some(Arc::new(move || *clock_for_closure.lock().unwrap())));
        clock
    }

    #[test]
    fn sleep_completes_after_advancing_the_fake_clock() {
        let clock = fake_clock();
        let mut task = spawn(async {
            sleep(Duration::from_millis(10)).await;
            7u32
        });
        assert_eq!(tick(1), 1);
        assert!(!task.is_finished());
        *clock.lock().unwrap() += Duration::from_millis(10);
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(7));
        set_clock(None);
    }

    #[test]
    fn sleep_does_not_complete_before_the_deadline() {
        let clock = fake_clock();
        let mut task = spawn(async {
            sleep(Duration::from_millis(10)).await;
            1u32
        });
        assert_eq!(tick(1), 1);
        assert_eq!(tick(1), 0);
        assert!(!task.is_finished());
        *clock.lock().unwrap() += Duration::from_millis(9);
        assert_eq!(tick(1), 0);
        assert!(!task.is_finished());
        *clock.lock().unwrap() += Duration::from_millis(1);
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(1));
        set_clock(None);
    }

    #[test]
    fn sleep_zero_is_ready_immediately() {
        let mut task = spawn(async {
            sleep(Duration::ZERO).await;
            3u32
        });
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(3));
    }

    #[test]
    fn timeout_returns_elapsed_when_the_clock_advances() {
        let clock = fake_clock();
        let mut task =
            spawn(async { timeout(Duration::from_millis(5), std::future::pending::<u32>()).await });
        assert_eq!(tick(1), 1);
        assert!(!task.is_finished());
        *clock.lock().unwrap() += Duration::from_millis(5);
        assert_eq!(tick(1), 1);
        assert_eq!(poll_task(&mut task), Poll::Ready(Err(Elapsed)));
        set_clock(None);
    }

    #[test]
    fn timeout_returns_the_inner_value_when_it_finishes_first() {
        let mut task = spawn(async { timeout(Duration::from_millis(5), async { 42u32 }).await });
        assert_eq!(tick(1), 1);
        assert_eq!(poll_task(&mut task), Poll::Ready(Ok(42)));
    }

    #[test]
    fn join_set_join_all_returns_all_outputs() {
        let mut set = JoinSet::new();
        set.spawn(async { 1u32 });
        set.spawn(async { 2u32 });
        set.spawn(async { 3u32 });
        let mut task = spawn(async move {
            let mut values = set.join_all().await;
            values.sort_unstable();
            values
        });
        tick(32);
        tick(32);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(vec![1, 2, 3]));
    }

    #[test]
    fn join_next_returns_none_when_the_set_is_empty() {
        let mut set: JoinSet<u32> = JoinSet::new();
        let mut task = spawn(async move { set.join_next().await });
        assert_eq!(tick(1), 1);
        assert!(task.is_finished());
        assert_eq!(poll_task(&mut task), Poll::Ready(None));
    }

    #[test]
    fn dropping_a_join_set_cancels_pending_tasks() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicUsize::new(0));
        let probe = Probe(dropped.clone());
        let mut set = JoinSet::new();
        set.spawn(async move {
            let _probe = probe;
            std::future::pending::<()>().await;
        });
        assert_eq!(tick(1), 1);
        drop(set);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert_eq!(tick(1), 0);
    }

    #[test]
    fn block_on_returns_a_task_output() {
        let task = spawn(async { 7u32 });
        assert_eq!(block_on(task), 7);
    }

    #[test]
    fn block_on_drives_a_timer() {
        let task = spawn(async {
            sleep(Duration::from_millis(1)).await;
            9u32
        });
        assert_eq!(block_on(task), 9);
    }

    #[test]
    fn block_with_timeout_returns_none_for_a_pending_future() {
        assert_eq!(
            block_with_timeout(Duration::from_millis(1), std::future::pending::<()>()),
            None
        );
    }

    #[test]
    fn block_with_timeout_returns_some_for_a_ready_future() {
        assert_eq!(
            block_with_timeout(Duration::from_millis(100), async { 5u32 }),
            Some(5)
        );
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
    fn task_poll_and_drop_do_not_inherit_update_context() {
        use crate::update_context::{UpdateContext, with_update_context};

        let owner = OwnedScope::new();
        let source = owner.run(|| Signal::new(0u32));
        let observed = Rc::new(RefCell::new(Vec::new()));
        let observed_in_effect = observed.clone();
        owner.run(|| {
            watch_effect(move || {
                source.get();
                observed_in_effect
                    .borrow_mut()
                    .push(UpdateContext::current().get::<u32>().copied());
            });
        });
        let task = spawn(async move {
            assert_eq!(UpdateContext::current().get::<u32>(), None);
            source.set(1);
            std::future::pending::<()>().await;
        });
        with_update_context(7u32, || {
            tick(1);
            assert_eq!(UpdateContext::current().get::<u32>(), Some(&7));
            drop(task);
            assert_eq!(UpdateContext::current().get::<u32>(), Some(&7));
        });
        assert_eq!(observed.borrow().last().copied().flatten(), None);
        drop(owner);
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
    fn async_state_helpers_expose_loading_value_and_error() {
        let loading = AsyncState::<u32, &'static str>::Loading;
        assert!(loading.is_loading());
        assert_eq!(loading.value(), None);
        assert_eq!(loading.error(), None);

        let ok = AsyncState::<u32, &'static str>::Ready(Ok(7));
        assert!(!ok.is_loading());
        assert_eq!(ok.value(), Some(&7));
        assert_eq!(ok.error(), None);
        assert_eq!(ok.clone().map(|value| value + 1), AsyncState::Ready(Ok(8)));

        let error = AsyncState::<u32, &'static str>::Ready(Err("boom"));
        assert_eq!(error.value(), None);
        assert_eq!(error.error(), Some(&"boom"));
        assert_eq!(
            error.map_err(|message| message.len()),
            AsyncState::Ready(Err(4usize))
        );
    }

    #[test]
    fn reload_with_replaces_the_operation() {
        let (old_sender, old_receiver) = oneshot::channel::<Result<u32, ()>>();
        let mut resource =
            AsyncResource::<u32, ()>::new(async move { old_receiver.await.unwrap_or(Err(())) });
        assert_eq!(tick(1), 1);
        let (new_sender, new_receiver) = oneshot::channel::<Result<u32, ()>>();
        resource.reload_with(async move { new_receiver.await.unwrap_or(Err(())) });
        assert_eq!(resource.state().get_clone_untracked(), AsyncState::Loading);
        assert!(old_sender.send(Ok(1)).is_err());
        new_sender.send(Ok(2)).unwrap();
        assert_eq!(tick(1), 1);
        assert_eq!(
            resource.state().get_clone_untracked(),
            AsyncState::Ready(Ok(2))
        );
    }

    #[test]
    fn new_blocking_publishes_result() {
        let resource = AsyncResource::<u32, ()>::new_blocking(|| Ok(7));
        let deadline = Instant::now() + Duration::from_secs(5);
        while resource.state().get_clone_untracked() != AsyncState::Ready(Ok(7)) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for blocking result"
            );
            tick(1);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn next_timer_deadline_reports_a_pending_timer() {
        let task = spawn(async { sleep(Duration::from_millis(10)).await });
        assert_eq!(tick(1), 1);
        assert!(next_timer_deadline().is_some());
        task.detach();
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
    fn wake_callback_installed_after_spawn_is_observed() {
        let owner = OwnedScope::new();
        let waker_slot = Rc::new(RefCell::new(None));
        let task = owner.run(|| {
            let waker_slot = waker_slot.clone();
            spawn(std::future::poll_fn(move |cx| {
                *waker_slot.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            }))
        });
        tick(1);
        let calls = Arc::new(AtomicUsize::new(0));
        set_wake_callback(Some({
            let calls = calls.clone();
            Arc::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
            })
        }));
        let waker = waker_slot.borrow_mut().take().unwrap();
        let before = calls.load(Ordering::SeqCst);
        std::thread::spawn(move || waker.wake()).join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), before + 1);
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
        let token = task.token().clone();
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
            let token = Arc::downgrade(task.token());
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
        let token = Arc::downgrade(task.token());
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
            let queue = task.token().queue.clone();
            let waker = EXECUTOR.with_borrow(|executor| {
                executor.tasks.get(&task.token().key).unwrap().waker.clone()
            });
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
