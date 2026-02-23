use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_PPQ: u16 = 480;
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

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
            effects: Vec::new(),
            clips: Vec::new(),
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
    pub notes: Vec<MidiNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioClip {
    pub source_path: String,
    pub gain_db: f32,
    pub pan: f32,
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
