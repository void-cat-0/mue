mod app;
pub mod color;
pub mod event;
pub mod font;
pub mod gesture;
pub mod hook;
pub mod inherit;
pub mod layout;
pub mod math;
pub mod node;
pub mod paint;
pub mod runtime;
pub mod shader;
pub mod style;
pub mod transition;
mod viewport;

pub use app::App;

/// Install a callback for waking a blocking host event loop when a task is woken.
pub fn set_task_wake_callback(callback: Option<std::sync::Arc<dyn Fn() + Send + Sync>>) {
    mue_core::task::set_wake_callback(callback);
}

pub use mue_macros::style;

#[doc(hidden)]
pub mod __private {
    pub use mue_core as core;
}
