#![doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!("../../docs/index.md"))]

// Rustdoc resolves these definitions before the Markdown fallback paths.
macro_rules! guide {
    ($module:ident, $file:literal) => {
        #[doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!($file))]
        pub mod $module {}
    };
    ($module:ident, $file:literal, illustrated) => {
        #[cfg_attr(not(doc), doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!($file)))]
        #[cfg_attr(doc, doc = concat!(
            include_str!("guide-links.md"), "\n\n",
            include_str!("../../docs/images/embedded.md"), "\n\n",
            include_str!("../../docs/demo-style.md"), "\n\n",
            include_str!($file)
        ))]
        pub mod $module {}
    };
}

#[doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!("../../docs/learn/index.md"))]
pub mod learn {
    guide!(first_window, "../../docs/learn/first-step.md", illustrated);
    guide!(reactivity, "../../docs/learn/basics.md");
    guide!(styling, "../../docs/learn/style.md", illustrated);
    guide!(view_builders, "../../docs/learn/components.md", illustrated);
    guide!(theme_context, "../../docs/learn/themes.md", illustrated);
    guide!(navigation, "../../docs/learn/scenes.md", illustrated);
    guide!(input_routing, "../../docs/learn/input.md");
    guide!(async_work, "../../docs/learn/tasks.md", illustrated);
    guide!(localization, "../../docs/learn/i18n.md", illustrated);
}

#[doc = concat!(include_str!("guide-links.md"), "\n\n", include_str!("../../docs/reference/index.md"))]
pub mod reference {
    guide!(
        view_authoring,
        "../../docs/reference/components-and-node.md"
    );
    guide!(
        reactive_runtime,
        "../../docs/reference/state-children-and-scope.md"
    );
    guide!(
        rendering,
        "../../docs/reference/paint-mask-and-textures.md",
        illustrated
    );
    guide!(style_syntax, "../../docs/reference/style.md");
    guide!(focus_keyboard, "../../docs/reference/focus-and-keyboard.md");
    guide!(
        layers_overlays,
        "../../docs/reference/layers-and-overlays.md"
    );
    guide!(
        component_conventions,
        "../../docs/reference/component-conventions.md"
    );
    guide!(scene_runtime, "../../docs/reference/scene.md", illustrated);
    guide!(theme_store, "../../docs/reference/themes-and-tokens.md");
    guide!(executor, "../../docs/reference/tasks.md");
    guide!(fluent, "../../docs/reference/i18n.md");
    guide!(examples, "../../docs/reference/examples.md");
}

guide!(example_app, "../../docs/example-app.md", illustrated);

// Preserve the guide's historical module paths outside the course contents.
#[doc(hidden)]
pub use learn::{
    async_work as tasks, first_window as getting_started, localization as i18n,
    navigation as scenes, navigation as scene_transitions, styling as style,
    styling as transitions, theme_context as themes, view_builders as components,
};
#[doc(hidden)]
pub use reference::examples;
