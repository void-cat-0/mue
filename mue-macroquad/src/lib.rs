//! A reactive UI renderer and retained Scene runtime for [macroquad].
//!
//! Build a root [`node`] tree with [`App::new`], or implement [`Scene`] when an
//! application needs gameplay drawing, multiple UI roots, retained screens, or
//! explicit input/lifecycle policy. An [`App`] owns the Scene stack and runs one
//! frame at a time; the host still owns macroquad's asynchronous event loop.
//!
//! # A minimal host loop
//!
//! Create the App after macroquad initializes, then call [`App::try_frame`] once
//! before each [`macroquad::window::next_frame`]. Use the fallible method when
//! Scene callbacks can return errors; the convenience [`App::frame`] method
//! panics on those errors.
//!
//! ```no_run
//! use mue_macroquad::{App, Scene};
//!
//! struct Menu;
//! impl Scene for Menu {}
//!
//! async fn run() {
//!     let mut app = App::with_scene(Menu);
//!     loop {
//!         app.try_frame().expect("application frame failed");
//!         macroquad::window::next_frame().await;
//!     }
//! }
//! ```
//!
//! # Scene and input model
//!
//! A [`Scene`] owns its UI and application state. Only the stack's top Scene is
//! scheduled; covered Scenes remain retained. [`SceneEvent`] carries lifecycle
//! notifications and the complete raw [`input::InputBatch`]. Node pointer routing
//! is separate: it does not consume or filter the batch delivered to a Scene.
//! Stack operations from [`SceneContext`] are deferred to safe frame boundaries.
//!
//! Optional draw-only [Scene transitions](scene_transition) can animate those
//! deferred operations. [`transition::Transition`] is also the curve type used
//! by reactive style values; these property animations and Scene drawing effects
//! share curves, but have distinct scheduling and resource lifetimes.
//!
//! # Host rendering
//!
//! When combining an external renderer with mue, the host renders and flushes its
//! target, closes any raw pass, and restores the default camera before entering
//! the UI frame. [`RenderStateGuard`] restores the logical material, draw mode,
//! scissor, and viewport only; it does not manage camera, render pass, depth,
//! flushing, or model-matrix balance. [`node::portal()`] consumes a published
//! [`SharedTexture`] and does not run the external renderer or change passes.
//! Use [`SafeTexture`] when an owning texture handle may be dropped off the render
//! thread; call [`flush_pending_texture_deletions`] after drawing when not using
//! App's normal-frame cleanup.
//!
//! The higher-level component catalog is provided by `mue-components`; this crate
//! exposes renderer and node primitives without depending on that catalog.
//!
//! [macroquad]: https://docs.rs/macroquad

pub mod anchor;
mod app;
pub mod clip;
pub mod color;
pub mod cursor;
mod error;
pub mod event;
pub mod focus;
pub mod font;
pub mod gesture;
pub mod hook;
pub mod inherit;
pub mod input;
pub mod layer;
pub mod layout;
pub mod material;
pub mod math;
pub mod node;
pub mod paint;
pub mod path;
pub mod render;
pub mod runtime;
pub mod scene;
pub mod scene_transition;
pub mod shader;
pub mod shadow;
pub mod style;
pub mod svg;
pub mod texture;
pub mod transition;
mod viewport;

pub use app::App;
pub use clip::Sector;
pub use error::{
    AppError, BoxedError, CallbackError, CallbackResult, Result, SceneError, TransitionError,
};
pub use material::{UiMaterial, UniformValue, with_material};
pub use render::RenderStateGuard;
pub use scene::{
    RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRequest, SceneResult,
    SceneRoot,
};
pub use scene_transition::{
    ScenePictures, SceneTransition, TransitionFrame, TransitionPhase, TransitionTiming,
};
pub use shader::SharedTexture;
pub use shadow::{Shadow, ShadowCache};
pub use texture::{SafeTexture, flush_pending_texture_deletions};

/// Install a callback for waking a blocking host event loop when a task is woken.
pub fn set_task_wake_callback(callback: Option<std::sync::Arc<dyn Fn() + Send + Sync>>) {
    mue_core::task::set_wake_callback(callback);
}

pub use mue_macros::style;

#[doc(hidden)]
pub mod __private {
    pub use mue_core as core;
}
