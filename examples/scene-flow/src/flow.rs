//! CPU-only loading and an explicitly started, shareable attempt model.
//!
//! The Scene owns its ordinary `OwnedScope`, this `Request`, and any GPU
//! resources. Loading starts one `spawn_blocking` worker immediately; a normal
//! caller-owned foreground `Task` awaits it. Only a normal Scene update should
//! call `Request::update`, upload `Prepared::rgba`, and then start a Session.
//! No scope, reactive handle, Rc, GPU object, clock, or Window enters the worker.

use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use mue_core::task::{Task, spawn, spawn_blocking};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
const HEADER_BYTES: usize = 16;
const POINT_BYTES: usize = 16;
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_SIDE: u16 = 256;
const MAX_POINTS: usize = 4096;

fn next_generation() -> u64 {
    let mut generation = NEXT_GENERATION.load(Ordering::Relaxed);
    loop {
        let next = generation
            .checked_add(1)
            .expect("request generation space exhausted");
        match NEXT_GENERATION.compare_exchange_weak(
            generation,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return generation,
            Err(current) => generation = current,
        }
    }
}

/// The bundled MSF1 source consists of a 16-byte little-endian header
/// (magic, duration:f32, width:u16, height:u16, point_count:u32), raw RGBA8 pixels,
/// and point_count records (at:f32, x:f32, y:f32, color:[u8;4]). Coordinates are
/// normalized; point times are sorted and lie within the positive duration.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub at: f32,
    pub x: f32,
    pub y: f32,
    pub color: [u8; 4],
}

/// Owned, validated CPU data. Rc wrapping happens only after the foreground
/// await, and its generation identifies the GPU upload the Scene may reuse.
#[derive(Debug)]
pub struct Prepared {
    pub generation_id: u64,
    pub duration: f32,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
    pub points: Vec<Point>,
    pub source_hash: u64,
    #[cfg(test)]
    released: Option<tests::ReleaseNotice>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LoadConfig {
    /// An ordinary business failure after actual source validation/decoding.
    /// This is useful for the example's failure/retry controls, not a Task fault.
    pub fail_after_decode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub generation_id: u64,
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (request {})", self.message, self.generation_id)
    }
}

impl std::error::Error for Failure {}

#[derive(Debug)]
pub enum LoadOutcome {
    Ready(Rc<Prepared>),
    Failure(Failure),
    Cancelled { generation_id: u64 },
}

/// Thread-safe, read-only observation that can outlive a Request. Finished means
/// CPU work has ended, NOT that its output has been accepted, uploaded, or run.
/// A cancelled monitor can finish late but can never become accepted.
#[derive(Debug)]
pub struct LoadMonitor {
    generation_id: u64,
    source_bytes: usize,
    completed_bytes: AtomicUsize,
    cancelled: AtomicBool,
    finished: AtomicBool,
    accepted: AtomicBool,
}

impl LoadMonitor {
    pub fn generation_id(&self) -> u64 {
        self.generation_id
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted.load(Ordering::Acquire)
    }

    pub fn progress(&self) -> f32 {
        let total = self.source_bytes.max(1);
        self.completed_bytes.load(Ordering::Relaxed).min(total) as f32 / total as f32
    }
}

/// The only foreground loading owner. Replacement and cancellation invalidate
/// the old generation before dropping its publisher. BlockingTask drop cannot
/// interrupt an OS thread; the atomic token enables cooperative parser exits,
/// and both Task drop and the generation check reject non-cooperative late work.
#[derive(Default)]
pub struct Request {
    generation_id: Option<u64>,
    publisher: Option<Task<LoadOutcome>>,
    monitor: Option<Arc<LoadMonitor>>,
    pending_cancel: Option<u64>,
}

impl Request {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start actual CPU work now, without waiting for the first foreground tick.
    pub fn load(&mut self, source: Vec<u8>, config: LoadConfig) -> u64 {
        self.load_inner(
            source,
            config,
            #[cfg(test)]
            None,
        )
    }

    fn load_inner(
        &mut self,
        source: Vec<u8>,
        config: LoadConfig,
        #[cfg(test)] gate: Option<tests::DecodeGate>,
    ) -> u64 {
        self.invalidate();
        self.pending_cancel = None;
        let generation_id = next_generation();
        let monitor = Arc::new(LoadMonitor {
            generation_id,
            source_bytes: source.len(),
            completed_bytes: AtomicUsize::new(0),
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            accepted: AtomicBool::new(false),
        });
        let worker_monitor = monitor.clone();
        let blocking = spawn_blocking(move || {
            #[allow(unused_mut)]
            let mut decoded = decode(source, generation_id, config, &worker_monitor);
            // Test-only channels hold an already decoded real resource at the
            // handoff boundary, allowing cancellation/replacement without sleeps.
            #[cfg(test)]
            if let Some(gate) = &gate {
                gate.wait_after_decode(&mut decoded);
            }
            worker_monitor.finished.store(true, Ordering::Release);
            #[cfg(test)]
            if let Some(gate) = gate {
                gate.worker_done();
            }
            decoded
        });
        let publish_monitor = monitor.clone();
        self.publisher = Some(spawn(async move {
            let decoded = blocking.await;
            if publish_monitor.is_cancelled() {
                return LoadOutcome::Cancelled { generation_id };
            }
            match decoded {
                Ok(Some(prepared)) => LoadOutcome::Ready(Rc::new(prepared)),
                Ok(None) => LoadOutcome::Cancelled { generation_id },
                Err(failure) => LoadOutcome::Failure(failure),
            }
        }));
        self.generation_id = Some(generation_id);
        self.monitor = Some(monitor);
        generation_id
    }

    pub fn generation_id(&self) -> Option<u64> {
        self.generation_id
    }

    /// Observe CPU completion without consuming a Task or handing off resources.
    /// Foreground task ticks are still required before update can yield Ready.
    pub fn is_finished(&self) -> bool {
        self.monitor
            .as_ref()
            .is_some_and(|monitor| monitor.is_finished())
    }

    pub fn monitor(&self) -> Option<Arc<LoadMonitor>> {
        self.monitor.clone()
    }

    /// Consume once, only during a normal Scene update. This never uploads GPU
    /// data and never starts a Session; Ready merely hands over the CPU resource.
    pub fn update(&mut self) -> Option<LoadOutcome> {
        if let Some(generation_id) = self.pending_cancel.take() {
            return Some(LoadOutcome::Cancelled { generation_id });
        }
        let outcome = self.publisher.as_mut()?.try_take()?;
        self.publisher.take();
        let monitor = self.monitor.as_ref()?;
        let generation_id = match &outcome {
            LoadOutcome::Ready(prepared) => prepared.generation_id,
            LoadOutcome::Failure(failure) => failure.generation_id,
            LoadOutcome::Cancelled { generation_id } => *generation_id,
        };
        if self.generation_id != Some(generation_id) || monitor.is_cancelled() {
            // Normally Task drop already prevents this path. Retain the check
            // here: resource identity is validated at the actual handoff boundary.
            return None;
        }
        if matches!(&outcome, LoadOutcome::Ready(_)) {
            monitor.accepted.store(true, Ordering::Release);
        }
        Some(outcome)
    }

    /// Immediately revoke a pending request and drop its caller-owned Task.
    /// The next normal update reports Cancelled once. An already accepted CPU
    /// resource belongs to its recipient and cannot be cancelled retroactively.
    pub fn cancel(&mut self) -> Option<u64> {
        self.publisher.as_ref()?;
        let generation_id = self.generation_id?;
        self.invalidate();
        self.pending_cancel = Some(generation_id);
        Some(generation_id)
    }

    fn invalidate(&mut self) {
        if self.publisher.is_some()
            && let Some(monitor) = &self.monitor
        {
            monitor.cancelled.store(true, Ordering::Release);
        }
        // On the owner thread this immediately destroys the foreground future,
        // including its BlockingTask observer. No worker captures this Request.
        self.publisher.take();
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        self.invalidate();
    }
}

fn decode(
    source: Vec<u8>,
    generation_id: u64,
    config: LoadConfig,
    monitor: &LoadMonitor,
) -> std::result::Result<Option<Prepared>, Failure> {
    let fail = |message: &str| Failure {
        generation_id,
        message: message.into(),
    };
    if monitor.is_cancelled() {
        return Ok(None);
    }
    if source.len() < HEADER_BYTES || source.len() > MAX_SOURCE_BYTES {
        return Err(fail("source size is outside the MSF1 asset limits"));
    }
    if &source[..4] != b"MSF1" {
        return Err(fail("source is not an MSF1 asset"));
    }
    let f32_at = |offset: usize| f32::from_le_bytes(source[offset..offset + 4].try_into().unwrap());
    let duration = f32_at(4);
    let width = u16::from_le_bytes(source[8..10].try_into().unwrap());
    let height = u16::from_le_bytes(source[10..12].try_into().unwrap());
    let point_count = u32::from_le_bytes(source[12..16].try_into().unwrap()) as usize;
    if !duration.is_finite() || !(0.0..=600.0).contains(&duration) || duration == 0.0 {
        return Err(fail("duration must be finite and in (0, 600] seconds"));
    }
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(fail("texture dimensions must be in 1..=256"));
    }
    if point_count == 0 || point_count > MAX_POINTS {
        return Err(fail("point count must be in 1..=4096"));
    }
    let pixel_bytes = usize::from(width) * usize::from(height) * 4;
    // All operands are bounded above before these additions, even on 32-bit hosts.
    let expected_bytes = HEADER_BYTES + pixel_bytes + point_count * POINT_BYTES;
    if source.len() != expected_bytes {
        return Err(fail(
            "asset length does not match its pixels and point records",
        ));
    }
    monitor
        .completed_bytes
        .store(HEADER_BYTES, Ordering::Relaxed);
    let mut rgba = Vec::with_capacity(pixel_bytes);
    for chunk in source[HEADER_BYTES..HEADER_BYTES + pixel_bytes].chunks(1024) {
        if monitor.is_cancelled() {
            return Ok(None);
        }
        rgba.extend_from_slice(chunk);
        monitor
            .completed_bytes
            .fetch_add(chunk.len(), Ordering::Relaxed);
    }
    let mut points = Vec::with_capacity(point_count);
    let mut previous_at = 0.0;
    for index in 0..point_count {
        if monitor.is_cancelled() {
            return Ok(None);
        }
        let offset = HEADER_BYTES + pixel_bytes + index * POINT_BYTES;
        let at = f32_at(offset);
        let x = f32_at(offset + 4);
        let y = f32_at(offset + 8);
        if !at.is_finite() || at < previous_at || at < 0.0 || at > duration {
            return Err(fail(
                "point times must be finite, ordered, and within the duration",
            ));
        }
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Err(fail("point coordinates must be finite and normalized"));
        }
        points.push(Point {
            at,
            x,
            y,
            color: source[offset + 12..offset + 16].try_into().unwrap(),
        });
        previous_at = at;
        monitor
            .completed_bytes
            .fetch_add(POINT_BYTES, Ordering::Relaxed);
    }
    // A source-derived identity computed in the CPU worker, rather than a script
    // that merely sends ready signals. Pixels and trajectories above also come
    // from the supplied source and are the data consumed by the real Scene.
    let mut source_hash = 0xcbf29ce484222325_u64;
    for chunk in source.chunks(1024) {
        if monitor.is_cancelled() {
            return Ok(None);
        }
        for byte in chunk {
            source_hash = (source_hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    if config.fail_after_decode {
        return Err(fail("requested business failure after CPU decode"));
    }
    Ok(Some(Prepared {
        generation_id,
        duration,
        width,
        height,
        rgba,
        points,
        source_hash,
        #[cfg(test)]
        released: None,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    Running,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Completed,
    Exited,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttemptResult {
    pub generation_id: u64,
    pub attempt: u64,
    pub reason: EndReason,
    pub elapsed: f32,
    pub duration: f32,
    pub points_reached: usize,
}

/// An owned business return, suitable for a SceneResult downcast in retained
/// details. Failure and cancellation are values, not errors of the host App.
#[derive(Clone, Debug, PartialEq)]
pub enum FlowResult {
    AttemptResult(AttemptResult),
    Failure(Failure),
    Cancelled { generation_id: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionSnapshot {
    pub generation_id: u64,
    pub state: SessionState,
    pub elapsed: f32,
    pub duration: f32,
    pub attempt: u64,
    pub points_reached: usize,
    pub start_count: u64,
    pub stop_count: u64,
    /// Started-but-not-stopped attempts: 0 or 1. A paused attempt remains active;
    /// pause/resume changes permission to advance, not resource/session ownership.
    pub running_count: u64,
    pub result: Option<AttemptResult>,
}

struct Attempt {
    state: SessionState,
    elapsed: f32,
    points_reached: usize,
    start_count: u64,
    stop_count: u64,
    discard_next_dt: bool,
    result: Option<AttemptResult>,
}

/// Put this in Rc to retain one retry context and one Rc<Prepared> across results.
/// This model has no GPU resource; the Scene retains its matching GPU upload and
/// starts the attempt only after that upload succeeds during a normal update.
pub struct Session {
    prepared: Rc<Prepared>,
    attempt: RefCell<Attempt>,
}

impl Session {
    pub fn new(prepared: Rc<Prepared>) -> Self {
        Self {
            prepared,
            attempt: RefCell::new(Attempt {
                state: SessionState::Ready,
                elapsed: 0.0,
                points_reached: 0,
                start_count: 0,
                stop_count: 0,
                discard_next_dt: false,
                result: None,
            }),
        }
    }

    pub fn prepared(&self) -> Rc<Prepared> {
        self.prepared.clone()
    }

    /// Explicit first start or retry. Never resets an already live/paused attempt.
    pub fn start(&self) -> bool {
        let mut attempt = self.attempt.borrow_mut();
        if matches!(attempt.state, SessionState::Running | SessionState::Paused) {
            return false;
        }
        attempt.start_count = attempt
            .start_count
            .checked_add(1)
            .expect("attempt count exhausted");
        attempt.state = SessionState::Running;
        attempt.elapsed = 0.0;
        attempt.points_reached = 0;
        attempt.discard_next_dt = true;
        attempt.result = None;
        true
    }

    pub fn pause(&self) -> bool {
        let mut attempt = self.attempt.borrow_mut();
        if attempt.state != SessionState::Running {
            return false;
        }
        attempt.state = SessionState::Paused;
        true
    }

    pub fn resume(&self) -> bool {
        let mut attempt = self.attempt.borrow_mut();
        if attempt.state != SessionState::Paused {
            return false;
        }
        attempt.state = SessionState::Running;
        attempt.discard_next_dt = true;
        true
    }

    /// dt belongs to ordinary foreground updates, not host wall time. First
    /// start/resume update is dt0 even if a caller accidentally supplies a gap.
    /// Ready, paused, and stopped attempts never advance. Natural end emits once.
    pub fn update(&self, dt: f32) -> Option<AttemptResult> {
        let mut attempt = self.attempt.borrow_mut();
        if attempt.state != SessionState::Running {
            return None;
        }
        if attempt.discard_next_dt {
            attempt.discard_next_dt = false;
            return None;
        }
        if !dt.is_finite() || dt <= 0.0 {
            return None;
        }
        let remaining = self.prepared.duration - attempt.elapsed;
        attempt.elapsed = (attempt.elapsed + dt.min(remaining)).min(self.prepared.duration);
        attempt.points_reached = self
            .prepared
            .points
            .partition_point(|point| point.at <= attempt.elapsed);
        if attempt.elapsed >= self.prepared.duration {
            return Some(self.stop(&mut attempt, EndReason::Completed));
        }
        None
    }

    /// Stop a running or paused session explicitly before a Scene returns/pops.
    /// Exiting before start or exiting twice cannot manufacture an attempt result.
    pub fn exit(&self) -> Option<AttemptResult> {
        let mut attempt = self.attempt.borrow_mut();
        if !matches!(attempt.state, SessionState::Running | SessionState::Paused) {
            return None;
        }
        Some(self.stop(&mut attempt, EndReason::Exited))
    }

    fn stop(&self, attempt: &mut Attempt, reason: EndReason) -> AttemptResult {
        attempt.state = SessionState::Stopped;
        attempt.stop_count += 1;
        attempt.discard_next_dt = false;
        let result = AttemptResult {
            generation_id: self.prepared.generation_id,
            attempt: attempt.start_count,
            reason,
            elapsed: attempt.elapsed,
            duration: self.prepared.duration,
            points_reached: attempt.points_reached,
        };
        attempt.result = Some(result.clone());
        result
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let attempt = self.attempt.borrow();
        SessionSnapshot {
            generation_id: self.prepared.generation_id,
            state: attempt.state,
            elapsed: attempt.elapsed,
            duration: self.prepared.duration,
            attempt: attempt.start_count,
            points_reached: attempt.points_reached,
            start_count: attempt.start_count,
            stop_count: attempt.stop_count,
            running_count: attempt.start_count - attempt.stop_count,
            result: attempt.result.clone(),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Last-owner fallback: normal Scene exit uses exit() and returns its value.
        // There is no detached driver/timer capable of running after this drop.
        self.exit();
    }
}

#[cfg(test)]
#[path = "flow/tests.rs"]
mod tests;
