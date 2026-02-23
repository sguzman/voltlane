use std::path::Path;

use parking_lot::Mutex;
use serde::Deserialize;
use tauri::{Manager, State};
use tauri_plugin_log::{Target, TargetKind, log::LevelFilter};
use tracing::{error, info, instrument};
use uuid::Uuid;
use voltlane_core::{
    AddClipRequest, AddTrackRequest, ClipPayload, Engine, ExportKind, MidiClip, MidiNote,
    ParityReport, PatternClip, Project, TrackStatePatch, init_tracing,
};

#[derive(Default)]
struct AppState {
    engine: Mutex<Engine>,
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
        input.bpm.unwrap_or(140.0),
        input.sample_rate.unwrap_or(48_000),
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
    let ffmpeg_binary = input.ffmpeg_binary.as_deref().map(Path::new);
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
    let engine = state.engine.lock();
    let path = engine
        .autosave(Path::new(&autosave_dir))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Info)
                .level_for("voltlane_core", LevelFilter::Trace)
                .level_for("voltlane_tauri", LevelFilter::Trace)
                .target(Target::new(TargetKind::Stdout))
                .target(Target::new(TargetKind::Webview))
                .target(Target::new(TargetKind::LogDir {
                    file_name: Some("voltlane-tauri".to_string()),
                }))
                .build(),
        )
        .setup(|app| {
            let log_dir = app
                .path()
                .app_log_dir()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let telemetry = init_tracing(&log_dir)
                .map_err(|error| anyhow::anyhow!("tracing init failed: {error}"))?;
            info!(
                session_id = %telemetry.session_id,
                log_dir = %log_dir.display(),
                "voltlane tauri setup complete"
            );

            // Leak once at startup to keep worker guard alive for the process lifetime.
            let _telemetry_ref = Box::leak(Box::new(telemetry));
            Ok(())
        })
        .manage(AppState::default())
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
