//! Decode chart JSON into the playfield's coordinate and time units.
mod easing;
mod pgr;
mod rpe;
mod track;

#[cfg(test)]
mod tests;

use std::{error::Error, fmt};

use serde::Deserialize;
use thiserror::Error;

use track::{Layer, SpeedTrack};

// Phira scroll distances span a playfield two units high before normalization.
const FLOOR_SCALE: f64 = 1. / (2. * 0.83175);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Rpe,
    Pgr,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rpe => "RPE 113",
            Self::Pgr => "PGR v3",
        })
    }
}

#[derive(Debug)]
pub struct Chart {
    pub format: Format,
    /// Subtract from the playback clock to obtain chart time, in seconds.
    pub offset: f64,
    pub lines: Vec<Line>,
    pub hits: Vec<Hit>,
    pub last_time: f64,
    pub bpm: f64,
}

#[derive(Debug)]
pub struct Line {
    pub notes: Vec<Note>,
    pub cover: bool,
    layers: Vec<Layer>,
    scroll: Vec<SpeedTrack>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub angle: f32,
    pub opacity: f32,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Tap,
    Drag,
    Hold,
    Flick,
}

#[derive(Debug)]
pub struct Note {
    pub time: f64,
    pub end: f64,
    pub height: f64,
    pub tail_height: f64,
    pub speed: f64,
    pub x: f32,
    pub y_offset: f64,
    pub above: bool,
    pub kind: Kind,
    pub size: f32,
    pub alpha: f32,
    pub visible_time: f64,
    pub fake: bool,
    pub highlight: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub time: f64,
    pub line: usize,
    pub note: usize,
}

#[derive(Debug, Error)]
enum ChartError {
    #[error("expected RPE or PGR chart JSON")]
    UnknownFormat,
    #[error("unsupported {0} chart version {1}")]
    Version(&'static str, u32),
    #[error("invalid BPM: {0}")]
    Bpm(f64),
    #[error("RPE chart has no BPM entries")]
    NoBpm,
    #[error("invalid RPE beat denominator: {0}")]
    Beat(i64),
    #[error("invalid {0} note type: {1}")]
    NoteType(&'static str, u32),
}

fn positive_bpm(bpm: f64) -> Result<f64, ChartError> {
    if bpm > 0. && bpm.is_finite() {
        Ok(bpm)
    } else {
        Err(ChartError::Bpm(bpm))
    }
}

impl Chart {
    pub fn parse(bytes: &[u8]) -> Result<Chart, Box<dyn Error + Send + Sync>> {
        #[derive(Deserialize)]
        struct Header {
            #[serde(rename = "META")]
            meta: Option<rpe::Meta>,
            #[serde(rename = "formatVersion")]
            format_version: Option<u32>,
        }

        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        let header: Header = serde_json::from_slice(bytes)?;
        if let Some(meta) = header.meta {
            if meta.version != 113 {
                return Err(ChartError::Version("RPE", meta.version).into());
            }
            return rpe::parse(bytes, meta);
        }
        match header.format_version {
            Some(3) => pgr::parse(bytes),
            Some(version) => Err(ChartError::Version("PGR", version).into()),
            None => Err(ChartError::UnknownFormat.into()),
        }
    }

    fn new(format: Format, offset: f64, mut lines: Vec<Line>, bpm: f64) -> Self {
        let mut hits = Vec::new();
        let mut last_time: Option<f64> = None;
        for (line, data) in lines.iter().enumerate() {
            for (note, n) in data.notes.iter().enumerate() {
                last_time = Some(last_time.map_or(n.end, |latest| latest.max(n.end)));
                if !n.fake {
                    hits.push(Hit {
                        time: n.time,
                        line,
                        note,
                    });
                }
            }
        }
        hits.sort_by(|a, b| a.time.total_cmp(&b.time));
        // One small tolerance covers floating-point differences between lines' BPM maps.
        let mut groups = hits.as_slice();
        while let Some((first, _)) = groups.split_first() {
            let count = groups.partition_point(|hit| hit.time - first.time <= 1e-7);
            let (chord, rest) = groups.split_at(count);
            if count > 1 {
                for hit in chord {
                    lines[hit.line].notes[hit.note].highlight = true;
                }
            }
            groups = rest;
        }
        Self {
            format,
            offset,
            lines,
            hits,
            last_time: last_time.unwrap_or(0.),
            bpm,
        }
    }
}

impl Line {
    pub fn sample(&self, time: f64) -> Pose {
        let mut pose = Pose {
            height: self.height(time),
            ..Pose::default()
        };
        for layer in &self.layers {
            pose.x += layer.x.at(time) as f32;
            pose.y += layer.y.at(time) as f32;
            pose.angle += layer.angle.at(time) as f32;
            pose.opacity += layer.opacity.at(time) as f32;
        }
        pose
    }

    fn height(&self, time: f64) -> f64 {
        self.scroll.iter().map(|track| track.height(time)).sum()
    }
}
