//! Application guides for mue. The [crate root](crate) indexes the API reference.
//!
//! - [Getting started] — add the Git dependency and run a first UI.
//! - [Components and custom nodes] — build reusable UI and retain state.
//! - [Reusable component declarations] — share builder logic across views.
//! - [Styles and units] — use typed declarations and responsive values.
//! - [Property transitions] — animate reactive style values.
//! - [Scenes and input] — organize screens, input, and rendering.
//! - [Scene transitions] — animate stack changes.
//! - [Application tokens] — share reactive design values and scope context.
//! - [Foreground tasks] — schedule async work and publish results.
//! - [Fluent localization] — load catalogs and build reactive translations.
//! - [Examples] — explore complete applications.
//!
//! [Getting started]: getting_started
//! [Components and custom nodes]: components
//! [Reusable component declarations]: factories
//! [Styles and units]: style
//! [Property transitions]: transitions
//! [Scenes and input]: scenes
//! [Scene transitions]: scene_transitions
//! [Application tokens]: themes
//! [Foreground tasks]: tasks
//! [Fluent localization]: i18n
//! [Examples]: examples

// Put the Rustdoc link definitions first so they take precedence over the
// fallback links in the Markdown files when Cargo builds the guide pages.
macro_rules! guide {
    ($module:ident, $file:literal) => {
        #[doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!($file))]
        pub mod $module {}
    };
}

guide!(getting_started, "../../docs/getting-started.md");
guide!(components, "../../docs/components.md");
guide!(examples, "../../docs/examples.md");
guide!(factories, "../../docs/factories.md");
guide!(i18n, "../../docs/i18n.md");
guide!(scene_transitions, "../../docs/scene-transitions.md");
guide!(scenes, "../../docs/scenes.md");
guide!(style, "../../docs/style.md");
guide!(tasks, "../../docs/tasks.md");
guide!(themes, "../../docs/themes.md");
guide!(transitions, "../../docs/transitions.md");
