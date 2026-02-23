use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Dev,
    Prod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub mode: AppMode,
    pub project: ProjectConfig,
    pub midi: MidiConfig,
    pub audio: AudioConfig,
    pub transport: TransportConfig,
    pub diagnostics: DiagnosticsConfig,
    pub paths: PathsConfig,
    pub wayland: WaylandConfig,
    pub export: ExportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub default_title: String,
    pub default_bpm: f64,
    pub default_sample_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiConfig {
    pub default_soundfont_path: PathBuf,
    pub default_soundfont_license_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransportConfig {
    pub default_loop_start_tick: u64,
    pub default_loop_end_tick: u64,
    pub metronome_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub asset_directories: Vec<PathBuf>,
    pub waveform_cache_dir: PathBuf,
    pub analysis_bucket_size: usize,
    pub default_import_clip_name: String,
    pub default_gain_db: f32,
    pub default_pan: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagnosticsConfig {
    pub rust_log_filter: String,
    pub trace_file_prefix: String,
    pub tauri_log_level: String,
    pub tauri_log_stdout: bool,
    pub tauri_log_webview: bool,
    pub tauri_log_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub dev_logs_dir: PathBuf,
    pub dev_autosave_dir: PathBuf,
    pub dev_export_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WaylandConfig {
    pub enable_workarounds: bool,
    pub disable_dmabuf_renderer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    pub ffmpeg_binary: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: AppMode::Dev,
            project: ProjectConfig::default(),
            midi: MidiConfig::default(),
            audio: AudioConfig::default(),
            transport: TransportConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            paths: PathsConfig::default(),
            wayland: WaylandConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            default_title: "Untitled".to_string(),
            default_bpm: 140.0,
            default_sample_rate: 48_000,
        }
    }
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self {
            default_soundfont_path: PathBuf::from("src-tauri/res/soundfonts/piano.sf2"),
            default_soundfont_license_path: PathBuf::from(
                "src-tauri/res/soundfonts/piano.sf2.LICENSE",
            ),
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            default_loop_start_tick: 0,
            default_loop_end_tick: 1_920,
            metronome_enabled: true,
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            asset_directories: vec![PathBuf::from("data/audio-library")],
            waveform_cache_dir: PathBuf::from("data/waveform-cache"),
            analysis_bucket_size: 1024,
            default_import_clip_name: "Audio Clip".to_string(),
            default_gain_db: 0.0,
            default_pan: 0.0,
        }
    }
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            rust_log_filter: "info,voltlane_core=trace,voltlane_tauri=trace,tauri_plugin_log=info"
                .to_string(),
            trace_file_prefix: "voltlane".to_string(),
            tauri_log_level: "info".to_string(),
            tauri_log_stdout: true,
            tauri_log_webview: true,
            tauri_log_file: true,
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            dev_logs_dir: PathBuf::from("logs"),
            dev_autosave_dir: PathBuf::from("data/autosave"),
            dev_export_dir: PathBuf::from("data/exports"),
        }
    }
}

impl Default for WaylandConfig {
    fn default() -> Self {
        Self {
            enable_workarounds: true,
            disable_dmabuf_renderer: true,
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            ffmpeg_binary: "ffmpeg".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let config_path = discover_config_path().with_context(|| {
            "failed to locate voltlane.config.toml; looked in cwd and parent directory".to_string()
        })?;

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config file {}", config_path.display()))?;

        let config: AppConfig = toml::from_str(&content).with_context(|| {
            format!("failed to parse config TOML from {}", config_path.display())
        })?;

        Ok(config)
    }
}

fn discover_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("VOLTLANE_CONFIG_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }

    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let candidates = [
        cwd.join("voltlane.config.toml"),
        cwd.join("../voltlane.config.toml"),
    ];

    candidates
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| anyhow::anyhow!("voltlane.config.toml not found"))
}
