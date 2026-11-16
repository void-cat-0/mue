mod app;
pub mod clip;
pub mod color;
pub mod event;
pub mod font;
pub mod gesture;
pub mod hook;
pub mod inherit;
pub mod input;
pub mod layout;
pub mod material;
pub mod math;
pub mod node;
pub mod paint;
pub mod render;
pub mod runtime;
pub mod scene;
pub mod scene_transition;
pub mod shader;
pub mod shadow;
pub mod style;
pub mod texture;
pub mod transition;
mod viewport;

pub use app::App;
pub use clip::Sector;
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
pub use shadow::Shadow;
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
