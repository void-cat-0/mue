//! Mouse cursor shapes chosen by the hovered node.
//!
//! The deepest hovered node with a cursor other than [`Cursor::Auto`] decides
//! the window cursor; with none, the platform default is shown. Touch input
//! ignores cursors.

use mue_core::prop::PropValue;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    /// Use the nearest ancestor's cursor, or the default arrow.
    #[default]
    Auto,
    Default,
    Pointer,
    Text,
    Crosshair,
    Move,
    NotAllowed,
    Wait,
    Help,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
}
impl PropValue for Cursor {}

impl Cursor {
    pub(crate) fn icon(self) -> miniquad::CursorIcon {
        use miniquad::CursorIcon as Icon;
        match self {
            Self::Auto | Self::Default => Icon::Default,
            Self::Pointer => Icon::Pointer,
            Self::Text => Icon::Text,
            Self::Crosshair => Icon::Crosshair,
            Self::Move => Icon::Move,
            Self::NotAllowed => Icon::NotAllowed,
            Self::Wait => Icon::Wait,
            Self::Help => Icon::Help,
            Self::EwResize => Icon::EWResize,
            Self::NsResize => Icon::NSResize,
            Self::NeswResize => Icon::NESWResize,
            Self::NwseResize => Icon::NWSEResize,
        }
    }
}
