//! CSS grid tracks and placement, lowered to Taffy's grid algorithm.
//!
//! Set `display: grid` (or `Display::Grid`) on a container, describe its
//! columns and rows with [`GridTemplate`], and optionally place children with
//! [`GridLine`]. Tracks accept pixels, percentages, `fr`, `auto`,
//! `min-content`, `max-content`, `fit-content(px)`, and `minmax(min, max)`.
//!
//! ```
//! use mue_macroquad::style::{GridLine, GridTemplate, GridTrack, Repeat};
//!
//! let cards = GridTemplate::new().repeat(Repeat::AutoFill, [GridTrack::minmax(
//!     GridTrack::px(160.),
//!     GridTrack::fr(1.),
//! )]);
//! let sidebar = GridTemplate::from([GridTrack::px(240.), GridTrack::fr(1.)]);
//! let header = GridLine::new(1, 3);
//! let wide = GridLine::span(2);
//! # let _ = (cards, sidebar, header, wide);
//! ```

use mue_core::prop::PropValue;
use taffy::style_helpers as helpers;
use taffy::{
    GridTemplateComponent, GridTemplateRepetition, MaxTrackSizingFunction, MinTrackSizingFunction,
    RepetitionCount, TrackSizingFunction,
};

pub use taffy::GridAutoFlow;

/// One grid track size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridTrack(pub(crate) TrackSizingFunction);

impl GridTrack {
    /// A fixed size in logical pixels.
    pub fn px(value: f32) -> Self {
        Self(helpers::length(value))
    }

    /// A percentage of the grid container's content size, from 0 to 100.
    pub fn percent(value: f32) -> Self {
        Self(helpers::percent(value / 100.))
    }

    /// A share of the remaining free space.
    pub fn fr(value: f32) -> Self {
        Self(helpers::fr(value))
    }

    pub fn auto() -> Self {
        Self(helpers::auto())
    }

    pub fn min_content() -> Self {
        Self(helpers::min_content())
    }

    pub fn max_content() -> Self {
        Self(helpers::max_content())
    }

    /// Size to the content, capped at `limit` pixels.
    pub fn fit_content(limit: f32) -> Self {
        Self(helpers::fit_content(taffy::LengthPercentage::length(limit)))
    }

    /// A track between `min` and `max`. The minimum uses the minimum sizing of
    /// `min` (a `fr` minimum behaves like `auto`); the maximum uses `max`.
    pub fn minmax(min: GridTrack, max: GridTrack) -> Self {
        Self(TrackSizingFunction {
            min: min.min_function(),
            max: max.0.max,
        })
    }

    fn min_function(self) -> MinTrackSizingFunction {
        self.0.min
    }

    /// The Taffy track sizing function.
    pub fn to_taffy(self) -> TrackSizingFunction {
        self.0
    }
}

impl From<MaxTrackSizingFunction> for GridTrack {
    fn from(max: MaxTrackSizingFunction) -> Self {
        Self(TrackSizingFunction {
            min: helpers::auto(),
            max,
        })
    }
}

/// How many times a `repeat(..)` group appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Count(u16),
    /// As many repetitions as fit, keeping empty tracks.
    AutoFill,
    /// As many repetitions as fit, collapsing empty tracks.
    AutoFit,
}

impl From<u16> for Repeat {
    fn from(count: u16) -> Self {
        Self::Count(count)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TemplateItem {
    Track(GridTrack),
    Repeat(Repeat, Vec<GridTrack>),
}

/// A `grid-template-columns` / `grid-template-rows` track list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTemplate(Vec<TemplateItem>);
impl PropValue for GridTemplate {}

impl GridTemplate {
    pub fn new() -> Self {
        Self::default()
    }

    /// `count` equal `1fr` tracks.
    pub fn equal(count: u16) -> Self {
        Self::new().repeat(count, [GridTrack::fr(1.)])
    }

    /// Append one track.
    pub fn track(mut self, track: GridTrack) -> Self {
        self.0.push(TemplateItem::Track(track));
        self
    }

    /// Append a `repeat(count, tracks...)` group.
    pub fn repeat(
        mut self,
        count: impl Into<Repeat>,
        tracks: impl IntoIterator<Item = GridTrack>,
    ) -> Self {
        self.0.push(TemplateItem::Repeat(
            count.into(),
            tracks.into_iter().collect(),
        ));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn to_taffy(&self) -> Vec<GridTemplateComponent<String>> {
        self.0
            .iter()
            .map(|item| match item {
                TemplateItem::Track(track) => GridTemplateComponent::Single(track.0),
                TemplateItem::Repeat(count, tracks) => {
                    GridTemplateComponent::Repeat(GridTemplateRepetition {
                        count: match *count {
                            Repeat::Count(count) => RepetitionCount::Count(count),
                            Repeat::AutoFill => RepetitionCount::AutoFill,
                            Repeat::AutoFit => RepetitionCount::AutoFit,
                        },
                        tracks: tracks.iter().map(|track| track.0).collect(),
                        line_names: Vec::new(),
                    })
                }
            })
            .collect()
    }
}

impl<const N: usize> From<[GridTrack; N]> for GridTemplate {
    fn from(tracks: [GridTrack; N]) -> Self {
        Self(tracks.into_iter().map(TemplateItem::Track).collect())
    }
}

impl From<Vec<GridTrack>> for GridTemplate {
    fn from(tracks: Vec<GridTrack>) -> Self {
        Self(tracks.into_iter().map(TemplateItem::Track).collect())
    }
}

/// Implicit track sizes for `grid-auto-rows` / `grid-auto-columns`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTracks(pub Vec<GridTrack>);
impl PropValue for GridTracks {}

impl<const N: usize> From<[GridTrack; N]> for GridTracks {
    fn from(tracks: [GridTrack; N]) -> Self {
        Self(tracks.into())
    }
}

impl From<GridTrack> for GridTracks {
    fn from(track: GridTrack) -> Self {
        Self(vec![track])
    }
}

/// One end of a grid item's placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridPlacement {
    #[default]
    Auto,
    /// A 1-based line number; negative numbers count from the end.
    Line(i16),
    /// Span this many tracks.
    Span(u16),
}

impl GridPlacement {
    fn to_taffy(self) -> taffy::GridPlacement<String> {
        match self {
            Self::Auto => taffy::GridPlacement::Auto,
            Self::Line(line) => helpers::line(line),
            Self::Span(span) => helpers::span(span),
        }
    }
}

/// A grid item's start and end along one axis (`grid-column` / `grid-row`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridLine {
    pub start: GridPlacement,
    pub end: GridPlacement,
}
impl PropValue for GridLine {}

impl GridLine {
    /// From line `start` to line `end` (CSS `start / end`).
    pub fn new(start: i16, end: i16) -> Self {
        Self {
            start: GridPlacement::Line(start),
            end: GridPlacement::Line(end),
        }
    }

    /// Auto-placed, spanning `tracks` tracks (CSS `span n`).
    pub fn span(tracks: u16) -> Self {
        Self {
            start: GridPlacement::Span(tracks),
            end: GridPlacement::Auto,
        }
    }

    /// Start at `line` and span `tracks` tracks (CSS `line / span n`).
    pub fn at(line: i16, tracks: u16) -> Self {
        Self {
            start: GridPlacement::Line(line),
            end: GridPlacement::Span(tracks),
        }
    }

    pub(crate) fn to_taffy(self) -> taffy::Line<taffy::GridPlacement<String>> {
        taffy::Line {
            start: self.start.to_taffy(),
            end: self.end.to_taffy(),
        }
    }
}
