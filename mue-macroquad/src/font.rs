//! Loading fonts into the shared font system.
//!
//! Every `text` node shapes against one process-wide `FontSystem`, so a font file only has
//! to be registered once. Both loaders return the families the file provides; pass one of them
//! to `text(..).font_family(..)` to use it.
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

// The `text` node speaks in cosmic-text types (`font_family`, `font_weight`), so re-export the
// ones a caller has to name instead of forcing every caller to add a cosmic-text dependency.
pub use cosmic_text::{FamilyOwned, Weight};

pub fn load_font(data: Vec<u8>) -> Vec<FamilyOwned> {
    Runtime::with_fonts_mut(|fonts| fonts.load_font_data(data))
        .into_iter()
        .map(|name| FamilyOwned::new(Family::Name(&name)))
        .collect()
}

pub fn load_font_file(path: impl AsRef<Path>) -> std::io::Result<Vec<FamilyOwned>> {
    Ok(load_font(std::fs::read(path)?))
}
