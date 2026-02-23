use voltlane_core::{Engine, EngineError, TrackerRow, fixtures::demo_project};

#[test]
fn upsert_pattern_rows_generates_note_timeline() {
    let project = demo_project();
    let track_id = project.tracks[1].id;
    let clip_id = project.tracks[1].clips[0].id;
    let mut engine = Engine::new(project);

    engine
        .upsert_pattern_rows(
            track_id,
            clip_id,
            vec![
                TrackerRow {
                    row: 0,
                    note: Some(36),
                    velocity: 110,
                    gate: true,
                    effect: Some("arp".to_string()),
                    effect_value: Some(0x123),
                },
                TrackerRow {
                    row: 4,
                    note: Some(43),
                    velocity: 96,
                    gate: true,
                    effect: None,
                    effect_value: None,
                },
                TrackerRow {
                    row: 6,
                    note: None,
                    velocity: 0,
                    gate: false,
                    effect: Some("cut".to_string()),
                    effect_value: Some(0x10),
                },
            ],
            Some(8),
        )
        .expect("pattern rows update should succeed");

    let clip = &engine.project().tracks[1].clips[0];
    let pattern = match &clip.payload {
        voltlane_core::model::ClipPayload::Pattern(pattern) => pattern,
        _ => panic!("fixture clip payload should be pattern"),
    };

    assert_eq!(pattern.lines_per_beat, 8);
    assert_eq!(pattern.rows.len(), 3);
    assert_eq!(
        pattern.notes.len(),
        2,
        "only gated rows with notes become midi"
    );
    assert_eq!(pattern.notes[0].start_tick, 0);
    assert_eq!(
        pattern.notes[1].start_tick, 240,
        "row 4 at 8 LPB and 480 PPQ should be 240 ticks"
    );
}

#[test]
fn tracker_rows_reject_zero_lines_per_beat() {
    let project = demo_project();
    let track_id = project.tracks[1].id;
    let clip_id = project.tracks[1].clips[0].id;
    let mut engine = Engine::new(project);

    let error = engine
        .upsert_pattern_rows(track_id, clip_id, Vec::new(), Some(0))
        .expect_err("zero lines_per_beat should fail");

    assert!(matches!(error, EngineError::InvalidTrackerLinesPerBeat(0)));
}
