//! Loading fonts into the shared font system.
//!
//! Every `text` node on a thread shapes against one thread-local `FontSystem`, so a font file
//! only has to be registered once per thread. Both loaders return the families the file provides;
//! pass one of them to `text(..).font_family(..)` to use it.
//!
//! To supply a custom locale and preloaded database, call [`initialize`] on the rendering thread
//! before loading fonts or creating text nodes. Otherwise the first font access lazily creates
//! the default system font database. Initialization is one-shot per thread; other runtime access
//! does not initialize fonts.
//!
//! ```no_run
//! use mue_macroquad::{font::load_font_file, node::*, style::Styleable};
//!
//! let family = load_font_file("assets/Inter.ttf").unwrap().remove(0);
//! let _label = text("hello").font_family(family);
//! ```
//!
//! A family can also be named without loading anything: either a name the system font database
//! knows, or one of cosmic-text's generic families. The `&str` / `String` conversions come
//! from mue-core's `impl-cosmic-text` feature.
//!
//! ```no_run
//! use mue_macroquad::{font::FamilyOwned, node::*, style::Styleable};
//!
//! let _by_str = text("hello").font_family("Microsoft YaHei");
//! let _by_name = text("hello").font_family(FamilyOwned::Name("Microsoft YaHei".into()));
//! let _generic = text("hello").font_family(FamilyOwned::Monospace);
//! ```
//!
//! The parameter is reactive, so a family can follow a signal:
//!
//! ```no_run
//! use mue_core::prelude::*;
//! use mue_macroquad::{font::FamilyOwned, node::*, style::Styleable};
//!
//! let family = signal(FamilyOwned::Monospace);
//! let _reactive = text("hello").font_family(computed(move |_| family.get_clone()));
//! ```

use std::path::Path;

use cosmic_text::Family;

use crate::runtime::Runtime;

pub use crate::node::typography::GlyphAtlasStats;

// The `text` node speaks in cosmic-text types (`font_family`, `font_weight`), so re-export the
// ones a caller has to name instead of forcing every caller to add a cosmic-text dependency.
pub use cosmic_text::{FamilyOwned, Weight, fontdb};

/// Fonts have already been initialized or accessed on the current thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontsAlreadyInitialized;

impl std::fmt::Display for FontsAlreadyInitialized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("fonts have already been initialized or accessed on this thread")
    }
}

impl std::error::Error for FontsAlreadyInitialized {}

/// Initialize the current thread's font system with a locale and preloaded database.
///
/// Call this on the rendering thread before loading fonts or creating any text nodes.
/// Fonts are thread-local, so initialization on another thread has no effect here.
/// Without explicit initialization, the first font access loads the default system fonts.
/// Other runtime access does not initialize fonts.
///
/// The database is passed to the font system's constructor so its caches include the preloaded
/// faces, including monospace fonts. Existing font state is never replaced.
///
/// # Errors
///
/// Returns [`FontsAlreadyInitialized`] if fonts have already been initialized or accessed on
/// this thread, including through [`load_font`], [`load_font_file`], or a text node.
pub fn initialize(
    locale: impl Into<String>,
    db: fontdb::Database,
) -> Result<(), FontsAlreadyInitialized> {
    Runtime::initialize_fonts(locale.into(), db)
}

pub fn load_font(data: Vec<u8>) -> Vec<FamilyOwned> {
    Runtime::with_fonts_mut(|fonts| fonts.load_font_data(data))
        .into_iter()
        .map(|name| FamilyOwned::new(Family::Name(&name)))
        .collect()
}

pub fn load_font_file(path: impl AsRef<Path>) -> std::io::Result<Vec<FamilyOwned>> {
    Ok(load_font(std::fs::read(path)?))
}

/// Counter snapshot of the thread's glyph atlas, or `None` until text renders.
///
/// Useful for spotting thrash: an `evicted` count that keeps climbing means the
/// visible glyph set no longer fits the atlas and rasters are recreated every
/// frame.
pub fn glyph_atlas_stats() -> Option<GlyphAtlasStats> {
    Runtime::with_fonts(|fonts| fonts.glyph_atlas_stats()).flatten()
}
