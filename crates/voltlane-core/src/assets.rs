use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::ErrorKind,
    path::Path,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};
use tracing::{debug, instrument, warn};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioWaveformPeaks {
    pub bucket_size: usize,
    pub peaks: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioAnalysis {
    pub source_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub total_frames: u64,
    pub duration_seconds: f64,
    pub peaks: AudioWaveformPeaks,
    pub cache_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioAssetEntry {
    pub path: String,
    pub extension: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[instrument(fields(path = %path.display(), bucket_size))]
pub fn analyze_audio_file(path: &Path, bucket_size: usize) -> Result<AudioAnalysis> {
    if bucket_size == 0 {
        return Err(anyhow::anyhow!("bucket_size must be greater than zero"));
    }

    let decoded = decode_audio_file_mono(path)?;
    let total_frames = decoded.samples.len() as u64;
    let duration_seconds = if decoded.sample_rate == 0 {
        0.0
    } else {
        total_frames as f64 / f64::from(decoded.sample_rate)
    };
    let peaks = generate_waveform_peaks(&decoded.samples, bucket_size);

    Ok(AudioAnalysis {
        source_path: path.display().to_string(),
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        total_frames,
        duration_seconds,
        peaks: AudioWaveformPeaks { bucket_size, peaks },
        cache_path: None,
    })
}

#[instrument(fields(path = %path.display(), bucket_size, cache_dir = %cache_dir.display()))]
pub fn analyze_audio_file_with_cache(
    path: &Path,
    cache_dir: &Path,
    bucket_size: usize,
) -> Result<AudioAnalysis> {
    if bucket_size == 0 {
        return Err(anyhow::anyhow!("bucket_size must be greater than zero"));
    }

    fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create audio cache dir: {}", cache_dir.display()))?;

    let hash = asset_hash(path)?;
    let cache_path = cache_dir.join(format!("{hash}.peaks.json"));
    if cache_path.is_file() {
        let cached_bytes = fs::read(&cache_path)
            .with_context(|| format!("failed to read waveform cache {}", cache_path.display()))?;
        match serde_json::from_slice::<AudioAnalysis>(&cached_bytes) {
            Ok(mut cached) if cached.peaks.bucket_size == bucket_size => {
                cached.cache_path = Some(cache_path.display().to_string());
                debug!(path = %cache_path.display(), "waveform cache hit");
                return Ok(cached);
            }
            Ok(_) => {
                warn!(
                    path = %cache_path.display(),
                    "waveform cache bucket_size mismatch, regenerating"
                );
            }
            Err(error) => {
                warn!(
                    path = %cache_path.display(),
                    ?error,
                    "waveform cache parse failed, regenerating"
                );
            }
        }
    }

    let mut analysis = analyze_audio_file(path, bucket_size)?;
    analysis.cache_path = Some(cache_path.display().to_string());
    let json = serde_json::to_vec_pretty(&analysis).context("failed to encode analysis json")?;
    fs::write(&cache_path, json)
        .with_context(|| format!("failed to write audio cache: {}", cache_path.display()))?;
    Ok(analysis)
}

#[instrument(fields(path = %path.display()))]
pub fn decode_audio_file_mono(path: &Path) -> Result<DecodedAudio> {
    let file = File::open(path)
        .with_context(|| format!("failed to open audio file: {}", path.display()))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("no default audio track found in {}", path.display()))?;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let mut channels = track
        .codec_params
        .channels
        .map(|value| value.count() as u16)
        .unwrap_or(2);
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(anyhow::anyhow!(
                    "audio stream reset required for {}",
                    path.display()
                ));
            }
            Err(error) => return Err(error.into()),
        };

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => {
                continue;
            }
            Err(error) => return Err(error.into()),
        };

        sample_rate = decoded.spec().rate;
        channels = decoded.spec().channels.count() as u16;
        push_mono_samples(decoded, &mut samples);
    }

    if samples.is_empty() {
        return Err(anyhow::anyhow!(
            "decoded zero samples from {}",
            path.display()
        ));
    }

    debug!(
        sample_rate,
        channels,
        total_frames = samples.len(),
        "audio decode complete"
    );

    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples,
    })
}

#[instrument(fields(directory = %directory.display()))]
pub fn scan_audio_assets(directory: &Path) -> Result<Vec<AudioAssetEntry>> {
    if !directory.exists() {
        fs::create_dir_all(directory).with_context(|| {
            format!(
                "failed to create audio asset directory: {}",
                directory.display()
            )
        })?;
        debug!(
            directory = %directory.display(),
            "audio asset directory missing, created empty directory"
        );
        return Ok(Vec::new());
    }

    if !directory.is_dir() {
        return Err(anyhow::anyhow!(
            "audio asset path is not a directory: {}",
            directory.display()
        ));
    }

    let extensions = supported_audio_extensions();
    let mut assets = Vec::new();

    for entry in WalkDir::new(directory).follow_links(true) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warn!(
                    ?error,
                    "ignoring unreadable entry while scanning audio assets"
                );
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());
        let Some(extension) = extension else {
            continue;
        };
        if !extensions.contains(extension.as_str()) {
            continue;
        }

        let size_bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        assets.push(AudioAssetEntry {
            path: entry.path().display().to_string(),
            extension,
            size_bytes,
        });
    }

    assets.sort_by(|left, right| left.path.cmp(&right.path));
    debug!(count = assets.len(), "audio asset scan complete");
    Ok(assets)
}

fn push_mono_samples(decoded: symphonia::core::audio::AudioBufferRef<'_>, samples: &mut Vec<f32>) {
    let spec = *decoded.spec();
    let channel_count = spec.channels.count().max(1);
    let mut sample_buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
    sample_buffer.copy_interleaved_ref(decoded);

    for frame in sample_buffer.samples().chunks(channel_count) {
        let sum: f32 = frame.iter().copied().sum();
        samples.push(sum / channel_count as f32);
    }
}

fn generate_waveform_peaks(samples: &[f32], bucket_size: usize) -> Vec<f32> {
    samples
        .chunks(bucket_size)
        .map(|chunk| chunk.iter().copied().map(f32::abs).fold(0.0_f32, f32::max))
        .collect()
}

fn asset_hash(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize asset path: {}", path.display()))?;
    let metadata = fs::metadata(&canonical)
        .with_context(|| format!("failed to inspect asset metadata: {}", canonical.display()))?;
    let modified_seconds = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_secs());

    let payload = format!(
        "{}:{}:{}",
        canonical.display(),
        metadata.len(),
        modified_seconds
    );
    let digest = Sha256::digest(payload.as_bytes());
    Ok(format!("{digest:x}"))
}

fn supported_audio_extensions() -> BTreeSet<&'static str> {
    [
        "wav", "flac", "mp3", "ogg", "m4a", "aiff", "aif", "caf", "mkv",
    ]
    .into_iter()
    .collect()
}
