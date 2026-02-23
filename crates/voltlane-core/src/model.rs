use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_PPQ: u16 = 480;
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub const DEFAULT_TRACKER_LINES_PER_BEAT: u16 = 4;
pub const DEFAULT_TRACK_GAIN_DB: f32 = 0.0;
pub const DEFAULT_TRACK_PAN: f32 = 0.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: Uuid,
    pub session_id: Uuid,
    pub title: String,
    pub bpm: f64,
    pub ppq: u16,
    pub sample_rate: u32,
    pub transport: Transport,
    pub tracks: Vec<Track>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    #[must_use]
    pub fn new(title: impl Into<String>, bpm: f64, sample_rate: u32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            title: title.into(),
            bpm,
            ppq: DEFAULT_PPQ,
            sample_rate,
            transport: Transport::default(),
            tracks: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    #[must_use]
    pub fn clip_count(&self) -> usize {
        self.tracks.iter().map(|track| track.clips.len()).sum()
    }

    #[must_use]
    pub fn note_count(&self) -> usize {
        self.tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(Clip::note_count)
            .sum()
    }

    #[must_use]
    pub fn max_tick(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(Clip::end_tick)
            .max()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transport {
    pub playhead_tick: u64,
    pub loop_enabled: bool,
    pub loop_start_tick: u64,
    pub loop_end_tick: u64,
    pub metronome_enabled: bool,
    pub is_playing: bool,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playhead_tick: 0,
            loop_enabled: false,
            loop_start_tick: 0,
            loop_end_tick: u64::from(DEFAULT_PPQ) * 4,
            metronome_enabled: true,
            is_playing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub kind: TrackKind,
    pub hidden: bool,
    pub mute: bool,
    pub solo: bool,
    pub enabled: bool,
    #[serde(
        default = "default_track_gain_db",
        skip_serializing_if = "is_default_track_gain_db"
    )]
    pub gain_db: f32,
    #[serde(
        default = "default_track_pan",
        skip_serializing_if = "is_default_track_pan"
    )]
    pub pan: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bus: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<TrackSend>,
    pub effects: Vec<EffectSpec>,
    pub clips: Vec<Clip>,
}

impl Track {
    #[must_use]
    pub fn new(name: impl Into<String>, color: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            color: color.into(),
            kind,
            hidden: false,
            mute: false,
            solo: false,
            enabled: true,
            gain_db: default_track_gain_db(),
            pan: default_track_pan(),
            output_bus: None,
            sends: Vec::new(),
            effects: Vec::new(),
            clips: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TrackSend {
    pub id: Uuid,
    pub target_bus: Uuid,
    pub level_db: f32,
    pub pan: f32,
    pub pre_fader: bool,
    pub enabled: bool,
}

impl Default for TrackSend {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            target_bus: Uuid::nil(),
            level_db: 0.0,
            pan: 0.0,
            pre_fader: false,
            enabled: true,
        }
    }
}

impl TrackSend {
    #[must_use]
    pub fn new(target_bus: Uuid) -> Self {
        Self {
            target_bus,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Midi,
    Chip,
    Audio,
    Automation,
    Bus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectSpec {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub params: BTreeMap<String, f32>,
}

impl EffectSpec {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            params: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: Uuid,
    pub name: String,
    pub start_tick: u64,
    pub length_ticks: u64,
    pub disabled: bool,
    pub payload: ClipPayload,
}

impl Clip {
    #[must_use]
    pub fn end_tick(&self) -> u64 {
        self.start_tick.saturating_add(self.length_ticks)
    }

    #[must_use]
    pub fn note_count(&self) -> usize {
        match &self.payload {
            ClipPayload::Midi(midi) => midi.notes.len(),
            ClipPayload::Pattern(pattern) => pattern.notes.len(),
            ClipPayload::Audio(_) | ClipPayload::Automation(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClipPayload {
    Midi(MidiClip),
    Pattern(PatternClip),
    Audio(AudioClip),
    Automation(AutomationClip),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiClip {
    pub instrument: Option<String>,
    pub notes: Vec<MidiNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatternClip {
    pub source_chip: String,
    #[serde(default)]
    pub notes: Vec<MidiNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<TrackerRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<ChipMacroLane>,
    #[serde(
        default = "default_tracker_lines_per_beat",
        skip_serializing_if = "is_default_tracker_lines_per_beat"
    )]
    pub lines_per_beat: u16,
}

impl Default for PatternClip {
    fn default() -> Self {
        Self {
            source_chip: String::new(),
            notes: Vec::new(),
            rows: Vec::new(),
            macros: Vec::new(),
            lines_per_beat: default_tracker_lines_per_beat(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChipMacroLane {
    pub target: String,
    pub enabled: bool,
    pub values: Vec<i16>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
}

impl Default for ChipMacroLane {
    fn default() -> Self {
        Self {
            target: String::new(),
            enabled: true,
            values: Vec::new(),
            loop_start: None,
            loop_end: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TrackerRow {
    pub row: u32,
    pub note: Option<u8>,
    pub velocity: u8,
    pub gate: bool,
    pub effect: Option<String>,
    pub effect_value: Option<u16>,
}

impl Default for TrackerRow {
    fn default() -> Self {
        Self {
            row: 0,
            note: None,
            velocity: 100,
            gate: false,
            effect: None,
            effect_value: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioClip {
    pub source_path: String,
    pub gain_db: f32,
    pub pan: f32,
    pub source_sample_rate: u32,
    pub source_channels: u16,
    pub source_duration_seconds: f64,
    pub trim_start_seconds: f64,
    pub trim_end_seconds: f64,
    pub fade_in_seconds: f64,
    pub fade_out_seconds: f64,
    pub reverse: bool,
    pub stretch_ratio: f32,
    pub waveform_bucket_size: usize,
    pub waveform_peaks: Vec<f32>,
    pub waveform_cache_path: Option<String>,
}

impl Default for AudioClip {
    fn default() -> Self {
        Self {
            source_path: String::new(),
            gain_db: 0.0,
            pan: 0.0,
            source_sample_rate: DEFAULT_SAMPLE_RATE,
            source_channels: 2,
            source_duration_seconds: 0.0,
            trim_start_seconds: 0.0,
            trim_end_seconds: 0.0,
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
            reverse: false,
            stretch_ratio: 1.0,
            waveform_bucket_size: 1024,
            waveform_peaks: Vec::new(),
            waveform_cache_path: None,
        }
    }
}

impl AudioClip {
    #[must_use]
    pub fn normalized_trim_range(&self) -> (f64, f64) {
        let source_duration = self.source_duration_seconds.max(0.0);
        let trim_start = self.trim_start_seconds.clamp(0.0, source_duration);
        let trim_end = self.trim_end_seconds.clamp(trim_start, source_duration);
        (trim_start, trim_end)
    }

    #[must_use]
    pub fn effective_duration_seconds(&self) -> f64 {
        let (trim_start, trim_end) = self.normalized_trim_range();
        let trimmed_duration = (trim_end - trim_start).max(0.0);
        trimmed_duration * f64::from(self.stretch_ratio.max(0.01))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationClip {
    pub target_parameter_id: String,
    pub points: Vec<AutomationPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationPoint {
    pub tick: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiNote {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u64,
    pub length_ticks: u64,
    pub channel: u8,
}

impl MidiNote {
    #[must_use]
    pub fn end_tick(&self) -> u64 {
        self.start_tick.saturating_add(self.length_ticks)
    }
}

const fn default_tracker_lines_per_beat() -> u16 {
    DEFAULT_TRACKER_LINES_PER_BEAT
}

const fn default_track_gain_db() -> f32 {
    DEFAULT_TRACK_GAIN_DB
}

const fn default_track_pan() -> f32 {
    DEFAULT_TRACK_PAN
}

fn is_default_track_gain_db(value: &f32) -> bool {
    (*value - DEFAULT_TRACK_GAIN_DB).abs() <= f32::EPSILON
}

fn is_default_track_pan(value: &f32) -> bool {
    (*value - DEFAULT_TRACK_PAN).abs() <= f32::EPSILON
}

const fn is_default_tracker_lines_per_beat(value: &u16) -> bool {
    *value == DEFAULT_TRACKER_LINES_PER_BEAT
}
