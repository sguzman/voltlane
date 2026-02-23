use voltlane_core::{
    export::{export_midi, export_wav},
    model::{
        Clip, ClipPayload, DEFAULT_SAMPLE_RATE, MidiClip, MidiNote, Project, Track, TrackKind,
    },
};

#[test]
fn midi_and_wav_exports_generate_output() {
    let mut project = Project::new("Export Smoke", 120.0, DEFAULT_SAMPLE_RATE);
    let mut track = Track::new("Keys", "#18c0ff", TrackKind::Midi);
    track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "intro".to_string(),
        start_tick: 0,
        length_ticks: 960,
        disabled: false,
        payload: ClipPayload::Midi(MidiClip {
            instrument: Some("EP".to_string()),
            notes: vec![MidiNote {
                pitch: 60,
                velocity: 110,
                start_tick: 0,
                length_ticks: 960,
                channel: 0,
            }],
        }),
    });
    project.tracks.push(track);

    let temp_dir = tempfile::tempdir().expect("tempdir should work");
    let midi_path = temp_dir.path().join("smoke.mid");
    let wav_path = temp_dir.path().join("smoke.wav");

    export_midi(&project, &midi_path).expect("midi export should succeed");
    export_wav(&project, &wav_path).expect("wav export should succeed");

    let midi_size = std::fs::metadata(&midi_path)
        .expect("midi metadata must exist")
        .len();
    let wav_size = std::fs::metadata(&wav_path)
        .expect("wav metadata must exist")
        .len();

    assert!(midi_size > 0, "midi file should not be empty");
    assert!(
        wav_size > 44,
        "wav file should include samples beyond header"
    );
}
