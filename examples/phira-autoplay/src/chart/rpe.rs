use serde::Deserialize;

use super::easing::Ease;
use super::track::{Layer, Segment, SpeedTrack, Track};
use super::{Chart, ChartError, FLOOR_SCALE, Format, Kind, Line, Note, positive_bpm};

const WIDTH: f64 = 1350.;
const HEIGHT: f64 = 900.;
const SPEED_SCALE: f64 = (10. / 45.) * FLOOR_SCALE;

#[derive(Deserialize)]
pub(super) struct Meta {
    #[serde(rename = "RPEVersion")]
    pub version: u32,
    #[serde(default)]
    offset: f64,
}

#[derive(Deserialize)]
struct RawChart {
    #[serde(rename = "BPMList")]
    tempos: Vec<RawTempo>,
    #[serde(rename = "judgeLineList")]
    lines: Vec<RawLine>,
}

#[derive(Clone, Copy, Deserialize)]
struct Beat([i64; 3]);

impl Beat {
    fn value(self) -> Result<f64, ChartError> {
        let [whole, numerator, denominator] = self.0;
        if denominator <= 0 {
            return Err(ChartError::Beat(denominator));
        }
        Ok(whole as f64 + numerator as f64 / denominator as f64)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTempo {
    bpm: f64,
    start_time: Beat,
}

struct Tempos(Vec<Tempo>);

struct Tempo {
    beat: f64,
    bpm: f64,
    time: f64,
}

impl Tempos {
    fn new(raw: Vec<RawTempo>) -> Result<Self, ChartError> {
        if raw.is_empty() {
            return Err(ChartError::NoBpm);
        }
        let mut tempos = raw
            .into_iter()
            .map(|raw| {
                Ok(Tempo {
                    beat: raw.start_time.value()?,
                    bpm: positive_bpm(raw.bpm)?,
                    time: 0.,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        tempos.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for i in 1..tempos.len() {
            let previous = &tempos[i - 1];
            tempos[i].time = previous.time + (tempos[i].beat - previous.beat) * 60. / previous.bpm;
        }
        let mut map = Self(tempos);
        let zero = map.seconds(0.);
        for tempo in &mut map.0 {
            tempo.time -= zero;
        }
        Ok(map)
    }

    fn at(&self, beat: f64) -> &Tempo {
        &self.0[self
            .0
            .partition_point(|tempo| tempo.beat <= beat)
            .saturating_sub(1)]
    }

    fn seconds(&self, beat: f64) -> f64 {
        let tempo = self.at(beat);
        tempo.time + (beat - tempo.beat) * 60. / tempo.bpm
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLine {
    event_layers: Vec<Option<RawLayer>>,
    notes: Option<Vec<RawNote>>,
    #[serde(default = "one")]
    bpmfactor: f64,
    #[serde(default = "enabled")]
    is_cover: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLayer {
    move_x_events: Option<Vec<RawEvent>>,
    move_y_events: Option<Vec<RawEvent>>,
    rotate_events: Option<Vec<RawEvent>>,
    alpha_events: Option<Vec<RawEvent>>,
    speed_events: Option<Vec<RawEvent>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    start_time: Beat,
    end_time: Beat,
    start: f64,
    end: f64,
    #[serde(default)]
    easing_type: u32,
    #[serde(default)]
    easing_left: f64,
    #[serde(default = "one")]
    easing_right: f64,
    #[serde(default)]
    bezier: u32,
    bezier_points: Option<[f64; 4]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNote {
    #[serde(rename = "type")]
    kind: u32,
    start_time: Beat,
    end_time: Beat,
    #[serde(default = "enabled")]
    above: u32,
    #[serde(default = "opaque")]
    alpha: f64,
    #[serde(default)]
    is_fake: u32,
    position_x: f64,
    #[serde(default = "one")]
    speed: f64,
    #[serde(default = "one")]
    size: f64,
    #[serde(default = "infinite")]
    visible_time: f64,
    #[serde(default)]
    y_offset: f64,
}

fn one() -> f64 {
    1.
}
fn enabled() -> u32 {
    1
}
fn opaque() -> f64 {
    255.
}
fn infinite() -> f64 {
    f64::INFINITY
}

pub(super) fn parse(
    bytes: &[u8],
    meta: Meta,
) -> Result<Chart, Box<dyn std::error::Error + Send + Sync>> {
    let raw: RawChart = serde_json::from_slice(bytes)?;
    let tempos = Tempos::new(raw.tempos)?;
    let lines = raw
        .lines
        .into_iter()
        .map(|line| line.convert(&tempos))
        .collect::<Result<_, _>>()?;
    Ok(Chart::new(
        Format::Rpe,
        meta.offset / 1000.,
        lines,
        tempos.at(0.).bpm,
    ))
}

impl RawLine {
    fn convert(self, tempos: &Tempos) -> Result<Line, ChartError> {
        let factor = positive_bpm(self.bpmfactor)?;
        let at = |beat: Beat| Ok(tempos.seconds(beat.value()? * factor));
        let mut line = Line {
            notes: Vec::new(),
            cover: self.is_cover != 0,
            layers: Vec::new(),
            scroll: Vec::new(),
        };
        for raw in self.event_layers.into_iter().flatten() {
            line.layers.push(Layer {
                x: Track::new(project(raw.move_x_events, &at, 1. / WIDTH, true)?, 0.),
                y: Track::new(project(raw.move_y_events, &at, -1. / HEIGHT, true)?, 0.),
                angle: Track::new(
                    project(raw.rotate_events, &at, 1_f64.to_radians(), true)?,
                    0.,
                ),
                opacity: Track::new(project(raw.alpha_events, &at, 1. / 255., true)?, 0.),
            });
            // RPE 113 speeds are linear ramps, independently integrated per layer.
            let speeds = project(raw.speed_events, &at, SPEED_SCALE, false)?;
            let initial_rate = speeds
                .iter()
                .min_by(|a, b| a.from.total_cmp(&b.from))
                .map_or(0., |event| event.start);
            line.scroll.push(SpeedTrack::new(speeds, initial_rate));
        }
        line.notes = self
            .notes
            .unwrap_or_default()
            .into_iter()
            .map(|raw| {
                let kind = match raw.kind {
                    1 => Kind::Tap,
                    2 => Kind::Hold,
                    3 => Kind::Flick,
                    4 => Kind::Drag,
                    n => return Err(ChartError::NoteType("RPE", n)),
                };
                let time = at(raw.start_time)?;
                let end = if kind == Kind::Hold {
                    at(raw.end_time)?.max(time)
                } else {
                    time
                };
                Ok(Note {
                    time,
                    end,
                    height: line.height(time),
                    tail_height: line.height(end),
                    speed: raw.speed,
                    x: (raw.position_x / WIDTH) as f32,
                    y_offset: raw.y_offset / HEIGHT * raw.speed,
                    above: raw.above == 1,
                    kind,
                    size: raw.size as f32,
                    alpha: (raw.alpha / 255.) as f32,
                    visible_time: raw.visible_time,
                    fake: raw.is_fake != 0,
                    highlight: false,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(line)
    }
}

fn project(
    events: Option<Vec<RawEvent>>,
    at: &impl Fn(Beat) -> Result<f64, ChartError>,
    scale: f64,
    eased: bool,
) -> Result<Vec<Segment>, ChartError> {
    events
        .unwrap_or_default()
        .into_iter()
        .map(|event| {
            let ease = match (eased, event.bezier, event.bezier_points) {
                (true, 1, Some(points)) => Ease::Bezier(points),
                (true, _, _) if event.easing_type > 1 => {
                    let (left, right) = if event.easing_left < event.easing_right {
                        (
                            event.easing_left.clamp(0., 1.),
                            event.easing_right.clamp(0., 1.),
                        )
                    } else {
                        (0., 1.)
                    };
                    Ease::Rpe {
                        kind: event.easing_type,
                        left,
                        right,
                    }
                }
                _ => Ease::Linear,
            };
            Ok(Segment {
                from: at(event.start_time)?,
                to: at(event.end_time)?,
                start: event.start * scale,
                end: event.end * scale,
                ease,
            })
        })
        .collect()
}
