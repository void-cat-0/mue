use super::easing::Ease;

#[derive(Debug)]
pub(super) struct Segment {
    pub from: f64,
    pub to: f64,
    pub start: f64,
    pub end: f64,
    pub ease: Ease,
}

impl Segment {
    pub fn linear(from: f64, to: f64, start: f64, end: f64) -> Self {
        Self {
            from,
            to,
            start,
            end,
            ease: Ease::Linear,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Track {
    events: Vec<Segment>,
    default: f64,
}

impl Track {
    pub fn new(mut events: Vec<Segment>, default: f64) -> Self {
        events.sort_by(|a, b| a.from.total_cmp(&b.from));
        Self { events, default }
    }

    pub fn at(&self, time: f64) -> f64 {
        let next = self.events.partition_point(|event| event.from <= time);
        if next == 0 {
            return self
                .events
                .first()
                .map_or(self.default, |event| event.start);
        }
        let event = &self.events[next - 1];
        if time >= event.to || event.start == event.end {
            return event.end;
        }
        let t = (time - event.from) / (event.to - event.from);
        event.start + (event.end - event.start) * event.ease.at(t)
    }
}

#[derive(Debug, Default)]
pub(super) struct Layer {
    pub x: Track,
    pub y: Track,
    pub angle: Track,
    pub opacity: Track,
}

#[derive(Debug)]
pub(super) struct SpeedTrack {
    spans: Vec<SpeedSpan>,
    initial_rate: f64,
    zero: f64,
}

#[derive(Debug)]
struct SpeedSpan {
    from: f64,
    to: f64,
    start_rate: f64,
    end_rate: f64,
    start_height: f64,
}

impl SpeedSpan {
    fn height(&self, time: f64) -> f64 {
        let elapsed = time - self.from;
        let duration = self.to - self.from;
        if elapsed >= duration {
            self.start_height
                + (self.start_rate + self.end_rate) * 0.5 * duration
                + self.end_rate * (elapsed - duration)
        } else {
            // Exact area of the trapezoid up to the requested time.
            let rate = self.start_rate + (self.end_rate - self.start_rate) * (elapsed / duration);
            self.start_height + (self.start_rate + rate) * 0.5 * elapsed
        }
    }
}

impl SpeedTrack {
    pub fn new(mut events: Vec<Segment>, initial_rate: f64) -> Self {
        events.sort_by(|a, b| a.from.total_cmp(&b.from));
        let mut spans: Vec<SpeedSpan> = Vec::with_capacity(events.len());
        for event in events {
            // A later event takes over at its start, including if events overlap.
            let start_height = spans.last().map_or(event.from * initial_rate, |previous| {
                previous.height(event.from)
            });
            spans.push(SpeedSpan {
                from: event.from,
                to: event.to.max(event.from),
                start_rate: event.start,
                end_rate: event.end,
                start_height,
            });
        }
        let mut track = Self {
            spans,
            initial_rate,
            zero: 0.,
        };
        track.zero = track.height(0.);
        track
    }

    pub fn height(&self, time: f64) -> f64 {
        let next = self.spans.partition_point(|span| span.from <= time);
        let absolute = if next == 0 {
            time * self.initial_rate
        } else {
            self.spans[next - 1].height(time)
        };
        absolute - self.zero
    }
}
