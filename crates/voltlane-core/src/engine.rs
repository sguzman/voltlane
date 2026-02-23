use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::{
    assets::{
        AudioAnalysis, AudioAssetEntry, analyze_audio_file, analyze_audio_file_with_cache,
        scan_audio_assets,
    },
    export,
    model::{
        AudioClip, AutomationClip, AutomationPoint, ChipMacroLane, Clip, ClipPayload,
        DEFAULT_SAMPLE_RATE, EffectSpec, MidiNote, PatternClip, Project, Track, TrackKind,
        TrackSend, TrackerRow,
    },
    persistence,
    time::{seconds_to_ticks, tracker_rows_to_ticks},
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("track not found: {0}")]
    TrackNotFound(Uuid),
    #[error("track {track_id} is not an audio track (found: {kind:?})")]
    InvalidAudioTrack { track_id: Uuid, kind: TrackKind },
    #[error("clip not found: {0}")]
    ClipNotFound(Uuid),
    #[error("clip does not support midi note editing: {0}")]
    UnsupportedClipPayload(Uuid),
    #[error("clip is not an audio clip: {0}")]
    UnsupportedAudioClip(Uuid),
    #[error("clip is not an automation clip: {0}")]
    UnsupportedAutomationClip(Uuid),
    #[error("clip is not a pattern clip: {0}")]
    UnsupportedPatternClip(Uuid),
    #[error("track {track_id} has invalid bus target: {target_bus}")]
    InvalidBusTarget { track_id: Uuid, target_bus: Uuid },
    #[error("track {track_id} has invalid send target: {target_bus}")]
    InvalidTrackSend { track_id: Uuid, target_bus: Uuid },
    #[error("track send not found: {0}")]
    SendNotFound(Uuid),
    #[error("routing graph contains a cycle")]
    RoutingCycleDetected,
    #[error("invalid quantize grid ticks: {0}")]
    InvalidQuantizeGrid(u64),
    #[error("invalid tracker lines_per_beat: {0}")]
    InvalidTrackerLinesPerBeat(u16),
    #[error("invalid note index: {0}")]
    InvalidNoteIndex(usize),
    #[error("invalid reorder from {from} to {to}")]
    InvalidReorder { from: usize, to: usize },
    #[error("invalid audio trim range: start={start_seconds:.3}s end={end_seconds:.3}s")]
    InvalidAudioTrimRange {
        start_seconds: f64,
        end_seconds: f64,
    },
    #[error("invalid audio stretch ratio: {0}")]
    InvalidAudioStretchRatio(f32),
    #[error("invalid audio analysis bucket size: {0}")]
    InvalidAudioBucketSize(usize),
    #[error("io error: {0}")]
    Io(String),
}

impl From<anyhow::Error> for EngineError {
    fn from(value: anyhow::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTrackRequest {
    pub name: String,
    pub color: String,
    pub kind: TrackKind,
}

impl Default for AddTrackRequest {
    fn default() -> Self {
        Self {
            name: "Track".to_string(),
            color: "#52e1c4".to_string(),
            kind: TrackKind::Midi,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddClipRequest {
    pub track_id: Uuid,
    pub name: String,
    pub start_tick: u64,
    pub length_ticks: u64,
    pub payload: ClipPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackStatePatch {
    pub hidden: Option<bool>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioClipPatch {
    pub gain_db: Option<f32>,
    pub pan: Option<f32>,
    pub trim_start_seconds: Option<f64>,
    pub trim_end_seconds: Option<f64>,
    pub fade_in_seconds: Option<f64>,
    pub fade_out_seconds: Option<f64>,
    pub reverse: Option<bool>,
    pub stretch_ratio: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackMixPatch {
    pub gain_db: Option<f32>,
    pub pan: Option<f32>,
    pub output_bus: Option<Option<Uuid>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Midi,
    Wav,
    Mp3,
    StemWav,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    Offline,
    Realtime,
}

impl Default for RenderMode {
    fn default() -> Self {
        Self::Offline
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    project: Project,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            project: Project::new("Untitled", 140.0, DEFAULT_SAMPLE_RATE),
        }
    }
}

impl Engine {
    #[must_use]
    pub fn new(project: Project) -> Self {
        Self { project }
    }

    #[must_use]
    pub fn project(&self) -> &Project {
        &self.project
    }

    #[instrument(skip(self), fields(title = %title, bpm, sample_rate))]
    pub fn create_project(&mut self, title: String, bpm: f64, sample_rate: u32) {
        self.project = Project::new(title, bpm.max(20.0), sample_rate.max(8_000));
        info!(project_id = %self.project.id, "project created");
    }

    #[instrument(skip(self, project), fields(project_id = %project.id))]
    pub fn replace_project(&mut self, project: Project) {
        self.project = project;
        info!(project_id = %self.project.id, "project replaced");
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_name = %request.name, track_kind = ?request.kind))]
    pub fn add_track(&mut self, request: AddTrackRequest) -> Track {
        let track = Track::new(request.name, request.color, request.kind);
        self.project.tracks.push(track.clone());
        self.project.touch();
        info!(track_id = %track.id, "track added");
        track
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id))]
    pub fn remove_track(&mut self, track_id: Uuid) -> Result<(), EngineError> {
        let before = self.project.tracks.len();
        self.project.tracks.retain(|track| track.id != track_id);
        if self.project.tracks.len() == before {
            return Err(EngineError::TrackNotFound(track_id));
        }
        self.project.touch();
        info!("track removed");
        Ok(())
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, from, to))]
    pub fn reorder_track(&mut self, from: usize, to: usize) -> Result<(), EngineError> {
        if from >= self.project.tracks.len() || to >= self.project.tracks.len() {
            return Err(EngineError::InvalidReorder { from, to });
        }
        if from == to {
            debug!("reorder noop");
            return Ok(());
        }

        let track = self.project.tracks.remove(from);
        self.project.tracks.insert(to, track);
        self.project.touch();
        info!("track reordered");
        Ok(())
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id))]
    pub fn patch_track_state(
        &mut self,
        track_id: Uuid,
        patch: TrackStatePatch,
    ) -> Result<Track, EngineError> {
        let updated_track = {
            let track = self
                .project
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
                .ok_or(EngineError::TrackNotFound(track_id))?;

            if let Some(hidden) = patch.hidden {
                track.hidden = hidden;
            }
            if let Some(mute) = patch.mute {
                track.mute = mute;
            }
            if let Some(solo) = patch.solo {
                track.solo = solo;
            }
            if let Some(enabled) = patch.enabled {
                track.enabled = enabled;
            }

            track.clone()
        };
        self.project.touch();
        info!(
            hidden = updated_track.hidden,
            mute = updated_track.mute,
            enabled = updated_track.enabled,
            "track state patched"
        );
        Ok(updated_track)
    }

    #[instrument(skip(self, effect), fields(project_id = %self.project.id, track_id = %track_id, effect = %effect.name))]
    pub fn add_effect(
        &mut self,
        track_id: Uuid,
        mut effect: EffectSpec,
    ) -> Result<EffectSpec, EngineError> {
        populate_builtin_effect_defaults(&mut effect);
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?;

        track.effects.push(effect.clone());
        self.project.touch();
        info!(effect_id = %effect.id, "effect added to track");
        Ok(effect)
    }

    #[must_use]
    pub fn automation_parameter_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for track in &self.project.tracks {
            ids.push(format!("track:{}:gain_db", track.id));
            ids.push(format!("track:{}:pan", track.id));
            for effect in &track.effects {
                for key in effect.params.keys() {
                    ids.push(format!("track:{}:effect:{}:{}", track.id, effect.id, key));
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    #[instrument(skip(self, patch), fields(project_id = %self.project.id, track_id = %track_id))]
    pub fn patch_track_mix(
        &mut self,
        track_id: Uuid,
        patch: TrackMixPatch,
    ) -> Result<Track, EngineError> {
        let mut candidate_tracks = self.project.tracks.clone();
        if let Some(Some(target_bus)) = patch.output_bus {
            validate_bus_target(&candidate_tracks, track_id, target_bus)?;
        }
        let track = candidate_tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?;

        if let Some(gain_db) = patch.gain_db {
            track.gain_db = gain_db.clamp(-96.0, 12.0);
        }
        if let Some(pan) = patch.pan {
            track.pan = pan.clamp(-1.0, 1.0);
        }
        if let Some(output_bus) = patch.output_bus {
            track.output_bus = output_bus;
        }

        validate_routing_graph(&candidate_tracks)?;
        self.project.tracks = candidate_tracks;
        self.project.touch();

        let updated = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?
            .clone();
        info!(
            gain_db = updated.gain_db,
            pan = updated.pan,
            output_bus = ?updated.output_bus,
            "track mix patched"
        );
        Ok(updated)
    }

    #[instrument(skip(self, send), fields(project_id = %self.project.id, track_id = %track_id, send_id = %send.id))]
    pub fn upsert_track_send(
        &mut self,
        track_id: Uuid,
        mut send: TrackSend,
    ) -> Result<Track, EngineError> {
        let mut candidate_tracks = self.project.tracks.clone();
        sanitize_track_send(&mut send);
        validate_bus_target(&candidate_tracks, track_id, send.target_bus)?;

        let track = candidate_tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?;

        if let Some(existing) = track
            .sends
            .iter_mut()
            .find(|existing| existing.id == send.id)
        {
            *existing = send.clone();
        } else {
            track.sends.push(send);
        }

        validate_routing_graph(&candidate_tracks)?;
        self.project.tracks = candidate_tracks;
        self.project.touch();

        let updated = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?
            .clone();
        info!(send_count = updated.sends.len(), "track send upserted");
        Ok(updated)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, send_id = %send_id))]
    pub fn remove_track_send(
        &mut self,
        track_id: Uuid,
        send_id: Uuid,
    ) -> Result<Track, EngineError> {
        let mut candidate_tracks = self.project.tracks.clone();
        let track = candidate_tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?;

        let before = track.sends.len();
        track.sends.retain(|send| send.id != send_id);
        if track.sends.len() == before {
            return Err(EngineError::SendNotFound(send_id));
        }

        validate_routing_graph(&candidate_tracks)?;
        self.project.tracks = candidate_tracks;
        self.project.touch();

        let updated = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?
            .clone();
        info!(send_count = updated.sends.len(), "track send removed");
        Ok(updated)
    }

    #[instrument(skip(self, points), fields(project_id = %self.project.id, track_id = %track_id, clip_name = %name, points = points.len()))]
    pub fn add_automation_clip(
        &mut self,
        track_id: Uuid,
        name: String,
        start_tick: u64,
        length_ticks: u64,
        target_parameter_id: String,
        mut points: Vec<AutomationPoint>,
    ) -> Result<Clip, EngineError> {
        sanitize_automation_points(&mut points);
        let target_parameter_id = sanitize_automation_target_id(target_parameter_id, track_id);

        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))?;

        let clip = Clip {
            id: Uuid::new_v4(),
            name,
            start_tick,
            length_ticks: length_ticks.max(1),
            disabled: false,
            payload: ClipPayload::Automation(AutomationClip {
                target_parameter_id,
                points,
            }),
        };

        track.clips.push(clip.clone());
        self.project.touch();
        info!(clip_id = %clip.id, "automation clip added");
        Ok(clip)
    }

    #[instrument(skip(self, points), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, points = points.len()))]
    pub fn upsert_automation_clip(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        target_parameter_id: Option<String>,
        mut points: Vec<AutomationPoint>,
    ) -> Result<Clip, EngineError> {
        sanitize_automation_points(&mut points);
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let automation =
                clip_automation_mut(clip).ok_or(EngineError::UnsupportedAutomationClip(clip_id))?;

            if let Some(target_parameter_id) = target_parameter_id {
                automation.target_parameter_id =
                    sanitize_automation_target_id(target_parameter_id, track_id);
            }
            automation.points = points;
            clip.clone()
        };

        self.project.touch();
        info!("automation clip updated");
        Ok(updated_clip)
    }

    #[instrument(skip(self, request), fields(project_id = %self.project.id, track_id = %request.track_id, clip_name = %request.name))]
    pub fn add_clip(&mut self, request: AddClipRequest) -> Result<Clip, EngineError> {
        let mut payload = request.payload;
        if let ClipPayload::Pattern(pattern) = &mut payload {
            normalize_pattern_clip(pattern, self.project.ppq)?;
        }

        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == request.track_id)
            .ok_or(EngineError::TrackNotFound(request.track_id))?;

        let clip = Clip {
            id: Uuid::new_v4(),
            name: request.name,
            start_tick: request.start_tick,
            length_ticks: request.length_ticks.max(1),
            disabled: false,
            payload,
        };

        track.clips.push(clip.clone());
        self.project.touch();
        info!(clip_id = %clip.id, "clip added");
        Ok(clip)
    }

    #[instrument(skip(self), fields(directory = %directory.display()))]
    pub fn scan_audio_assets(&self, directory: &Path) -> Result<Vec<AudioAssetEntry>, EngineError> {
        scan_audio_assets(directory).map_err(Into::into)
    }

    #[instrument(skip(self), fields(path = %path.display(), bucket_size, cache_dir = %cache_dir.display()))]
    pub fn analyze_audio_asset(
        &self,
        path: &Path,
        cache_dir: &Path,
        bucket_size: usize,
    ) -> Result<AudioAnalysis, EngineError> {
        if bucket_size == 0 {
            return Err(EngineError::InvalidAudioBucketSize(bucket_size));
        }
        analyze_audio_file_with_cache(path, cache_dir, bucket_size).map_err(Into::into)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, source_path = %source_path.display(), start_tick, bucket_size, cache_dir = ?cache_dir.map(|value| value.display().to_string())))]
    pub fn import_audio_clip(
        &mut self,
        track_id: Uuid,
        name: String,
        source_path: &Path,
        start_tick: u64,
        bucket_size: usize,
        cache_dir: Option<&Path>,
        default_gain_db: f32,
        default_pan: f32,
    ) -> Result<Clip, EngineError> {
        if bucket_size == 0 {
            return Err(EngineError::InvalidAudioBucketSize(bucket_size));
        }

        let analysis = if let Some(cache_dir) = cache_dir {
            analyze_audio_file_with_cache(source_path, cache_dir, bucket_size)?
        } else {
            analyze_audio_file(source_path, bucket_size)?
        };

        let mut audio = AudioClip {
            source_path: analysis.source_path.clone(),
            gain_db: default_gain_db.clamp(-96.0, 12.0),
            pan: default_pan.clamp(-1.0, 1.0),
            source_sample_rate: analysis.sample_rate,
            source_channels: analysis.channels.max(1),
            source_duration_seconds: analysis.duration_seconds.max(0.0),
            trim_start_seconds: 0.0,
            trim_end_seconds: analysis.duration_seconds.max(0.0),
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
            reverse: false,
            stretch_ratio: 1.0,
            waveform_bucket_size: analysis.peaks.bucket_size,
            waveform_peaks: analysis.peaks.peaks.clone(),
            waveform_cache_path: analysis.cache_path.clone(),
        };
        sanitize_audio_clip(&mut audio)?;
        let length_ticks = seconds_to_ticks(
            audio.effective_duration_seconds(),
            self.project.bpm,
            self.project.ppq,
        )
        .max(1);

        let clip = Clip {
            id: Uuid::new_v4(),
            name,
            start_tick,
            length_ticks,
            disabled: false,
            payload: ClipPayload::Audio(audio),
        };

        let track = self.find_track_mut(track_id)?;
        if !matches!(track.kind, TrackKind::Audio) {
            return Err(EngineError::InvalidAudioTrack {
                track_id,
                kind: track.kind.clone(),
            });
        }

        track.clips.push(clip.clone());
        self.project.touch();
        info!(clip_id = %clip.id, "audio clip imported");
        Ok(clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id))]
    pub fn patch_audio_clip(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        patch: AudioClipPatch,
    ) -> Result<Clip, EngineError> {
        if let Some(stretch_ratio) = patch.stretch_ratio
            && stretch_ratio <= 0.0
        {
            return Err(EngineError::InvalidAudioStretchRatio(stretch_ratio));
        }

        let bpm = self.project.bpm;
        let ppq = self.project.ppq;
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let audio = match &mut clip.payload {
                ClipPayload::Audio(audio) => audio,
                _ => return Err(EngineError::UnsupportedAudioClip(clip_id)),
            };

            if let Some(gain_db) = patch.gain_db {
                audio.gain_db = gain_db.clamp(-96.0, 12.0);
            }
            if let Some(pan) = patch.pan {
                audio.pan = pan.clamp(-1.0, 1.0);
            }
            if let Some(trim_start_seconds) = patch.trim_start_seconds {
                audio.trim_start_seconds = trim_start_seconds.max(0.0);
            }
            if let Some(trim_end_seconds) = patch.trim_end_seconds {
                audio.trim_end_seconds = trim_end_seconds.max(0.0);
            }
            if let Some(fade_in_seconds) = patch.fade_in_seconds {
                audio.fade_in_seconds = fade_in_seconds.max(0.0);
            }
            if let Some(fade_out_seconds) = patch.fade_out_seconds {
                audio.fade_out_seconds = fade_out_seconds.max(0.0);
            }
            if let Some(reverse) = patch.reverse {
                audio.reverse = reverse;
            }
            if let Some(stretch_ratio) = patch.stretch_ratio {
                audio.stretch_ratio = stretch_ratio.max(0.01);
            }

            sanitize_audio_clip(audio)?;
            clip.length_ticks =
                seconds_to_ticks(audio.effective_duration_seconds(), bpm, ppq).max(1);
            clip.clone()
        };

        self.project.touch();
        info!("audio clip patched");
        Ok(updated_clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, clip_id = %clip_id, track_id = %track_id))]
    pub fn move_clip(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        start_tick: u64,
        length_ticks: u64,
    ) -> Result<Clip, EngineError> {
        let updated_clip = {
            let track = self
                .project
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
                .ok_or(EngineError::TrackNotFound(track_id))?;

            let clip = track
                .clips
                .iter_mut()
                .find(|clip| clip.id == clip_id)
                .ok_or(EngineError::ClipNotFound(clip_id))?;

            clip.start_tick = start_tick;
            clip.length_ticks = length_ticks.max(1);
            clip.clone()
        };
        self.project.touch();
        info!("clip moved/resized");
        Ok(updated_clip)
    }

    #[instrument(skip(self, notes), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, notes = notes.len()))]
    pub fn upsert_clip_notes(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        mut notes: Vec<MidiNote>,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        for note in &mut notes {
            sanitize_note(note);
        }

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            {
                let target =
                    clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
                *target = notes;
            }
            if let Some(pattern) = clip_pattern_mut(clip) {
                sync_pattern_rows_from_notes(pattern, ppq)?;
            }
            clip.clone()
        };

        self.project.touch();
        info!("clip notes replaced");
        Ok(updated_clip)
    }

    #[instrument(skip(self, note), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id))]
    pub fn add_clip_note(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        mut note: MidiNote,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        sanitize_note(&mut note);

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            {
                let notes =
                    clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
                notes.push(note);
                notes.sort_by_key(|candidate| candidate.start_tick);
            }
            if let Some(pattern) = clip_pattern_mut(clip) {
                sync_pattern_rows_from_notes(pattern, ppq)?;
            }
            clip.clone()
        };

        self.project.touch();
        info!("note added to clip");
        Ok(updated_clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, note_index))]
    pub fn remove_clip_note(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        note_index: usize,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            {
                let notes =
                    clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
                if note_index >= notes.len() {
                    return Err(EngineError::InvalidNoteIndex(note_index));
                }
                notes.remove(note_index);
            }
            if let Some(pattern) = clip_pattern_mut(clip) {
                sync_pattern_rows_from_notes(pattern, ppq)?;
            }
            clip.clone()
        };

        self.project.touch();
        info!("note removed from clip");
        Ok(updated_clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, semitones))]
    pub fn transpose_clip_notes(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        semitones: i16,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            {
                let notes =
                    clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
                for note in notes {
                    let pitch = i16::from(note.pitch)
                        .saturating_add(semitones)
                        .clamp(0, 127) as u8;
                    note.pitch = pitch;
                }
            }
            if let Some(pattern) = clip_pattern_mut(clip) {
                sync_pattern_rows_from_notes(pattern, ppq)?;
            }
            clip.clone()
        };

        self.project.touch();
        info!("clip notes transposed");
        Ok(updated_clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, grid_ticks))]
    pub fn quantize_clip_notes(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        grid_ticks: u64,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        if grid_ticks == 0 {
            return Err(EngineError::InvalidQuantizeGrid(grid_ticks));
        }

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            {
                let notes =
                    clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;

                for note in notes.iter_mut() {
                    note.start_tick = round_to_grid(note.start_tick, grid_ticks);
                    note.length_ticks =
                        round_to_grid(note.length_ticks.max(1), grid_ticks).max(grid_ticks);
                    sanitize_note(note);
                }
                notes.sort_by_key(|candidate| candidate.start_tick);
            }
            if let Some(pattern) = clip_pattern_mut(clip) {
                sync_pattern_rows_from_notes(pattern, ppq)?;
            }
            clip.clone()
        };

        self.project.touch();
        info!("clip notes quantized");
        Ok(updated_clip)
    }

    #[instrument(skip(self, rows), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, rows = rows.len(), lines_per_beat = ?lines_per_beat))]
    pub fn upsert_pattern_rows(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        mut rows: Vec<TrackerRow>,
        lines_per_beat: Option<u16>,
    ) -> Result<Clip, EngineError> {
        let ppq = self.project.ppq;
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let pattern =
                clip_pattern_mut(clip).ok_or(EngineError::UnsupportedPatternClip(clip_id))?;
            if let Some(lines_per_beat) = lines_per_beat {
                pattern.lines_per_beat = lines_per_beat;
            }

            for row in &mut rows {
                sanitize_tracker_row(row);
            }
            pattern.rows = rows;
            normalize_pattern_clip(pattern, ppq)?;
            clip.clone()
        };

        self.project.touch();
        info!("pattern rows replaced");
        Ok(updated_clip)
    }

    #[instrument(skip(self, macros), fields(project_id = %self.project.id, track_id = %track_id, clip_id = %clip_id, macros = macros.len()))]
    pub fn upsert_pattern_macros(
        &mut self,
        track_id: Uuid,
        clip_id: Uuid,
        mut macros: Vec<ChipMacroLane>,
    ) -> Result<Clip, EngineError> {
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let pattern =
                clip_pattern_mut(clip).ok_or(EngineError::UnsupportedPatternClip(clip_id))?;

            for lane in &mut macros {
                sanitize_chip_macro_lane(lane);
            }
            pattern.macros = macros;
            clip.clone()
        };

        self.project.touch();
        info!("pattern macros replaced");
        Ok(updated_clip)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id))]
    pub fn toggle_playback(&mut self, is_playing: bool) {
        self.project.transport.is_playing = is_playing;
        self.project.touch();
        info!(is_playing, "transport state changed");
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, loop_start_tick, loop_end_tick, loop_enabled))]
    pub fn set_loop_region(
        &mut self,
        loop_start_tick: u64,
        loop_end_tick: u64,
        loop_enabled: bool,
    ) {
        if loop_end_tick <= loop_start_tick {
            warn!("ignored invalid loop range");
            return;
        }

        self.project.transport.loop_start_tick = loop_start_tick;
        self.project.transport.loop_end_tick = loop_end_tick;
        self.project.transport.loop_enabled = loop_enabled;
        self.project.touch();
        info!("loop region updated");
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, path = %path.display()))]
    pub fn save_project(&self, path: &Path) -> Result<(), EngineError> {
        persistence::save_project(path, &self.project)?;
        Ok(())
    }

    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn load_project(&mut self, path: &Path) -> Result<Project, EngineError> {
        let project = persistence::load_project(path)?;
        self.replace_project(project.clone());
        Ok(project)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, autosave_dir = %autosave_dir.display()))]
    pub fn autosave(&self, autosave_dir: &Path) -> Result<PathBuf, EngineError> {
        let autosave_path = persistence::autosave_project(&self.project, autosave_dir)?;
        Ok(autosave_path)
    }

    #[instrument(skip(self), fields(project_id = %self.project.id, kind = ?kind, path = %output_path.display()))]
    pub fn export(
        &self,
        kind: ExportKind,
        output_path: &Path,
        ffmpeg_binary: Option<&Path>,
        render_mode: RenderMode,
    ) -> Result<(), EngineError> {
        match kind {
            ExportKind::Midi => export::export_midi(&self.project, output_path)?,
            ExportKind::Wav => export::export_wav(&self.project, output_path, render_mode)?,
            ExportKind::Mp3 => {
                export::export_mp3(&self.project, output_path, ffmpeg_binary, render_mode)?
            }
            ExportKind::StemWav => {
                let _paths = export::export_stem_wav(&self.project, output_path, render_mode)?;
            }
        }
        Ok(())
    }

    fn find_track_mut(&mut self, track_id: Uuid) -> Result<&mut Track, EngineError> {
        self.project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or(EngineError::TrackNotFound(track_id))
    }

    fn find_clip_mut(&mut self, track_id: Uuid, clip_id: Uuid) -> Result<&mut Clip, EngineError> {
        let track = self.find_track_mut(track_id)?;

        track
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or(EngineError::ClipNotFound(clip_id))
    }
}

fn clip_note_vec_mut(clip: &mut Clip) -> Option<&mut Vec<MidiNote>> {
    match &mut clip.payload {
        ClipPayload::Midi(midi) => Some(&mut midi.notes),
        ClipPayload::Pattern(pattern) => Some(&mut pattern.notes),
        ClipPayload::Audio(_) | ClipPayload::Automation(_) => None,
    }
}

fn clip_pattern_mut(clip: &mut Clip) -> Option<&mut PatternClip> {
    match &mut clip.payload {
        ClipPayload::Pattern(pattern) => Some(pattern),
        ClipPayload::Midi(_) | ClipPayload::Audio(_) | ClipPayload::Automation(_) => None,
    }
}

fn clip_automation_mut(clip: &mut Clip) -> Option<&mut AutomationClip> {
    match &mut clip.payload {
        ClipPayload::Automation(automation) => Some(automation),
        ClipPayload::Midi(_) | ClipPayload::Audio(_) | ClipPayload::Pattern(_) => None,
    }
}

fn sanitize_note(note: &mut MidiNote) {
    note.pitch = note.pitch.min(127);
    note.velocity = note.velocity.min(127);
    note.channel = note.channel.min(15);
    note.length_ticks = note.length_ticks.max(1);
}

fn sanitize_tracker_row(row: &mut TrackerRow) {
    row.velocity = row.velocity.min(127);
    if let Some(note) = row.note {
        row.note = Some(note.min(127));
    }
    if row.effect.as_deref().is_some_and(str::is_empty) {
        row.effect = None;
    }
    if row.effect.is_none() {
        row.effect_value = None;
    }
}

fn sanitize_chip_macro_lane(lane: &mut ChipMacroLane) {
    lane.target = lane.target.trim().to_ascii_lowercase();
    lane.values.truncate(256);
    for value in &mut lane.values {
        *value = (*value).clamp(-127, 127);
    }

    if lane.target.is_empty() || lane.values.is_empty() {
        lane.enabled = false;
    }

    match (lane.loop_start, lane.loop_end) {
        (Some(start), Some(end)) if start <= end && end < lane.values.len() => {}
        _ => {
            lane.loop_start = None;
            lane.loop_end = None;
        }
    }
}

fn sanitize_track_send(send: &mut TrackSend) {
    if send.id.is_nil() {
        send.id = Uuid::new_v4();
    }
    send.level_db = send.level_db.clamp(-96.0, 12.0);
    send.pan = send.pan.clamp(-1.0, 1.0);
}

fn sanitize_automation_points(points: &mut Vec<AutomationPoint>) {
    points.retain(|point| point.value.is_finite());
    points.sort_by_key(|point| point.tick);
}

fn sanitize_automation_target_id(target_parameter_id: String, track_id: Uuid) -> String {
    let trimmed = target_parameter_id.trim();
    if trimmed.is_empty() {
        format!("track:{track_id}:gain_db")
    } else {
        trimmed.to_string()
    }
}

fn normalize_pattern_clip(pattern: &mut PatternClip, ppq: u16) -> Result<(), EngineError> {
    if pattern.lines_per_beat == 0 {
        return Err(EngineError::InvalidTrackerLinesPerBeat(
            pattern.lines_per_beat,
        ));
    }

    if pattern.lines_per_beat > 64 {
        pattern.lines_per_beat = 64;
    }

    if pattern.rows.is_empty() && !pattern.notes.is_empty() {
        pattern.rows = tracker_rows_from_notes(&pattern.notes, pattern.lines_per_beat, ppq);
    } else {
        for row in &mut pattern.rows {
            sanitize_tracker_row(row);
        }
    }

    for lane in &mut pattern.macros {
        sanitize_chip_macro_lane(lane);
    }

    pattern.rows.sort_by_key(|row| row.row);
    pattern.notes = tracker_rows_to_notes(&pattern.rows, pattern.lines_per_beat, ppq)?;
    Ok(())
}

fn sync_pattern_rows_from_notes(pattern: &mut PatternClip, ppq: u16) -> Result<(), EngineError> {
    if pattern.lines_per_beat == 0 {
        return Err(EngineError::InvalidTrackerLinesPerBeat(
            pattern.lines_per_beat,
        ));
    }
    pattern.rows = tracker_rows_from_notes(&pattern.notes, pattern.lines_per_beat, ppq);
    Ok(())
}

fn tracker_rows_from_notes(notes: &[MidiNote], lines_per_beat: u16, ppq: u16) -> Vec<TrackerRow> {
    let ticks_per_row = tracker_rows_to_ticks(1, lines_per_beat, ppq).max(1);
    let mut rows = Vec::with_capacity(notes.len());
    for note in notes {
        rows.push(TrackerRow {
            row: (note.start_tick / ticks_per_row) as u32,
            note: Some(note.pitch.min(127)),
            velocity: note.velocity.min(127),
            gate: true,
            effect: None,
            effect_value: None,
        });
    }
    rows.sort_by_key(|row| row.row);
    rows
}

fn tracker_rows_to_notes(
    rows: &[TrackerRow],
    lines_per_beat: u16,
    ppq: u16,
) -> Result<Vec<MidiNote>, EngineError> {
    if lines_per_beat == 0 {
        return Err(EngineError::InvalidTrackerLinesPerBeat(lines_per_beat));
    }

    let row_length_ticks = tracker_rows_to_ticks(1, lines_per_beat, ppq).max(1);
    let mut notes = Vec::new();
    for row in rows {
        if !row.gate {
            continue;
        }
        let Some(note) = row.note else {
            continue;
        };

        notes.push(MidiNote {
            pitch: note.min(127),
            velocity: row.velocity.min(127),
            start_tick: tracker_rows_to_ticks(row.row, lines_per_beat, ppq),
            length_ticks: row_length_ticks,
            channel: 0,
        });
    }
    notes.sort_by_key(|note| note.start_tick);
    Ok(notes)
}

fn sanitize_audio_clip(audio: &mut AudioClip) -> Result<(), EngineError> {
    audio.gain_db = audio.gain_db.clamp(-96.0, 12.0);
    audio.pan = audio.pan.clamp(-1.0, 1.0);
    audio.source_duration_seconds = audio.source_duration_seconds.max(0.0);
    audio.trim_start_seconds = audio.trim_start_seconds.max(0.0);
    audio.trim_end_seconds = audio.trim_end_seconds.max(0.0);
    audio.fade_in_seconds = audio.fade_in_seconds.max(0.0);
    audio.fade_out_seconds = audio.fade_out_seconds.max(0.0);

    if audio.stretch_ratio <= 0.0 {
        return Err(EngineError::InvalidAudioStretchRatio(audio.stretch_ratio));
    }
    audio.stretch_ratio = audio.stretch_ratio.max(0.01);

    if audio.trim_end_seconds < audio.trim_start_seconds {
        return Err(EngineError::InvalidAudioTrimRange {
            start_seconds: audio.trim_start_seconds,
            end_seconds: audio.trim_end_seconds,
        });
    }

    if audio.trim_start_seconds > audio.source_duration_seconds {
        audio.trim_start_seconds = audio.source_duration_seconds;
    }
    if audio.trim_end_seconds > audio.source_duration_seconds {
        audio.trim_end_seconds = audio.source_duration_seconds;
    }

    if audio.fade_in_seconds + audio.fade_out_seconds > audio.effective_duration_seconds() {
        let available = audio.effective_duration_seconds();
        if available > 0.0 {
            let scale = available / (audio.fade_in_seconds + audio.fade_out_seconds);
            audio.fade_in_seconds *= scale;
            audio.fade_out_seconds *= scale;
        } else {
            audio.fade_in_seconds = 0.0;
            audio.fade_out_seconds = 0.0;
        }
    }

    Ok(())
}

fn validate_bus_target(
    tracks: &[Track],
    track_id: Uuid,
    target_bus: Uuid,
) -> Result<(), EngineError> {
    if track_id == target_bus {
        return Err(EngineError::InvalidBusTarget {
            track_id,
            target_bus,
        });
    }

    let bus = tracks.iter().find(|track| track.id == target_bus).ok_or(
        EngineError::InvalidBusTarget {
            track_id,
            target_bus,
        },
    )?;
    if !matches!(bus.kind, TrackKind::Bus) {
        return Err(EngineError::InvalidBusTarget {
            track_id,
            target_bus,
        });
    }
    Ok(())
}

fn validate_routing_graph(tracks: &[Track]) -> Result<(), EngineError> {
    for track in tracks {
        if let Some(output_bus) = track.output_bus {
            validate_bus_target(tracks, track.id, output_bus)?;
        }
        for send in &track.sends {
            if send.enabled {
                validate_bus_target(tracks, track.id, send.target_bus).map_err(|_| {
                    EngineError::InvalidTrackSend {
                        track_id: track.id,
                        target_bus: send.target_bus,
                    }
                })?;
            }
        }
    }

    let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for track in tracks {
        let edges = adjacency.entry(track.id).or_default();
        if let Some(output_bus) = track.output_bus {
            edges.push(output_bus);
        }
        for send in &track.sends {
            if send.enabled {
                edges.push(send.target_bus);
            }
        }
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for track in tracks {
        if detect_cycle(track.id, &adjacency, &mut visiting, &mut visited) {
            return Err(EngineError::RoutingCycleDetected);
        }
    }
    Ok(())
}

fn detect_cycle(
    node: Uuid,
    adjacency: &HashMap<Uuid, Vec<Uuid>>,
    visiting: &mut HashSet<Uuid>,
    visited: &mut HashSet<Uuid>,
) -> bool {
    if visited.contains(&node) {
        return false;
    }
    if !visiting.insert(node) {
        return true;
    }

    if let Some(neighbors) = adjacency.get(&node) {
        for neighbor in neighbors {
            if detect_cycle(*neighbor, adjacency, visiting, visited) {
                return true;
            }
        }
    }

    visiting.remove(&node);
    visited.insert(node);
    false
}

fn populate_builtin_effect_defaults(effect: &mut EffectSpec) {
    if !effect.params.is_empty() {
        return;
    }

    let mut params = BTreeMap::new();
    match effect.name.trim().to_ascii_lowercase().as_str() {
        "eq" => {
            params.insert("low_gain_db".to_string(), 0.0);
            params.insert("mid_gain_db".to_string(), 0.0);
            params.insert("high_gain_db".to_string(), 0.0);
            params.insert("low_freq_hz".to_string(), 120.0);
            params.insert("high_freq_hz".to_string(), 8_000.0);
        }
        "comp" | "compressor" => {
            params.insert("threshold_db".to_string(), -18.0);
            params.insert("ratio".to_string(), 4.0);
            params.insert("attack_ms".to_string(), 10.0);
            params.insert("release_ms".to_string(), 120.0);
            params.insert("makeup_db".to_string(), 0.0);
        }
        "reverb" => {
            params.insert("mix".to_string(), 0.18);
            params.insert("room_size".to_string(), 0.62);
            params.insert("damping".to_string(), 0.45);
            params.insert("width".to_string(), 0.85);
        }
        "delay" => {
            params.insert("mix".to_string(), 0.25);
            params.insert("time_ms".to_string(), 320.0);
            params.insert("feedback".to_string(), 0.38);
            params.insert("hi_cut_hz".to_string(), 6_500.0);
        }
        "limiter" => {
            params.insert("ceiling_db".to_string(), -0.8);
            params.insert("release_ms".to_string(), 80.0);
        }
        "bitcrusher" => {
            params.insert("bits".to_string(), 8.0);
            params.insert("downsample".to_string(), 2.0);
        }
        _ => {}
    }

    effect.params = params;
}

fn round_to_grid(value: u64, grid: u64) -> u64 {
    ((value.saturating_add(grid / 2)) / grid) * grid
}
