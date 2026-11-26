//! Application entry point for mue, a retained, reactive UI toolkit for Macroquad.
//!
//! Import [`prelude`] for common builders, reactive values, and authoring macros.
//! Use [`renderer`] for the full renderer API and [`macroquad`] for the host
//! window and frame loop. Enable the `i18n` Cargo feature for Fluent localization.
//!
//! Start with the [your first window](guide::learn::first_window) or browse all
//! [application guides](guide). The project is hosted on [GitHub](https://github.com/void-cat-0/mue).
//!
//! ```no_run
//! use mue::{App, prelude::*};
//!
//! #[component]
//! fn greeting() -> impl Component {
//!     center()
//!         .style(style! { width: 100%; height: 100%; })
//!         .children(text("Hello, mue!"))
//! }
//!
//! # fn after_macroquad_initializes() {
//! let mut app = App::new(greeting());
//! app.frame();
//! # }
//! ```

/// The Macroquad crate used by the renderer and the host application.
pub use macroquad;
/// The layout, typography, graphics, and application token components.
pub use mue_components as components;
/// The reactive runtime, scopes, properties, and foreground tasks.
pub use mue_core as core;
/// The Macroquad renderer, node primitives, and Scene runtime.
pub use mue_macroquad as renderer;

#[cfg(feature = "i18n")]
/// Fluent catalogs and reactive translations.
pub use mue_i18n as i18n;

pub use mue_macroquad::{App, Scene};
pub use mue_macros::{component, node, style};

pub mod guide;

/// Common imports for application UI and custom components.
pub mod prelude {
    pub use crate::components::{button, center, heading, hstack, stack, vstack};
    pub use crate::core::{
        effect::computed,
        prop::Prop,
        scope::{OwnedScope, inject, provide},
        signal::{Access, ReadSignal, Signal, signal},
    };
    pub use crate::renderer::{
        cursor::Cursor,
        focus::FocusScope,
        node::{
            Component, IntoNode, Node, NodeContext, NodeHandle, NodeRef, flexbox, path, svg, text,
        },
        path::{PathData, StrokeStyle},
        presence::presence,
        style::{Style, Styleable, StyleableExt},
        svg::SvgDocument,
    };
    pub use crate::{component, node, style};
}
