use super::*;
use std::{
    future::poll_fn,
    sync::mpsc::{self, Receiver, SyncSender},
    task::Poll,
    thread::{self, ThreadId},
    time::Duration,
};

use mue_core::{
    OwnedScope,
    task::{block_on, tick},
};

const WAIT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(super) struct ReleaseNotice(SyncSender<()>);

impl Drop for ReleaseNotice {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct DecodeReport {
    worker_thread: ThreadId,
    worker_name: String,
    generation_id: u64,
    source_hash: u64,
}

/// Only this test module can construct a gate. It holds real decoded CPU data
/// before BlockingTask publication; production configuration has no gate/delay.
pub(super) struct DecodeGate {
    decoded: SyncSender<DecodeReport>,
    release: Receiver<()>,
    done: SyncSender<()>,
    discarded: SyncSender<()>,
}

impl DecodeGate {
    pub(super) fn wait_after_decode(
        &self,
        decoded: &mut std::result::Result<Option<Prepared>, Failure>,
    ) {
        let prepared = match decoded {
            Ok(Some(prepared)) => prepared,
            _ => panic!("gated fixture must decode a real resource"),
        };
        prepared.released = Some(ReleaseNotice(self.discarded.clone()));
        let _ = self.decoded.send(DecodeReport {
            worker_thread: thread::current().id(),
            worker_name: thread::current().name().unwrap_or_default().into(),
            generation_id: prepared.generation_id,
            source_hash: prepared.source_hash,
        });
        let _ = self.release.recv();
    }

    pub(super) fn worker_done(self) {
        let _ = self.done.send(());
    }
}

struct Gate {
    decoded: Receiver<DecodeReport>,
    release: Option<SyncSender<()>>,
    done: Receiver<()>,
    discarded: Receiver<()>,
    collected: bool,
}

impl Gate {
    fn report(&self) -> DecodeReport {
        self.decoded
            .recv_timeout(WAIT)
            .expect("CPU decode did not reach handoff")
    }

    fn complete(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if !self.collected {
            self.done
                .recv_timeout(WAIT)
                .expect("blocking CPU closure did not finish");
            self.collected = true;
        }
    }

    fn resource_discarded(&self) {
        self.discarded
            .recv_timeout(WAIT)
            .expect("decoded CPU resource was not released");
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        // Unwinding must also release the channel gate and collect its worker.
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if !self.collected {
            let _ = self.done.recv_timeout(WAIT);
        }
    }
}

fn gated_load(request: &mut Request, source: Vec<u8>) -> Gate {
    let (decoded_tx, decoded) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let (done_tx, done) = mpsc::sync_channel(1);
    let (discarded_tx, discarded) = mpsc::sync_channel(1);
    request.load_inner(
        source,
        LoadConfig::default(),
        Some(DecodeGate {
            decoded: decoded_tx,
            release: release_rx,
            done: done_tx,
            discarded: discarded_tx,
        }),
    );
    Gate {
        decoded,
        release: Some(release),
        done,
        discarded,
        collected: false,
    }
}

fn fixture(marker: u8) -> (Vec<u8>, Vec<u8>, Vec<Point>) {
    let rgba = vec![
        marker, 32, 64, 255, 96, 128, 160, 255, 192, 224, 255, 255, 16, 48, 80, 255,
    ];
    let points = vec![
        Point {
            at: 0.25,
            x: 0.1,
            y: 0.2,
            color: [marker, 40, 80, 255],
        },
        Point {
            at: 1.5,
            x: 0.5,
            y: 0.7,
            color: [60, 120, 180, 255],
        },
        Point {
            at: 3.0,
            x: 0.9,
            y: 0.4,
            color: [200, 100, 50, 255],
        },
    ];
    let mut source = b"MSF1".to_vec();
    source.extend_from_slice(&3.0_f32.to_le_bytes());
    source.extend_from_slice(&2_u16.to_le_bytes());
    source.extend_from_slice(&2_u16.to_le_bytes());
    source.extend_from_slice(&(points.len() as u32).to_le_bytes());
    source.extend_from_slice(&rgba);
    for point in &points {
        source.extend_from_slice(&point.at.to_le_bytes());
        source.extend_from_slice(&point.x.to_le_bytes());
        source.extend_from_slice(&point.y.to_le_bytes());
        source.extend_from_slice(&point.color);
    }
    (source, rgba, points)
}

fn hash(source: &[u8]) -> u64 {
    source.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn outcome(request: &mut Request) -> LoadOutcome {
    // Headless-only driver. Pending does not self-wake: core's actual blocking
    // completion wakes its foreground runnable; block_on parks between ticks.
    // No sleep, spinning, fake signals, pool, timer, or UI-thread block_on.
    block_on(poll_fn(|_| {
        request.update().map_or(Poll::Pending, Poll::Ready)
    }))
}

fn prepared(request: &mut Request) -> Rc<Prepared> {
    match outcome(request) {
        LoadOutcome::Ready(prepared) => prepared,
        other => panic!("expected source resource, got {other:?}"),
    }
}

fn session() -> Rc<Session> {
    let mut request = Request::new();
    request.load(fixture(7).0, LoadConfig::default());
    Rc::new(Session::new(prepared(&mut request)))
}

#[test]
fn real_cpu_decode_hands_source_resources_to_foreground_once() {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send::<Prepared>();
    assert_send_sync::<LoadMonitor>();

    let (source, rgba, points) = fixture(17);
    let expected_hash = hash(&source);
    let owner = OwnedScope::new();
    let mut request = Request::new();
    let mut gate = owner.scope().run(|| gated_load(&mut request, source));
    let monitor = request.monitor().unwrap();
    let report = gate.report(); // Actual worker runs before ANY foreground tick.
    assert_ne!(report.worker_thread, thread::current().id());
    assert!(report.worker_name.starts_with("mue-blocking-"));
    assert_eq!(report.source_hash, expected_hash);
    assert_eq!(report.generation_id, request.generation_id().unwrap());
    assert_eq!(monitor.progress(), 1.0);
    assert!(!monitor.is_finished());
    assert!(!monitor.is_accepted());
    assert!(request.update().is_none());

    gate.complete();
    assert!(request.is_finished());
    assert!(!monitor.is_accepted());
    let cpu = prepared(&mut request);
    assert_eq!(cpu.generation_id, report.generation_id);
    assert_eq!((cpu.width, cpu.height), (2, 2));
    assert_eq!(cpu.duration, 3.0);
    assert_eq!(cpu.rgba, rgba);
    assert_eq!(cpu.points, points);
    assert_eq!(cpu.source_hash, expected_hash);
    assert!(monitor.is_accepted());
    assert!(!monitor.is_cancelled());
    assert!(request.update().is_none());
    assert!(request.cancel().is_none());
    drop(cpu);
    gate.resource_discarded();
}

#[test]
fn failure_is_business_value_and_retry_decodes_real_source() {
    let mut request = Request::new();
    let (source, rgba, points) = fixture(23);
    let expected_hash = hash(&source);
    let failed_generation = request.load(
        source.clone(),
        LoadConfig {
            fail_after_decode: true,
        },
    );
    let failed_monitor = request.monitor().unwrap();
    match outcome(&mut request) {
        LoadOutcome::Failure(failure) => {
            assert_eq!(failure.generation_id, failed_generation);
            assert!(failure.message.contains("after CPU decode"));
        }
        other => panic!("expected business failure, got {other:?}"),
    }
    assert!(failed_monitor.is_finished());
    assert_eq!(failed_monitor.progress(), 1.0); // Failure AFTER genuine decoding.
    assert!(!failed_monitor.is_accepted());
    assert!(!failed_monitor.is_cancelled());
    assert!(request.update().is_none());

    let retry_generation = request.load(source, LoadConfig::default());
    assert_ne!(retry_generation, failed_generation);
    let cpu = prepared(&mut request);
    assert_eq!(cpu.generation_id, retry_generation);
    assert_eq!(cpu.source_hash, expected_hash);
    assert_eq!(cpu.rgba, rgba);
    assert_eq!(cpu.points, points);
    assert!(!failed_monitor.is_accepted());
}

#[test]
fn malformed_source_returns_failure_instead_of_task_fault() {
    let source = fixture(11).0;
    let mut cases = vec![Vec::new(), vec![0; MAX_SOURCE_BYTES + 1]];
    let mut add = |offset: usize, bytes: &[u8]| {
        let mut invalid = source.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(bytes);
        cases.push(invalid);
    };
    add(0, b"bad!");
    add(4, &f32::NAN.to_le_bytes());
    add(4, &0.0_f32.to_le_bytes());
    add(8, &0_u16.to_le_bytes());
    add(10, &257_u16.to_le_bytes());
    add(12, &0_u32.to_le_bytes());
    add(12, &4097_u32.to_le_bytes());
    add(48, &0.1_f32.to_le_bytes()); // Second point before the first.
    add(36, &f32::INFINITY.to_le_bytes());
    add(40, &1.1_f32.to_le_bytes());
    cases.push(source[..source.len() - 1].to_vec());
    let mut request = Request::new();
    for invalid in cases {
        let generation_id = request.load(invalid, LoadConfig::default());
        match outcome(&mut request) {
            LoadOutcome::Failure(failure) => {
                assert_eq!(failure.generation_id, generation_id);
                assert!(!failure.message.is_empty());
            }
            other => panic!("malformed source produced {other:?}"),
        }
        let monitor = request.monitor().unwrap();
        assert!(monitor.is_finished());
        assert!(!monitor.is_accepted());
    }
}

#[test]
fn cancel_after_decode_drops_publisher_and_rejects_late_resource() {
    let mut request = Request::new();
    let mut gate = gated_load(&mut request, fixture(31).0);
    let report = gate.report();
    let monitor = request.monitor().unwrap();
    tick(1); // Foreground await registers for a still-blocked, real CPU resource.
    assert!(!monitor.is_finished());
    assert_eq!(request.cancel(), Some(report.generation_id));
    assert!(monitor.is_cancelled());
    assert!(request.publisher.is_none());
    assert!(
        matches!(request.update(), Some(LoadOutcome::Cancelled { generation_id })
        if generation_id == report.generation_id)
    );
    assert!(request.cancel().is_none());
    assert!(request.update().is_none());

    gate.complete(); // A valid Prepared finishes AFTER cancellation.
    gate.resource_discarded(); // Not just a ready signal: actual Vec resource drops.
    assert!(monitor.is_finished());
    assert!(!monitor.is_accepted());
    tick(8);
    assert!(request.update().is_none());
}

#[test]
fn replacement_invalidates_old_generation_before_its_late_completion() {
    let mut request = Request::new();
    let mut old_gate = gated_load(&mut request, fixture(41).0);
    let old = old_gate.report();
    let old_monitor = request.monitor().unwrap();
    tick(1);
    let (new_source, new_rgba, _) = fixture(97);
    let new_hash = hash(&new_source);
    let mut new_gate = gated_load(&mut request, new_source);
    let new = new_gate.report();
    assert_ne!(new.generation_id, old.generation_id);
    assert_eq!(request.generation_id(), Some(new.generation_id));
    assert!(old_monitor.is_cancelled());
    assert!(!old_monitor.is_finished());

    new_gate.complete();
    let cpu = prepared(&mut request);
    assert_eq!(cpu.generation_id, new.generation_id);
    assert_eq!(cpu.source_hash, new_hash);
    assert_eq!(cpu.rgba, new_rgba);
    assert!(request.monitor().unwrap().is_accepted());
    old_gate.complete();
    old_gate.resource_discarded();
    tick(8);
    assert!(old_monitor.is_finished());
    assert!(!old_monitor.is_accepted());
    assert!(request.update().is_none());
    assert_eq!(request.generation_id(), Some(new.generation_id));
    drop(cpu);
    new_gate.resource_discarded();
}

#[test]
fn replacement_also_discards_already_published_but_unaccepted_resource() {
    let mut request = Request::new();
    let mut old_gate = gated_load(&mut request, fixture(103).0);
    let old = old_gate.report();
    let old_monitor = request.monitor().unwrap();
    old_gate.complete();
    // Drive the real foreground await to completion without taking its output.
    // The caller still owns that Ready Task, and normal update has not accepted it.
    block_on(poll_fn(|_| {
        if request.publisher.as_ref().unwrap().is_finished() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    assert!(old_monitor.is_finished());
    assert!(!old_monitor.is_accepted());
    let mut new_gate = gated_load(&mut request, fixture(107).0);
    let new = new_gate.report();
    old_gate.resource_discarded();
    assert!(old_monitor.is_cancelled());
    assert_ne!(old.generation_id, new.generation_id);
    assert!(request.update().is_none());
    new_gate.complete();
    let cpu = prepared(&mut request);
    assert_eq!(cpu.generation_id, new.generation_id);
    assert_eq!(cpu.rgba[0], 107);
    assert!(!old_monitor.is_accepted());
    drop(cpu);
    new_gate.resource_discarded();
}

#[test]
fn dropping_request_revokes_task_without_joining_blocked_worker() {
    let mut request = Request::new();
    let mut gate = gated_load(&mut request, fixture(53).0);
    gate.report();
    let monitor = request.monitor().unwrap();
    tick(1);
    drop(request); // Returns while the CPU worker is still channel-blocked.
    assert!(monitor.is_cancelled());
    assert!(!monitor.is_finished());
    gate.complete();
    gate.resource_discarded();
    assert!(monitor.is_finished());
    assert!(!monitor.is_accepted());
}

#[test]
fn creator_owned_scope_does_not_own_caller_task() {
    let owner = OwnedScope::new();
    let mut request = Request::new();
    let mut gate = owner
        .scope()
        .run(|| gated_load(&mut request, fixture(61).0));
    gate.report();
    let monitor = request.monitor().unwrap();
    drop(owner); // Ordinary caller-owned Task remains alive, unlike scoped wrappers.
    assert!(!monitor.is_cancelled());
    gate.complete();
    let cpu = prepared(&mut request);
    assert!(monitor.is_accepted());
    assert_eq!(cpu.generation_id, monitor.generation_id());
    drop(cpu);
    gate.resource_discarded();
}

#[test]
fn finished_but_unaccepted_cpu_work_can_still_be_cancelled() {
    let mut request = Request::new();
    let mut gate = gated_load(&mut request, fixture(67).0);
    let report = gate.report();
    let monitor = request.monitor().unwrap();
    gate.complete();
    // A suspended Scene may observe CPU completion but must not call update.
    for _ in 0..4 {
        assert!(request.is_finished());
        assert!(!monitor.is_accepted());
    }
    assert_eq!(request.cancel(), Some(report.generation_id));
    assert!(matches!(
        request.update(),
        Some(LoadOutcome::Cancelled { .. })
    ));
    gate.resource_discarded();
    assert!(monitor.is_cancelled());
    assert!(!monitor.is_accepted());
    assert!(request.update().is_none());
}

#[test]
fn ready_attempt_never_advances_until_explicit_start() {
    let session = session();
    assert!(session.update(60.0).is_none());
    assert!(session.update(0.0).is_none());
    assert!(!session.pause());
    assert!(!session.resume());
    assert!(session.exit().is_none());
    let snapshot = session.snapshot();
    assert_eq!(snapshot.state, SessionState::Ready);
    assert_eq!(snapshot.elapsed, 0.0);
    assert_eq!(snapshot.points_reached, 0);
    assert_eq!(
        (
            snapshot.start_count,
            snapshot.stop_count,
            snapshot.running_count
        ),
        (0, 0, 0)
    );
    assert!(session.start());
    assert!(!session.start());
    assert!(session.update(60.0).is_none()); // First activation is dt0.
    assert_eq!(session.snapshot().elapsed, 0.0);
    session.exit().unwrap();
}

#[test]
fn pause_resume_discards_gap_and_keeps_one_live_attempt() {
    let session = session();
    assert!(session.start());
    assert!(session.update(0.0).is_none());
    assert!(session.update(0.5).is_none());
    assert_eq!(session.snapshot().points_reached, 1);
    assert!(session.pause());
    assert!(!session.pause());
    assert!(!session.start());
    assert!(session.update(100.0).is_none());
    let paused = session.snapshot();
    assert_eq!(paused.state, SessionState::Paused);
    assert_eq!(paused.elapsed, 0.5);
    assert_eq!(
        (paused.start_count, paused.stop_count, paused.running_count),
        (1, 0, 1)
    );
    assert!(session.resume());
    assert!(!session.resume());
    assert!(session.update(100.0).is_none()); // Resume's first normal update is dt0.
    assert_eq!(session.snapshot().elapsed, 0.5);
    assert!(session.update(0.25).is_none());
    for dt in [-1.0, f32::NAN, f32::INFINITY, 0.0] {
        assert!(session.update(dt).is_none());
    }
    assert_eq!(session.snapshot().elapsed, 0.75);
    let result = session.exit().unwrap();
    assert_eq!(result.reason, EndReason::Exited);
    assert_eq!(session.snapshot().running_count, 0);
}

#[test]
fn natural_end_clamps_duration_and_stops_exactly_once() {
    let session = session();
    assert!(session.start());
    session.update(0.0);
    assert!(session.update(1.5).is_none());
    assert_eq!(session.snapshot().points_reached, 2);
    let result = session.update(100.0).unwrap();
    assert_eq!(result.reason, EndReason::Completed);
    assert_eq!(result.elapsed, 3.0);
    assert_eq!(result.duration, 3.0);
    assert_eq!(result.points_reached, 3);
    assert_eq!(result.attempt, 1);
    let stopped = session.snapshot();
    assert_eq!(stopped.state, SessionState::Stopped);
    assert_eq!(
        (
            stopped.start_count,
            stopped.stop_count,
            stopped.running_count
        ),
        (1, 1, 0)
    );
    assert_eq!(stopped.result.as_ref(), Some(&result));
    assert!(session.update(100.0).is_none());
    assert!(session.exit().is_none());
    assert!(!session.pause());
    assert!(!session.resume());
    assert_eq!(session.snapshot(), stopped);
}

#[test]
fn active_exit_stops_running_or_paused_attempt_exactly_once() {
    for paused in [false, true] {
        let session = session();
        session.start();
        session.update(0.0);
        session.update(0.5);
        if paused {
            session.pause();
        }
        let result = session.exit().unwrap();
        assert_eq!(result.reason, EndReason::Exited);
        assert_eq!(result.elapsed, 0.5);
        assert_eq!(result.points_reached, 1);
        assert!(session.exit().is_none());
        assert!(session.update(10.0).is_none());
        let snapshot = session.snapshot();
        assert_eq!(snapshot.state, SessionState::Stopped);
        assert_eq!(
            (
                snapshot.start_count,
                snapshot.stop_count,
                snapshot.running_count
            ),
            (1, 1, 0)
        );
    }
}

#[test]
fn retry_reuses_prepared_and_shared_session_context() {
    let session = session();
    let shared = session.clone();
    let cpu = session.prepared();
    session.start();
    session.update(0.0);
    let first = session.update(3.0).unwrap();
    assert_eq!(first.reason, EndReason::Completed);
    assert!(shared.start());
    assert!(Rc::ptr_eq(&session, &shared));
    assert!(Rc::ptr_eq(&cpu, &shared.prepared()));
    let retry = session.snapshot();
    assert_eq!(retry.generation_id, cpu.generation_id);
    assert_eq!(retry.elapsed, 0.0);
    assert_eq!(retry.points_reached, 0);
    assert_eq!(
        (retry.start_count, retry.stop_count, retry.running_count),
        (2, 1, 1)
    );
    assert!(retry.result.is_none());
    shared.update(50.0); // Retry start still consumes a dt0 activation frame.
    assert_eq!(session.snapshot().elapsed, 0.0);
    let second = session.exit().unwrap();
    assert_eq!(second.attempt, 2);
    assert_eq!(second.reason, EndReason::Exited);
    assert_eq!(first.attempt, 1); // The old owned return was not mutated by retry.
    assert_eq!(shared.snapshot().stop_count, 2);
    assert_eq!(shared.snapshot().running_count, 0);
}

#[test]
fn last_session_owner_releases_source_resource_without_detached_driver() {
    let mut request = Request::new();
    let mut gate = gated_load(&mut request, fixture(71).0);
    gate.report();
    gate.complete();
    let cpu = prepared(&mut request);
    let weak = Rc::downgrade(&cpu);
    let session = Rc::new(Session::new(cpu));
    let retained = session.clone();
    session.start();
    session.update(0.0);
    session.update(0.5);
    drop(session);
    assert!(weak.upgrade().is_some());
    assert_eq!(retained.snapshot().running_count, 1);
    retained.exit().unwrap();
    assert_eq!(retained.snapshot().running_count, 0);
    drop(retained);
    assert!(weak.upgrade().is_none());
    gate.resource_discarded();
}

#[test]
fn business_returns_are_owned_distinct_values_for_retained_details() {
    let session = session();
    session.start();
    let result = session.exit().unwrap();
    let boxed: Box<dyn std::any::Any> = Box::new(FlowResult::AttemptResult(result.clone()));
    let returned = *boxed.downcast::<FlowResult>().unwrap();
    assert_eq!(returned, FlowResult::AttemptResult(result));
    let failure = FlowResult::Failure(Failure {
        generation_id: 13,
        message: "bad asset".into(),
    });
    let cancelled = FlowResult::Cancelled { generation_id: 13 };
    assert_ne!(failure, cancelled);
    assert_ne!(returned, cancelled);
}
