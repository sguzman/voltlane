use voltlane_core::{
    Engine,
    fixtures::demo_project,
    model::MidiNote,
};

#[test]
fn transpose_and_quantize_notes_update_clip_data() {
    let project = demo_project();
    let track_id = project.tracks[0].id;
    let clip_id = project.tracks[0].clips[0].id;

    let mut engine = Engine::new(project);

    engine
        .transpose_clip_notes(track_id, clip_id, 2)
        .expect("transpose should succeed");
    engine
        .quantize_clip_notes(track_id, clip_id, 120)
        .expect("quantize should succeed");

    let project = engine.project();
    let clip = &project.tracks[0].clips[0];
    let notes = match &clip.payload {
        voltlane_core::model::ClipPayload::Midi(midi) => &midi.notes,
        _ => panic!("fixture clip payload should be midi"),
    };

    assert_eq!(notes[0].pitch, 74);
    assert_eq!(notes[0].start_tick, 0);
    assert_eq!(notes[0].length_ticks % 120, 0);
}

#[test]
fn add_and_remove_note_roundtrip() {
    let project = demo_project();
    let track_id = project.tracks[0].id;
    let clip_id = project.tracks[0].clips[0].id;

    let mut engine = Engine::new(project);
    engine
        .add_clip_note(
            track_id,
            clip_id,
            MidiNote {
                pitch: 84,
                velocity: 100,
                start_tick: 180,
                length_ticks: 90,
                channel: 0,
            },
        )
        .expect("add note should succeed");

    let note_count_after_add = match &engine.project().tracks[0].clips[0].payload {
        voltlane_core::model::ClipPayload::Midi(midi) => midi.notes.len(),
        _ => panic!("fixture clip payload should be midi"),
    };

    engine
        .remove_clip_note(track_id, clip_id, note_count_after_add - 1)
        .expect("remove note should succeed");

    let note_count_after_remove = match &engine.project().tracks[0].clips[0].payload {
        voltlane_core::model::ClipPayload::Midi(midi) => midi.notes.len(),
        _ => panic!("fixture clip payload should be midi"),
    };

    assert_eq!(note_count_after_remove, note_count_after_add - 1);
}
