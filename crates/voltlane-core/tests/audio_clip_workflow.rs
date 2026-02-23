use std::path::Path;

use tempfile::tempdir;
use voltlane_core::{
    Engine,
    model::{Project, Track, TrackKind},
};

fn write_test_wav(path: &Path, seconds: f32) {
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
        let phase = frame as f32 / sample_rate as f32 * 330.0 * std::f32::consts::TAU;
        let sample = (phase.sin() * 0.35 * f32::from(i16::MAX)).round() as i16;
        writer
            .write_sample(sample)
            .expect("test wav sample write should succeed");
    }
    writer.finalize().expect("test wav finalize should succeed");
}

#[test]
fn audio_clip_import_and_patch_updates_project_state() {
    let temp = tempdir().expect("tempdir should be creatable");
    let audio_path = temp.path().join("loop.wav");
    let cache_dir = temp.path().join("waveforms");
    write_test_wav(&audio_path, 1.0);

    let mut project = Project::new("Audio Workflow", 140.0, 48_000);
    let track = Track::new("Audio 1", "#ffaa4f", TrackKind::Audio);
    let track_id = track.id;
    project.tracks.push(track);

    let mut engine = Engine::new(project);
    let imported = engine
        .import_audio_clip(
            track_id,
            "Loop".to_string(),
            &audio_path,
            0,
            512,
            Some(&cache_dir),
            0.0,
            0.0,
        )
        .expect("audio import should succeed");

    assert!(matches!(
        imported.payload,
        voltlane_core::model::ClipPayload::Audio(_)
    ));

    let clip_id = imported.id;
    let original_length = imported.length_ticks;
    let patched = engine
        .patch_audio_clip(
            track_id,
            clip_id,
            voltlane_core::AudioClipPatch {
                trim_start_seconds: Some(0.10),
                trim_end_seconds: Some(0.60),
                fade_in_seconds: Some(0.05),
                fade_out_seconds: Some(0.05),
                reverse: Some(true),
                stretch_ratio: Some(1.5),
                gain_db: Some(-3.0),
                pan: Some(0.25),
            },
        )
        .expect("audio patch should succeed");

    let audio = match patched.payload {
        voltlane_core::model::ClipPayload::Audio(audio) => audio,
        _ => panic!("imported clip payload should be audio"),
    };

    assert!(patched.length_ticks != original_length);
    assert!(audio.reverse);
    assert_eq!(audio.trim_start_seconds, 0.10);
    assert_eq!(audio.trim_end_seconds, 0.60);
    assert_eq!(audio.stretch_ratio, 1.5);
    assert!(audio.waveform_cache_path.is_some());
}
