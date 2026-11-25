use serde::Deserialize;

use super::track::{Layer, Segment, SpeedTrack, Track};
use super::{Chart, ChartError, FLOOR_SCALE, Format, Kind, Line, Note, positive_bpm};

const NOTE_X_SCALE: f64 = 0.9 / 16.;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChart {
    #[serde(default)]
    offset: f64,
    judge_line_list: Vec<RawLine>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLine {
    bpm: f64,
    #[serde(default)]
    notes_above: Vec<RawNote>,
    #[serde(default)]
    notes_below: Vec<RawNote>,
    #[serde(default)]
    speed_events: Vec<RawSpeed>,
    #[serde(default)]
    judge_line_move_events: Vec<RawEvent>,
    #[serde(default)]
    judge_line_rotate_events: Vec<RawEvent>,
    #[serde(default)]
    judge_line_disappear_events: Vec<RawEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNote {
    #[serde(rename = "type")]
    kind: u32,
    time: f64,
    #[serde(default)]
    hold_time: f64,
    position_x: f64,
    speed: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSpeed {
    start_time: f64,
    end_time: f64,
    value: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    start_time: f64,
    end_time: f64,
    start: f64,
    end: f64,
    #[serde(default)]
    start2: f64,
    #[serde(default)]
    end2: f64,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Chart, Box<dyn std::error::Error + Send + Sync>> {
    let raw: RawChart = serde_json::from_slice(bytes)?;
    let bpm = raw.judge_line_list.first().map_or(0., |line| line.bpm);
    let lines = raw
        .judge_line_list
        .into_iter()
        .map(RawLine::convert)
        .collect::<Result<_, _>>()?;
    Ok(Chart::new(Format::Pgr, raw.offset, lines, bpm))
}

impl RawLine {
    fn convert(self) -> Result<Line, ChartError> {
        let tick = 60. / (32. * positive_bpm(self.bpm)?);
        let layer = Layer {
            x: project(&self.judge_line_move_events, tick, 0., |e| {
                (e.start - 0.5, e.end - 0.5)
            }),
            y: project(&self.judge_line_move_events, tick, 0., |e| {
                (0.5 - e.start2, 0.5 - e.end2)
            }),
            angle: project(&self.judge_line_rotate_events, tick, 0., |e| {
                (-e.start.to_radians(), -e.end.to_radians())
            }),
            opacity: project(&self.judge_line_disappear_events, tick, 1., |e| {
                (e.start, e.end)
            }),
        };
        let speeds: Vec<_> = self
            .speed_events
            .into_iter()
            .map(|event| {
                let rate = event.value * FLOOR_SCALE;
                Segment::linear(event.start_time * tick, event.end_time * tick, rate, rate)
            })
            .collect();
        let initial_rate = speeds
            .iter()
            .min_by(|a, b| a.from.total_cmp(&b.from))
            .map_or(0., |event| event.start);
        let scroll = SpeedTrack::new(speeds, initial_rate);
        let notes = self
            .notes_above
            .into_iter()
            .map(|note| note.convert(tick, true, &scroll))
            .chain(
                self.notes_below
                    .into_iter()
                    .map(|note| note.convert(tick, false, &scroll)),
            )
            .collect::<Result<_, _>>()?;
        Ok(Line {
            notes,
            cover: true,
            layers: vec![layer],
            scroll: vec![scroll],
        })
    }
}

impl RawNote {
    fn convert(self, tick: f64, above: bool, scroll: &SpeedTrack) -> Result<Note, ChartError> {
        let kind = match self.kind {
            1 => Kind::Tap,
            2 => Kind::Drag,
            3 => Kind::Hold,
            4 => Kind::Flick,
            n => return Err(ChartError::NoteType("PGR", n)),
        };
        let time = self.time * tick;
        let end = if kind == Kind::Hold {
            time + self.hold_time.max(0.) * tick
        } else {
            time
        };
        // Use the line's integral even when authored note floor/speed disagree.
        Ok(Note {
            time,
            end,
            height: scroll.height(time),
            tail_height: scroll.height(end),
            speed: if kind == Kind::Hold { 1. } else { self.speed },
            x: (self.position_x * NOTE_X_SCALE) as f32,
            y_offset: 0.,
            above,
            kind,
            size: 1.,
            alpha: 1.,
            visible_time: f64::INFINITY,
            fake: false,
            highlight: false,
        })
    }
}

fn project(
    events: &[RawEvent],
    tick: f64,
    default: f64,
    values: impl Fn(&RawEvent) -> (f64, f64),
) -> Track {
    Track::new(
        events
            .iter()
            .map(|event| {
                let (start, end) = values(event);
                Segment::linear(event.start_time * tick, event.end_time * tick, start, end)
            })
            .collect(),
        default,
    )
}
