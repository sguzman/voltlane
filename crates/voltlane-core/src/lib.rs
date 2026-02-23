pub mod assets;
pub mod diagnostics;
pub mod engine;
pub mod export;
pub mod fixtures;
pub mod model;
pub mod parity;
pub mod persistence;
pub mod time;

pub use assets::{
    AudioAnalysis, AudioAssetEntry, AudioWaveformPeaks, DecodedAudio, analyze_audio_file,
    analyze_audio_file_with_cache, decode_audio_file_mono, scan_audio_assets,
};
pub use diagnostics::{
    TelemetryGuard, init_tracing, init_tracing_with_file_prefix, init_tracing_with_options,
};
pub use engine::{
    AddClipRequest, AddTrackRequest, AudioClipPatch, Engine, EngineError, ExportKind, RenderMode,
    TrackMixPatch, TrackStatePatch,
};
pub use model::{
    AudioClip, AutomationClip, AutomationPoint, ChipMacroLane, Clip, ClipPayload,
    DEFAULT_TRACKER_LINES_PER_BEAT, EffectSpec, MidiClip, MidiNote, PatternClip, Project, Track,
    TrackKind, TrackSend, TrackerRow, Transport,
};
pub use parity::{ParityReport, generate_parity_report};
