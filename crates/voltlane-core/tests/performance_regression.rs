use std::time::Instant;

use voltlane_core::{
    export::{midi_bytes, render_project_samples},
    model::{
        Clip, ClipPayload, DEFAULT_SAMPLE_RATE, DEFAULT_TRACKER_LINES_PER_BEAT, MidiClip, MidiNote,
        PatternClip, Project, Track, TrackKind,
    },
};

fn budget_ms_from_env(key: &str, fallback: u128) -> u128 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
        .unwrap_or(fallback)
}

fn perf_project() -> Project {
    let mut project = Project::new("Perf Regression", 140.0, DEFAULT_SAMPLE_RATE);
    let clip_count = 6_u64;
    let notes_per_clip = 48_u64;

    for track_index in 0..8_u64 {
        let is_chip = track_index % 2 == 1;
        let mut track = Track::new(
            format!("Track {}", track_index + 1),
            "#28c4aa",
            if is_chip {
                TrackKind::Chip
            } else {
                TrackKind::Midi
            },
        );

        for clip_index in 0..clip_count {
            let start_tick = clip_index * 1_920;
            let mut notes = Vec::with_capacity(notes_per_clip as usize);
            for note_index in 0..notes_per_clip {
                let pitch_seed = 48 + ((track_index + note_index) % 24) as u8;
                notes.push(MidiNote {
                    pitch: pitch_seed.min(127),
                    velocity: 96,
                    start_tick: note_index * 40,
                    length_ticks: 60,
                    channel: 0,
                });
            }

            let payload = if is_chip {
                ClipPayload::Pattern(PatternClip {
                    source_chip: "gameboy_apu".to_string(),
                    notes,
                    rows: Vec::new(),
                    macros: Vec::new(),
                    lines_per_beat: DEFAULT_TRACKER_LINES_PER_BEAT,
                })
            } else {
                ClipPayload::Midi(MidiClip {
                    instrument: Some("Perf Synth".to_string()),
                    notes,
                })
            };

            track.clips.push(Clip {
                id: uuid::Uuid::new_v4(),
                name: format!("clip-{}-{}", track_index + 1, clip_index + 1),
                start_tick,
                length_ticks: 1_920,
                disabled: false,
                payload,
            });
        }

        project.tracks.push(track);
    }

    project
}

#[test]
fn render_and_midi_encoding_stay_within_budget() {
    let project = perf_project();
    let max_midi_ms = budget_ms_from_env("VOLTLANE_PERF_MAX_MIDI_MS", 1_500);
    let max_render_ms = budget_ms_from_env("VOLTLANE_PERF_MAX_RENDER_MS", 4_500);

    let midi_start = Instant::now();
    let midi = midi_bytes(&project).expect("midi encoding should succeed");
    let midi_elapsed_ms = midi_start.elapsed().as_millis();
    assert!(!midi.is_empty(), "midi bytes should not be empty");
    assert!(
        midi_elapsed_ms <= max_midi_ms,
        "midi encode regression: {}ms exceeded budget {}ms",
        midi_elapsed_ms,
        max_midi_ms
    );

    let render_start = Instant::now();
    let rendered = render_project_samples(&project, 1.0);
    let render_elapsed_ms = render_start.elapsed().as_millis();
    assert!(!rendered.is_empty(), "rendered buffer should not be empty");
    assert!(
        render_elapsed_ms <= max_render_ms,
        "render regression: {}ms exceeded budget {}ms",
        render_elapsed_ms,
        max_render_ms
    );
}
