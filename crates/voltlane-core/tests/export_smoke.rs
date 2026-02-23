use voltlane_core::{
    RenderMode,
    export::{export_midi, export_stem_wav, export_wav, midi_bytes},
    model::{
        AudioClip, ChipMacroLane, Clip, ClipPayload, DEFAULT_SAMPLE_RATE, MidiClip, MidiNote,
        PatternClip, Project, Track, TrackKind,
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
    export_wav(&project, &wav_path, RenderMode::Offline).expect("wav export should succeed");

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

#[test]
fn wav_export_renders_audio_clip_payload() {
    let temp_dir = tempfile::tempdir().expect("tempdir should work");
    let source_wav = temp_dir.path().join("source.wav");
    write_test_wav(&source_wav, 0.75);

    let mut project = Project::new("Audio Clip Render", 120.0, DEFAULT_SAMPLE_RATE);
    let mut track = Track::new("Audio", "#ff9757", TrackKind::Audio);
    track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "audio".to_string(),
        start_tick: 0,
        length_ticks: 1_440,
        disabled: false,
        payload: ClipPayload::Audio(AudioClip {
            source_path: source_wav.display().to_string(),
            gain_db: 0.0,
            pan: 0.0,
            source_sample_rate: 48_000,
            source_channels: 1,
            source_duration_seconds: 0.75,
            trim_start_seconds: 0.0,
            trim_end_seconds: 0.75,
            fade_in_seconds: 0.02,
            fade_out_seconds: 0.02,
            reverse: false,
            stretch_ratio: 1.0,
            waveform_bucket_size: 256,
            waveform_peaks: vec![0.3; 64],
            waveform_cache_path: None,
        }),
    });
    project.tracks.push(track);

    let output_wav = temp_dir.path().join("audio_clip.wav");
    export_wav(&project, &output_wav, RenderMode::Offline)
        .expect("wav export with audio clip should succeed");
    let wav_size = std::fs::metadata(&output_wav)
        .expect("wav metadata must exist")
        .len();
    assert!(
        wav_size > 44,
        "wav file should include rendered audio samples beyond header"
    );
}

#[test]
fn pattern_arpeggio_macro_changes_midi_pitch_output() {
    let mut project = Project::new("Macro MIDI", 120.0, DEFAULT_SAMPLE_RATE);
    let mut track = Track::new("Chip", "#ffb347", TrackKind::Chip);
    track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "chip-pattern".to_string(),
        start_tick: 0,
        length_ticks: 960,
        disabled: false,
        payload: ClipPayload::Pattern(PatternClip {
            source_chip: "gameboy_apu".to_string(),
            notes: vec![
                MidiNote {
                    pitch: 60,
                    velocity: 100,
                    start_tick: 0,
                    length_ticks: 240,
                    channel: 0,
                },
                MidiNote {
                    pitch: 60,
                    velocity: 100,
                    start_tick: 120,
                    length_ticks: 240,
                    channel: 0,
                },
            ],
            rows: Vec::new(),
            macros: vec![ChipMacroLane {
                target: "arpeggio".to_string(),
                enabled: true,
                values: vec![0, 12],
                loop_start: Some(0),
                loop_end: Some(1),
            }],
            lines_per_beat: 4,
        }),
    });
    project.tracks.push(track);

    let bytes = midi_bytes(&project).expect("midi bytes should render");
    let smf = midly::Smf::parse(&bytes).expect("rendered midi should parse");

    let mut note_ons = Vec::new();
    for track in &smf.tracks {
        for event in track {
            if let midly::TrackEventKind::Midi { message, .. } = event.kind
                && let midly::MidiMessage::NoteOn { key, vel } = message
                && vel.as_int() > 0
            {
                note_ons.push(key.as_int());
            }
        }
    }

    assert!(
        note_ons.contains(&60) && note_ons.contains(&72),
        "macro arpeggio should emit transposed note-on pitch values"
    );
}

#[test]
fn stem_export_writes_per_track_wav_files() {
    let mut project = Project::new("Stem Export", 120.0, DEFAULT_SAMPLE_RATE);
    let mut midi_track = Track::new("Lead Synth", "#18c0ff", TrackKind::Midi);
    midi_track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "lead".to_string(),
        start_tick: 0,
        length_ticks: 960,
        disabled: false,
        payload: ClipPayload::Midi(MidiClip {
            instrument: Some("Lead".to_string()),
            notes: vec![MidiNote {
                pitch: 67,
                velocity: 110,
                start_tick: 0,
                length_ticks: 960,
                channel: 0,
            }],
        }),
    });
    project.tracks.push(midi_track);

    let mut chip_track = Track::new("Chip Bass", "#f97316", TrackKind::Chip);
    chip_track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "bass".to_string(),
        start_tick: 0,
        length_ticks: 960,
        disabled: false,
        payload: ClipPayload::Pattern(PatternClip {
            source_chip: "gameboy_apu".to_string(),
            notes: vec![MidiNote {
                pitch: 48,
                velocity: 100,
                start_tick: 0,
                length_ticks: 960,
                channel: 0,
            }],
            rows: Vec::new(),
            macros: Vec::new(),
            lines_per_beat: 4,
        }),
    });
    project.tracks.push(chip_track);

    let temp_dir = tempfile::tempdir().expect("tempdir should work");
    let stem_paths = export_stem_wav(&project, temp_dir.path(), RenderMode::Realtime)
        .expect("stem export should succeed");
    assert_eq!(
        stem_paths.len(),
        2,
        "two enabled tracks should produce two stems"
    );
    for path in stem_paths {
        let metadata = std::fs::metadata(path).expect("stem path should exist");
        assert!(
            metadata.len() > 44,
            "stem wav should include rendered audio"
        );
    }
}

fn write_test_wav(path: &std::path::Path, seconds: f32) {
    let sample_rate = 48_000_u32;
    let frame_count = (seconds * sample_rate as f32).round() as usize;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).expect("test wav should be creatable");
    for frame in 0..frame_count {
        let phase = frame as f32 / sample_rate as f32 * 220.0 * std::f32::consts::TAU;
        let sample = (phase.sin() * 0.4 * f32::from(i16::MAX)).round() as i16;
        writer
            .write_sample(sample)
            .expect("test wav sample write should succeed");
    }
    writer.finalize().expect("test wav finalize should succeed");
}
