use std::{cell::RefCell, rc::Rc};

use super::{
    PhaseClock, PictureCache, SceneTransition, TransitionFrame, TransitionPhase, TransitionTiming,
    picture::target_pixels,
};
use crate::{math::Rect, transition::Transition};

fn viewport() -> Rect {
    Rect::new(0., 0., 640., 360.)
}

fn present(clock: &mut PhaseClock, dt: f32) -> TransitionFrame {
    let frame = clock.frame(dt, viewport());
    clock.did_present();
    frame
}

#[test]
fn constructors_and_unused_drop_are_gpu_free() {
    let fade = SceneTransition::crossfade(Transition::ease(0.2));
    assert!(fade.is_composite());
    assert_eq!(
        fade.timing(),
        TransitionTiming::AfterSwitch(Transition::ease(0.2))
    );
    let custom = SceneTransition::composite(Transition::linear(1.), |_, _| Ok(()));
    assert!(custom.is_composite());
    let overlay =
        SceneTransition::overlay(TransitionTiming::AfterSwitch(Transition::Instant), |_| {
            Ok(())
        });
    assert!(!overlay.is_composite());
    drop((
        fade,
        custom,
        overlay,
        PictureCache::new(),
        PictureCache::headless(),
    ));
}

#[test]
fn every_noninstant_segment_first_draw_is_zero() {
    let mut clock = PhaseClock::new(TransitionTiming::AroundSwitch {
        exit: Transition::ease_in(1.),
        enter: Transition::ease_out(2.),
    });
    let first = present(&mut clock, 100.);
    assert_eq!(first.phase, TransitionPhase::BeforeSwitch);
    assert_eq!(
        (
            first.linear_progress,
            first.progress,
            first.dt,
            first.elapsed
        ),
        (0., 0., 0., 0.)
    );
    present(&mut clock, 1.);
    assert!(clock.ready_to_switch());
    clock.after_switch().unwrap();
    let first_enter = present(&mut clock, 100.);
    assert_eq!(first_enter.phase, TransitionPhase::AfterSwitch);
    assert_eq!(
        (
            first_enter.linear_progress,
            first_enter.progress,
            first_enter.dt,
            first_enter.elapsed
        ),
        (0., 0., 0., 0.)
    );
    assert!(!clock.is_finished());
    assert_eq!(present(&mut clock, 1.).linear_progress, 0.5);
}

#[test]
fn endpoint_must_be_acknowledged_before_switch_or_completion() {
    let mut clock = PhaseClock::new(TransitionTiming::AroundSwitch {
        exit: Transition::linear(1.),
        enter: Transition::linear(1.),
    });
    assert!(clock.after_switch().is_err());
    present(&mut clock, 0.);
    let endpoint = clock.frame(10., viewport());
    assert_eq!(endpoint.linear_progress, 1.);
    assert!(!clock.ready_to_switch());
    assert!(clock.after_switch().is_err());
    clock.did_present();
    assert!(clock.ready_to_switch());
    assert!(!clock.is_finished());
    clock.after_switch().unwrap();
    assert!(!clock.ready_to_switch());
    assert!(clock.after_switch().is_err());
    present(&mut clock, 10.); // Enter's first draw is still zero.
    assert_eq!(clock.frame(10., viewport()).linear_progress, 1.);
    assert!(!clock.is_finished());
    clock.did_present();
    assert!(clock.is_finished());
}

#[test]
fn instant_and_invalid_durations_still_require_a_presented_endpoint() {
    let curves = [
        Transition::Instant,
        Transition::linear(0.),
        Transition::linear(-1.),
        Transition::linear(f32::NAN),
        Transition::ease(f32::INFINITY),
        Transition::spring(f32::NEG_INFINITY, 1.),
    ];
    for curve in curves {
        let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(curve));
        let frame = clock.frame(f32::MAX, viewport());
        assert_eq!(
            (
                frame.linear_progress,
                frame.progress,
                frame.elapsed,
                frame.dt
            ),
            (1., 1., 0., 0.)
        );
        assert!(!clock.is_finished());
        clock.did_present();
        assert!(clock.is_finished());
    }
}

#[test]
fn instant_exit_does_not_skip_instant_enter_draw() {
    let mut clock = PhaseClock::new(TransitionTiming::AroundSwitch {
        exit: Transition::Instant,
        enter: Transition::Instant,
    });
    assert_eq!(clock.frame(1., viewport()).linear_progress, 1.);
    assert!(!clock.ready_to_switch());
    clock.did_present();
    clock.after_switch().unwrap();
    assert!(!clock.is_finished());
    assert_eq!(clock.frame(1., viewport()).linear_progress, 1.);
    assert!(!clock.is_finished());
    clock.did_present();
    assert!(clock.is_finished());
}

#[test]
fn only_finite_positive_delta_advances_and_elapsed_is_capped() {
    let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(Transition::linear(1.)));
    present(&mut clock, 0.);
    assert_eq!(present(&mut clock, 0.25).elapsed, 0.25);
    for dt in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let frame = present(&mut clock, dt);
        assert_eq!(
            (frame.elapsed, frame.dt, frame.linear_progress),
            (0.25, 0., 0.25)
        );
    }
    assert_eq!(present(&mut clock, f32::MAX).elapsed, 1.);
    assert!(clock.is_finished());
    assert_eq!(present(&mut clock, 10.).dt, 0.);

    let mut huge = PhaseClock::new(TransitionTiming::AfterSwitch(Transition::linear(f32::MAX)));
    present(&mut huge, 0.);
    present(&mut huge, f32::MAX / 2.);
    let endpoint = present(&mut huge, f32::MAX);
    assert!(endpoint.elapsed.is_finite());
    assert_eq!(endpoint.linear_progress, 1.);
}

#[test]
fn failed_or_repeated_candidates_do_not_commit_time() {
    let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(Transition::linear(1.)));
    assert_eq!(clock.frame(0.5, viewport()).linear_progress, 0.);
    assert_eq!(clock.frame(0.5, viewport()).linear_progress, 0.);
    clock.did_present();
    assert_eq!(clock.frame(0.25, viewport()).linear_progress, 0.25);
    assert_eq!(clock.frame(0.25, viewport()).linear_progress, 0.25);
    // The .25 candidate was not drawn: a resumed proposal uses only its dt=0.
    assert_eq!(clock.frame(0., viewport()).linear_progress, 0.);
    clock.did_present();
    clock.did_present(); // Idempotent; no phantom frame or progress.
    assert_eq!(clock.frame(0.25, viewport()).linear_progress, 0.25);
}

#[test]
fn ease_and_spring_are_sampled_but_raw_controls_completion() {
    let curve = Transition::ease_in(1.);
    let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(curve));
    present(&mut clock, 0.);
    let frame = present(&mut clock, 0.5);
    assert_eq!(frame.linear_progress, 0.5);
    assert_eq!(frame.progress, curve.sample(0.5));
    assert_eq!(frame.progress, 0.25);

    let spring = Transition::spring(1., 1.);
    let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(spring));
    present(&mut clock, 0.);
    let frame = present(&mut clock, 0.1);
    assert_eq!(frame.progress, spring.sample(frame.linear_progress));
    assert!(frame.progress > 1.);
    assert!(frame.linear_progress < 1.);
    assert!(!clock.is_finished());
    let endpoint = present(&mut clock, 1.);
    assert_eq!((endpoint.linear_progress, endpoint.progress), (1., 1.));
    assert!(clock.is_finished());
}

#[test]
fn host_pause_and_resize_do_not_restart_or_advance_clock() {
    let mut clock = PhaseClock::new(TransitionTiming::AfterSwitch(Transition::linear(1.)));
    let mut pictures = PictureCache::headless();
    present(&mut clock, 0.);
    assert_eq!(present(&mut clock, 0.4).linear_progress, 0.4);
    assert!(pictures.begin_source(viewport()).unwrap().is_none());
    pictures.finish_source();
    pictures.invalidate(); // No reference to PhaseClock; pause calls no frame.
    let resized = Rect::new(0., 0., 800., 600.);
    let resumed = clock.frame(0., resized);
    assert_eq!(
        (resumed.linear_progress, resumed.elapsed, resumed.dt),
        (0.4, 0.4, 0.)
    );
    assert_eq!(resumed.viewport, resized);
    clock.did_present();
    assert_eq!(present(&mut clock, 0.1).linear_progress, 0.5);
}

#[test]
fn headless_overlay_schedules_both_endpoints_without_gl() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let output = log.clone();
    let mut transition = SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(1.),
            enter: Transition::linear(1.),
        },
        move |frame| {
            output
                .borrow_mut()
                .push((frame.phase, frame.linear_progress));
            Ok(())
        },
    );
    let mut pictures = PictureCache::headless();
    let mut clock = PhaseClock::new(transition.timing());
    for dt in [100., 1.] {
        let frame = clock.frame(dt, viewport());
        pictures.draw_overlay(frame, &mut transition).unwrap();
        clock.did_present();
    }
    assert!(clock.ready_to_switch());
    clock.after_switch().unwrap();
    for dt in [100., 1.] {
        let frame = clock.frame(dt, viewport());
        pictures.draw_overlay(frame, &mut transition).unwrap();
        clock.did_present();
    }
    assert!(clock.is_finished());
    assert_eq!(
        &*log.borrow(),
        &[
            (TransitionPhase::BeforeSwitch, 0.),
            (TransitionPhase::BeforeSwitch, 1.),
            (TransitionPhase::AfterSwitch, 0.),
            (TransitionPhase::AfterSwitch, 1.),
        ]
    );
}

#[test]
fn overlay_does_not_validate_or_allocate_a_picture_viewport() {
    let mut transition =
        SceneTransition::overlay(TransitionTiming::AfterSwitch(Transition::Instant), |_| {
            Ok(())
        });
    let mut clock = PhaseClock::new(transition.timing());
    let mut pictures = PictureCache::headless();
    let frame = clock.frame(0., Rect::new(20., 30., 0., 0.));
    pictures.draw_overlay(frame, &mut transition).unwrap();
    clock.did_present();
    assert!(clock.is_finished());
}

#[test]
fn overlay_error_is_not_a_presented_endpoint() {
    let mut transition =
        SceneTransition::overlay(TransitionTiming::AfterSwitch(Transition::Instant), |_| {
            anyhow::bail!("expected callback failure")
        });
    let mut clock = PhaseClock::new(transition.timing());
    let mut pictures = PictureCache::headless();
    let frame = clock.frame(0., viewport());
    assert_eq!(frame.linear_progress, 1.);
    assert!(pictures.draw_overlay(frame, &mut transition).is_err());
    pictures.cleanup_scene_error(); // Headless Scene error cleanup must not call GL.
    pictures.finish();
    pictures.invalidate();
    assert!(!clock.is_finished());
}

#[test]
fn headless_capture_is_exclusive_and_abort_does_not_commit_source() {
    let mut pictures = PictureCache::headless();
    assert!(pictures.needs_source(viewport()).unwrap());
    assert!(pictures.begin_source(viewport()).unwrap().is_none());
    assert!(pictures.begin_live(viewport()).is_err());
    pictures.finish();
    pictures.finish_source(); // Abort is not a finished source.
    assert!(pictures.needs_source(viewport()).unwrap());
    pictures.begin_source(viewport()).unwrap();
    pictures.finish_source();
    assert!(!pictures.needs_source(viewport()).unwrap());
    assert!(!pictures.freeze_presented(viewport()).unwrap());
    pictures.begin_live(viewport()).unwrap();
    pictures.finish_live();
    let mut transition = SceneTransition::crossfade(Transition::linear(1.));
    let mut clock = PhaseClock::new(transition.timing());
    let frame = clock.frame(0., viewport());
    let error = pictures.compose(frame, &mut transition).unwrap_err();
    assert!(error.to_string().contains("headless"));
    let resized = Rect::new(0., 0., 800., 600.);
    assert!(pictures.needs_source(resized).unwrap());
    pictures.begin_live(resized).unwrap();
    pictures.finish_live();
    assert!(pictures.needs_source(resized).unwrap());
    pictures.invalidate();
    pictures.finish();
}

#[test]
fn wrong_draw_kind_errors_before_any_gl_access() {
    let mut pictures = PictureCache::headless();
    let mut fade = SceneTransition::crossfade(Transition::Instant);
    let mut clock = PhaseClock::new(fade.timing());
    let frame = clock.frame(0., viewport());
    assert!(pictures.draw_overlay(frame, &mut fade).is_err());
    assert!(fade.draw_overlay(frame).is_err());
    let mut overlay = SceneTransition::overlay(fade.timing(), |_| Ok(()));
    assert!(pictures.compose(frame, &mut overlay).is_err());
}

#[test]
fn picture_pixels_round_high_dpi_and_keep_subpixels_nonzero() {
    assert_eq!(target_pixels(viewport(), 2.).unwrap(), (1280, 720));
    assert_eq!(
        target_pixels(Rect::new(0., 0., 320.25, 180.5), 2.).unwrap(),
        (641, 361)
    );
    assert_eq!(
        target_pixels(Rect::new(0., 0., 0.1, 0.2), 0.5).unwrap(),
        (1, 1)
    );
}

#[test]
fn picture_pixels_reject_invalid_viewport_and_dpi() {
    for rect in [
        Rect::new(1., 0., 1., 1.),
        Rect::new(0., 1., 1., 1.),
        Rect::new(f32::NAN, 0., 1., 1.),
        Rect::new(0., f32::INFINITY, 1., 1.),
        Rect::new(0., 0., 0., 1.),
        Rect::new(0., 0., 1., -1.),
        Rect::new(0., 0., f32::NAN, 1.),
        Rect::new(0., 0., 1., f32::INFINITY),
    ] {
        assert!(target_pixels(rect, 1.).is_err());
    }
    for dpi in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(target_pixels(viewport(), dpi).is_err());
    }
}

#[test]
fn picture_pixels_reject_overflow_without_float_boundary_roundup() {
    assert!(target_pixels(Rect::new(0., 0., f32::MAX, 1.), f32::MAX).is_err());
    assert!(target_pixels(Rect::new(0., 0., i32::MAX as f32, 1.), 1.).is_err());
    let just_below = f32::from_bits((i32::MAX as f32).to_bits() - 1);
    assert_eq!(
        target_pixels(Rect::new(0., 0., just_below, 1.), 1.).unwrap(),
        (just_below as u32, 1)
    );
    assert!(target_pixels(Rect::new(0., 0., just_below, 1.), 2.).is_err());
}
