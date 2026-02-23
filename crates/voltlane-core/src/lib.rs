pub mod diagnostics;
pub mod engine;
pub mod export;
pub mod fixtures;
pub mod model;
pub mod parity;
pub mod persistence;
pub mod time;

pub use diagnostics::{TelemetryGuard, init_tracing};
pub use engine::{
    AddClipRequest, AddTrackRequest, Engine, EngineError, ExportKind, TrackStatePatch,
};
pub use model::{
    AudioClip, AutomationClip, AutomationPoint, Clip, ClipPayload, EffectSpec, MidiClip, MidiNote,
    PatternClip, Project, Track, TrackKind, Transport,
};
pub use parity::{ParityReport, generate_parity_report};
