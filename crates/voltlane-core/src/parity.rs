use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::{export, model::Project};

const PARITY_SCHEMA_VERSION: u32 = 1;
const AUDIO_FINGERPRINT_FRAMES: usize = 96_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParityReport {
    pub schema_version: u32,
    pub project_id: String,
    pub track_count: usize,
    pub clip_count: usize,
    pub note_count: usize,
    pub project_hash: String,
    pub midi_hash: String,
    pub audio_hash: String,
}

#[instrument(skip(project), fields(project_id = %project.id))]
pub fn generate_parity_report(project: &Project) -> Result<ParityReport> {
    let project_bytes = serde_json::to_vec(project).context("failed to serialize project")?;
    let midi_bytes = export::midi_bytes(project)?;
    let audio_samples = export::render_project_samples(project, 1.0);

    let mut audio_bytes = Vec::with_capacity(AUDIO_FINGERPRINT_FRAMES * 2);
    for sample in audio_samples.iter().take(AUDIO_FINGERPRINT_FRAMES) {
        let quantized = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        audio_bytes.extend_from_slice(&quantized.to_le_bytes());
    }

    Ok(ParityReport {
        schema_version: PARITY_SCHEMA_VERSION,
        project_id: project.id.to_string(),
        track_count: project.tracks.len(),
        clip_count: project.clip_count(),
        note_count: project.note_count(),
        project_hash: hash_hex(&project_bytes),
        midi_hash: hash_hex(&midi_bytes),
        audio_hash: hash_hex(&audio_bytes),
    })
}

pub fn read_parity_report(path: &Path) -> Result<ParityReport> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read parity report: {}", path.display()))?;
    let report: ParityReport =
        serde_json::from_slice(&bytes).context("failed to parse parity report json")?;
    Ok(report)
}

pub fn write_parity_report(path: &Path, report: &ParityReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parity directory: {}", parent.display()))?;
    }

    let json = serde_json::to_vec_pretty(report).context("failed to encode parity report json")?;
    fs::write(path, json)
        .with_context(|| format!("failed to write parity report: {}", path.display()))?;
    Ok(())
}

fn hash_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}
