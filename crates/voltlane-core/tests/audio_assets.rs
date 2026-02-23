use std::path::Path;

use tempfile::tempdir;
use voltlane_core::assets::{
    analyze_audio_file_with_cache, decode_audio_file_mono, scan_audio_assets,
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
        let phase = frame as f32 / sample_rate as f32 * 220.0 * std::f32::consts::TAU;
        let sample = (phase.sin() * 0.5 * f32::from(i16::MAX)).round() as i16;
        writer
            .write_sample(sample)
            .expect("test wav sample write should succeed");
    }
    writer.finalize().expect("test wav finalize should succeed");
}

#[test]
fn analyze_and_cache_audio_file() {
    let temp = tempdir().expect("tempdir should be creatable");
    let audio_path = temp.path().join("tone.wav");
    let cache_dir = temp.path().join("cache");
    write_test_wav(&audio_path, 0.75);

    let analysis = analyze_audio_file_with_cache(&audio_path, &cache_dir, 256)
        .expect("analysis should succeed");
    assert_eq!(analysis.sample_rate, 48_000);
    assert_eq!(analysis.channels, 1);
    assert!(analysis.total_frames > 0);
    assert!(!analysis.peaks.peaks.is_empty());
    let cache_path = analysis
        .cache_path
        .as_deref()
        .expect("cache path should be populated");
    assert!(Path::new(cache_path).is_file());

    let cached_analysis = analyze_audio_file_with_cache(&audio_path, &cache_dir, 256)
        .expect("cached analysis should succeed");
    assert_eq!(analysis.peaks, cached_analysis.peaks);
}

#[test]
fn decode_and_scan_audio_assets() {
    let temp = tempdir().expect("tempdir should be creatable");
    let audio_dir = temp.path().join("audio");
    std::fs::create_dir_all(&audio_dir).expect("audio dir should be creatable");

    let tone_path = audio_dir.join("tone.wav");
    let ignored_path = audio_dir.join("ignore.txt");
    write_test_wav(&tone_path, 0.5);
    std::fs::write(&ignored_path, "not audio").expect("ignored file should be writable");

    let decoded = decode_audio_file_mono(&tone_path).expect("decode should succeed");
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 1);
    assert!(!decoded.samples.is_empty());

    let assets = scan_audio_assets(&audio_dir).expect("scan should succeed");
    assert_eq!(assets.len(), 1);
    assert!(assets[0].path.ends_with("tone.wav"));
}
