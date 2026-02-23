mod config;

use std::path::{Path, PathBuf};

use anyhow::Context;
use parking_lot::Mutex;
use serde::Deserialize;
use tauri::{Manager, State};
use tauri_plugin_log::{Target, TargetKind, log::LevelFilter};
use tracing::{error, info, instrument};
use uuid::Uuid;
use voltlane_core::{
    AddClipRequest, AddTrackRequest, ClipPayload, Engine, ExportKind, MidiClip, MidiNote,
    ParityReport, PatternClip, Project, TrackStatePatch, init_tracing_with_options,
};

use crate::config::{AppConfig, AppMode};

struct AppState {
    engine: Mutex<Engine>,
    config: AppConfig,
}

#[derive(Debug, Deserialize)]
struct CreateProjectInput {
    title: String,
    bpm: Option<f64>,
    sample_rate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AddMidiClipInput {
    track_id: String,
    name: String,
    start_tick: u64,
    length_ticks: u64,
    instrument: Option<String>,
    source_chip: Option<String>,
    notes: Vec<MidiNote>,
}

#[derive(Debug, Deserialize)]
struct AddEffectInput {
    track_id: String,
    effect_name: String,
}

#[derive(Debug, Deserialize)]
struct ReorderTrackInput {
    from: usize,
    to: usize,
}

#[derive(Debug, Deserialize)]
struct PatchTrackInput {
    track_id: String,
    hidden: Option<bool>,
    mute: Option<bool>,
    solo: Option<bool>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MoveClipInput {
    track_id: String,
    clip_id: String,
    start_tick: u64,
    length_ticks: u64,
}

#[derive(Debug, Deserialize)]
struct ExportProjectInput {
    kind: ExportKind,
    output_path: String,
    ffmpeg_binary: Option<String>,
}

#[instrument(skip(state))]
#[tauri::command]
fn get_project(state: State<'_, AppState>) -> Project {
    state.engine.lock().project().clone()
}

#[instrument(skip(state))]
#[tauri::command]
fn create_project(state: State<'_, AppState>, input: CreateProjectInput) -> Project {
    let mut engine = state.engine.lock();
    engine.create_project(
        input.title,
        input
            .bpm
            .unwrap_or(state.config.project.default_bpm)
            .max(20.0),
        input
            .sample_rate
            .unwrap_or(state.config.project.default_sample_rate)
            .max(8_000),
    );
    engine.project().clone()
}

#[instrument(skip(state, request))]
#[tauri::command]
fn add_track(state: State<'_, AppState>, request: AddTrackRequest) -> Result<Project, String> {
    let mut engine = state.engine.lock();
    let _ = engine.add_track(request);
    Ok(engine.project().clone())
}

#[instrument(skip(state, input))]
#[tauri::command]
fn patch_track_state(
    state: State<'_, AppState>,
    input: PatchTrackInput,
) -> Result<Project, String> {
    let track_id = parse_uuid(&input.track_id)?;
    let mut engine = state.engine.lock();
    engine
        .patch_track_state(
            track_id,
            TrackStatePatch {
                hidden: input.hidden,
                mute: input.mute,
                solo: input.solo,
                enabled: input.enabled,
            },
        )
        .map_err(|error| error.to_string())?;

    Ok(engine.project().clone())
}

#[instrument(skip(state, input))]
#[tauri::command]
fn reorder_track(state: State<'_, AppState>, input: ReorderTrackInput) -> Result<Project, String> {
    let mut engine = state.engine.lock();
    engine
        .reorder_track(input.from, input.to)
        .map_err(|error| error.to_string())?;
    Ok(engine.project().clone())
}

#[instrument(skip(state, input))]
#[tauri::command]
fn add_midi_clip(state: State<'_, AppState>, input: AddMidiClipInput) -> Result<Project, String> {
    let track_id = parse_uuid(&input.track_id)?;

    let payload = if let Some(source_chip) = input.source_chip {
        ClipPayload::Pattern(PatternClip {
            source_chip,
            notes: input.notes,
        })
    } else {
        ClipPayload::Midi(MidiClip {
            instrument: input.instrument,
            notes: input.notes,
        })
    };

    let request = AddClipRequest {
        track_id,
        name: input.name,
        start_tick: input.start_tick,
        length_ticks: input.length_ticks,
        payload,
    };

    let mut engine = state.engine.lock();
    engine
        .add_clip(request)
        .map_err(|error| error.to_string())?;
    Ok(engine.project().clone())
}

#[instrument(skip(state, input))]
#[tauri::command]
fn move_clip(state: State<'_, AppState>, input: MoveClipInput) -> Result<Project, String> {
    let track_id = parse_uuid(&input.track_id)?;
    let clip_id = parse_uuid(&input.clip_id)?;
    let mut engine = state.engine.lock();
    engine
        .move_clip(track_id, clip_id, input.start_tick, input.length_ticks)
        .map_err(|error| error.to_string())?;

    Ok(engine.project().clone())
}

#[instrument(skip(state, input))]
#[tauri::command]
fn add_effect(state: State<'_, AppState>, input: AddEffectInput) -> Result<Project, String> {
    let track_id = parse_uuid(&input.track_id)?;
    let mut engine = state.engine.lock();
    engine
        .add_effect(track_id, voltlane_core::EffectSpec::new(input.effect_name))
        .map_err(|error| error.to_string())?;
    Ok(engine.project().clone())
}

#[instrument(skip(state))]
#[tauri::command]
fn set_playback(state: State<'_, AppState>, is_playing: bool) -> Project {
    let mut engine = state.engine.lock();
    engine.toggle_playback(is_playing);
    engine.project().clone()
}

#[instrument(skip(state))]
#[tauri::command]
fn set_loop_region(
    state: State<'_, AppState>,
    loop_start_tick: u64,
    loop_end_tick: u64,
    loop_enabled: bool,
) -> Project {
    let mut engine = state.engine.lock();
    engine.set_loop_region(loop_start_tick, loop_end_tick, loop_enabled);
    engine.project().clone()
}

#[instrument(skip(state, input))]
#[tauri::command]
fn export_project(state: State<'_, AppState>, input: ExportProjectInput) -> Result<String, String> {
    let engine = state.engine.lock();
    let ffmpeg_path = input
        .ffmpeg_binary
        .as_deref()
        .unwrap_or(state.config.export.ffmpeg_binary.as_str());
    let ffmpeg_binary = Some(Path::new(ffmpeg_path));

    engine
        .export(input.kind, Path::new(&input.output_path), ffmpeg_binary)
        .map_err(|error| error.to_string())?;
    Ok(input.output_path)
}

#[instrument(skip(state), fields(path = %path))]
#[tauri::command]
fn save_project(state: State<'_, AppState>, path: String) -> Result<Project, String> {
    let engine = state.engine.lock();
    engine
        .save_project(Path::new(&path))
        .map_err(|error| error.to_string())?;
    Ok(engine.project().clone())
}

#[instrument(skip(state), fields(path = %path))]
#[tauri::command]
fn load_project(state: State<'_, AppState>, path: String) -> Result<Project, String> {
    let mut engine = state.engine.lock();
    engine
        .load_project(Path::new(&path))
        .map_err(|error| error.to_string())
}

#[instrument(skip(state), fields(path = %autosave_dir))]
#[tauri::command]
fn autosave_project(state: State<'_, AppState>, autosave_dir: String) -> Result<String, String> {
    let autosave_path = if autosave_dir.trim().is_empty() {
        match state.config.mode {
            AppMode::Dev => resolve_dev_path(&state.config.paths.dev_autosave_dir),
            AppMode::Prod => PathBuf::from("autosave"),
        }
    } else {
        PathBuf::from(autosave_dir)
    };

    let engine = state.engine.lock();
    let path = engine
        .autosave(&autosave_path)
        .map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

#[instrument(skip(state))]
#[tauri::command]
fn measure_parity(state: State<'_, AppState>) -> Result<ParityReport, String> {
    let engine = state.engine.lock();
    voltlane_core::generate_parity_report(engine.project()).map_err(|error| error.to_string())
}

fn parse_uuid(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid UUID '{value}': {error}"))
}

fn parse_level_filter(value: &str) -> LevelFilter {
    match value.to_ascii_lowercase().as_str() {
        "off" => LevelFilter::Off,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    }
}

fn resolve_dev_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_runtime_log_dir(config: &AppConfig, app: &tauri::App) -> anyhow::Result<PathBuf> {
    match config.mode {
        AppMode::Dev => Ok(resolve_dev_path(&config.paths.dev_logs_dir)),
        AppMode::Prod => app
            .path()
            .app_log_dir()
            .map_err(|error| anyhow::anyhow!(error.to_string())),
    }
}

#[cfg(target_os = "linux")]
fn configure_wayland_env(config: &AppConfig) {
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !is_wayland || !config.wayland.enable_workarounds {
        return;
    }

    if config.wayland.disable_dmabuf_renderer
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        // SAFETY: Applied at process startup before any threads are spawned.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}

fn initial_engine(config: &AppConfig) -> Engine {
    let mut project = Project::new(
        config.project.default_title.clone(),
        config.project.default_bpm,
        config.project.default_sample_rate,
    );
    project.transport.loop_start_tick = config.transport.default_loop_start_tick;
    project.transport.loop_end_tick = config.transport.default_loop_end_tick;
    project.transport.metronome_enabled = config.transport.metronome_enabled;
    Engine::new(project)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load().unwrap_or_else(|error| {
        panic!("failed to load app config from voltlane.config.toml: {error}");
    });

    #[cfg(target_os = "linux")]
    configure_wayland_env(&config);

    let tauri_log_level = parse_level_filter(&config.diagnostics.tauri_log_level);
    let mut tauri_log_builder = tauri_plugin_log::Builder::new()
        .level(tauri_log_level)
        .level_for("voltlane_core", LevelFilter::Trace)
        .level_for("voltlane_tauri", LevelFilter::Trace);

    if config.diagnostics.tauri_log_stdout {
        tauri_log_builder = tauri_log_builder.target(Target::new(TargetKind::Stdout));
    }
    if config.diagnostics.tauri_log_webview {
        tauri_log_builder = tauri_log_builder.target(Target::new(TargetKind::Webview));
    }
    if config.diagnostics.tauri_log_file {
        tauri_log_builder = match config.mode {
            AppMode::Dev => tauri_log_builder.target(Target::new(TargetKind::Folder {
                path: resolve_dev_path(&config.paths.dev_logs_dir),
                file_name: Some("voltlane-tauri".to_string()),
            })),
            AppMode::Prod => tauri_log_builder.target(Target::new(TargetKind::LogDir {
                file_name: Some("voltlane-tauri".to_string()),
            })),
        };
    }

    let app_state = AppState {
        engine: Mutex::new(initial_engine(&config)),
        config: config.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_log_builder.build())
        .setup(move |app| {
            let log_dir = resolve_runtime_log_dir(&config, app)?;
            let telemetry = init_tracing_with_options(
                &log_dir,
                &config.diagnostics.trace_file_prefix,
                &config.diagnostics.rust_log_filter,
            )
            .context("tracing init failed")?;
            info!(
                mode = ?config.mode,
                session_id = %telemetry.session_id,
                log_dir = %log_dir.display(),
                "voltlane tauri setup complete"
            );

            // Leak once at startup to keep worker guard alive for the process lifetime.
            let _telemetry_ref = Box::leak(Box::new(telemetry));
            Ok(())
        })
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_project,
            create_project,
            add_track,
            patch_track_state,
            reorder_track,
            add_midi_clip,
            move_clip,
            add_effect,
            set_playback,
            set_loop_region,
            export_project,
            save_project,
            load_project,
            autosave_project,
            measure_parity
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            error!(?error, "error while running voltlane tauri application");
            panic!("tauri runtime failed: {error}");
        });
}
