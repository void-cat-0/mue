mod details;
mod library;
mod settings;

use std::rc::Rc;

pub use library::LibraryScene;
pub use settings::SettingsScene;

/// A narrow navigation capability: read-only consumers can open the editor Scene.
pub type SettingsFactory = Rc<dyn Fn() -> SettingsScene>;
