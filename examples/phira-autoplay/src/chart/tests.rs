use super::*;

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn pgr_uses_per_line_ticks_and_integrates_holds_on_both_sides() {
    let chart = Chart::parse(br#"{
        "formatVersion": 3, "offset": 0.15, "judgeLineList": [
            {"bpm": 120,
             "speedEvents": [{"startTime": 0, "endTime": 32, "value": 2},
                             {"startTime": 32, "endTime": 64, "value": 0},
                             {"startTime": 64, "endTime": 128, "value": -1}],
             "judgeLineMoveEvents": [{"startTime": 0, "endTime": 32, "start": 0.5, "end": 0.9, "start2": 0.5, "end2": 0.75}],
             "judgeLineRotateEvents": [{"startTime": 0, "endTime": 32, "start": 0, "end": 90}],
             "judgeLineDisappearEvents": [{"startTime": 0, "endTime": 32, "start": 1, "end": -1}],
             "notesAbove": [{"type": 3, "time": 32, "holdTime": 64, "positionX": 4, "speed": 2, "floorPosition": 999}],
             "notesBelow": [{"type": 3, "time": 32, "holdTime": 64, "positionX": -4, "speed": -2, "floorPosition": 999}]},
            {"bpm": 60, "notesAbove": [{"type": 2, "time": 16, "holdTime": 999, "positionX": 0, "speed": 1, "floorPosition": 0}]}
        ]
    }"#).unwrap();
    assert_eq!(chart.format, Format::Pgr);
    assert_eq!(chart.bpm, 120.);
    assert_eq!(chart.hits.len(), 3);
    near(chart.offset, 0.15);
    near(chart.last_time, 1.5);
    let l = &chart.lines[0];
    let pose = l.sample(0.25);
    near(pose.x.into(), 0.2);
    near(pose.y.into(), -0.125);
    near(pose.angle.into(), -std::f64::consts::FRAC_PI_4);
    assert!(l.sample(0.75).opacity < 0.);
    near(l.sample(0.25).height, 0.5 * FLOOR_SCALE);
    near(l.sample(0.75).height, FLOOR_SCALE);
    near(l.sample(1.25).height, 0.75 * FLOOR_SCALE);
    for (note, above, sign) in [(&l.notes[0], true, 1.), (&l.notes[1], false, -1.)] {
        assert_eq!(note.kind, Kind::Hold);
        assert_eq!(note.above, above);
        assert!(note.highlight);
        near(note.x.into(), 0.225 * sign);
        near(note.height, FLOOR_SCALE);
        near(note.tail_height, 0.5 * FLOOR_SCALE);
        near(note.speed, 1.);
        assert!(note.tail_height - l.sample(1.25).height < 0.);
    }
    let drag = &chart.lines[1].notes[0];
    assert_eq!(drag.kind, Kind::Drag);
    assert_eq!(drag.time, drag.end);
    assert!(drag.highlight);
    assert_eq!(chart.hits[2].line, 1);
}

#[test]
fn rpe_integrates_additive_layers_with_reversal_stop_and_tempo_change() {
    let chart = Chart::parse(br#"{
        "META": {"RPEVersion": 113, "offset": -240},
        "BPMList": [{"startTime": [2,0,1], "bpm": 60}, {"startTime": [0,0,1], "bpm": 120}],
        "judgeLineList": [{"isCover": 0, "eventLayers": [
            {"moveXEvents": [{"startTime": [0,0,1], "endTime": [2,0,1], "start": 0, "end": 1350, "easingType": 5}],
             "moveYEvents": [{"startTime": [0,0,1], "endTime": [2,0,1], "start": 0, "end": 900}],
             "rotateEvents": [{"startTime": [0,0,1], "endTime": [2,0,1], "start": 0, "end": 180}],
             "alphaEvents": [{"startTime": [0,0,1], "endTime": [0,0,1], "start": 0, "end": 255}],
             "speedEvents": [{"startTime": [0,0,1], "endTime": [2,0,1], "start": 15, "end": -15},
                             {"startTime": [3,0,1], "endTime": [4,0,1], "start": 0, "end": 0}]},
            null,
            {"moveXEvents": [{"startTime": [0,0,1], "endTime": [0,0,1], "start": 0, "end": 135}],
             "alphaEvents": [{"startTime": [0,0,1], "endTime": [2,0,1], "start": 0, "end": -510}],
             "speedEvents": [{"startTime": [0,0,1], "endTime": [1,0,1], "start": 15, "end": 15},
                             {"startTime": [1,0,1], "endTime": [1,0,1], "start": 0, "end": 0}]}
        ], "notes": [
            {"type": 3, "startTime": [3,1,2], "endTime": [8,0,1], "positionX": -675, "isFake": 1},
            {"type": 2, "startTime": [1,0,1], "endTime": [3,1,2], "positionX": 675, "speed": 0.5, "above": 2, "yOffset": 90, "alpha": 127.5, "size": 1.25, "visibleTime": 2},
            {"type": 1, "startTime": [0,1,2], "endTime": [0,1,2], "positionX": 0},
            {"type": 4, "startTime": [0,1,2], "endTime": [0,1,2], "positionX": 0, "isFake": 1}
        ]}]
    }"#).unwrap();
    assert_eq!(chart.format, Format::Rpe);
    assert_eq!(chart.bpm, 120.);
    near(chart.offset, -0.24);
    near(chart.last_time, 2.5);
    let l = &chart.lines[0];
    assert!(!l.cover);
    let pose = l.sample(0.5);
    near(pose.x.into(), 0.35);
    near(pose.y.into(), -0.5);
    near(pose.angle.into(), std::f64::consts::FRAC_PI_2);
    near(pose.opacity.into(), 0.);
    let rate = (10. / 45.) * FLOOR_SCALE * 15.;
    near(pose.height, 0.75 * rate);
    near(l.sample(1.).height, 0.5 * rate);
    near(l.sample(1.5).height, 0.);
    near(l.sample(2.7).height, -0.5 * rate);
    assert!(l.sample(2.7).opacity < 0.);

    assert_eq!((chart.hits[0].note, chart.hits[1].note), (2, 1));
    assert_eq!(chart.hits.len(), 2);
    assert!(!l.notes[2].highlight);
    assert!(l.notes[0].fake);
    assert_eq!(l.notes[0].kind, Kind::Flick);
    assert_eq!(l.notes[0].time, l.notes[0].end);
    let hold = &l.notes[1];
    assert!(!hold.above);
    assert_eq!(hold.kind, Kind::Hold);
    near(hold.time, 0.5);
    near(hold.end, 2.5);
    near(hold.height, 0.75 * rate);
    near(hold.tail_height, -0.5 * rate);
    near(hold.x.into(), 0.5);
    near(hold.y_offset, 0.05);
    near(hold.alpha.into(), 0.5);
    near(hold.size.into(), 1.25);
    near(hold.visible_time, 2.);
    near(
        (hold.tail_height - l.sample(1.).height) * hold.speed,
        -0.5 * rate,
    );
}

#[test]
fn rejects_unknown_version_and_invalid_tempo() {
    assert!(Chart::parse(br#"{"formatVersion": 1}"#).is_err());
    assert!(Chart::parse(br#"{"formatVersion": 3, "judgeLineList": [{"bpm": 0}]}"#).is_err());
    assert!(Chart::parse(br#"{"META":{"RPEVersion":113}, "BPMList":[{"startTime":[0,0,0],"bpm":120}],"judgeLineList":[]}"#).is_err());
}
