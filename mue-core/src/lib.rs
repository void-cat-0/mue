//! Reactive primitives and a foreground executor for mue applications.
//!
//! Signals, derived values, effects, and context are managed by a thread-local
//! reactive runtime. Non-owning handles are confined to that runtime thread; use
//! [`OwnedScope`] to control resource lifetime.
//!
//! # Foreground tasks
//!
//! [`task::spawn`] schedules a non-`Send` future on the current thread. The
//! host drives future polling with [`task::tick`], typically once per frame.
//! Dropping a [`Task`] requests cancellation. [`Task::detach`] leaves execution
//! with the executor. Wrap an operation in [`scope_future`] when it needs an
//! operation-local owner or inherited context.
//!
//! ```
//! use mue_core::task::{block_on, spawn};
//!
//! let task = spawn(async { 6 * 7 });
//! assert_eq!(block_on(task), 42);
//! ```
//!
//! # Reactive lifetimes
//!
//! Resources created under an attached [`scope::Scope`] are disposed with that
//! scope. A detached [`OwnedScope`] provides explicit RAII ownership.
//! [`ScopedFuture`] owns resources created while its inner future is polled and
//! disposes them on completion or drop. Handles returned from a disposed scope
//! do not keep its resources alive.
//!
//! See [`scope`], [`signal`], [`effect`], and [`task`] for their respective
//! lifecycle and scheduling contracts.

mod disposable;
pub mod effect;
pub mod prelude;
pub mod prop;
pub mod runtime;
pub mod scope;
pub mod signal;
pub mod task;

pub use disposable::{Disposable, Owned};
pub use runtime::batch;
pub use scope::{OwnedScope, ScopedFuture, scope_future};
pub use task::{
    BlockingTask, Elapsed, ExecutorClosed, FallibleTask, JoinSet, Task, block_on,
    block_with_timeout, next_timer_deadline, sleep, spawn, spawn_blocking, timeout, try_spawn,
};
pub mod units;
pub mod update_context;
pub use units::{FontSize, Size};
