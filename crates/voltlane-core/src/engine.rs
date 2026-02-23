use std::path::{Path, PathBuf};

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
        AudioClip, Clip, ClipPayload, DEFAULT_SAMPLE_RATE, EffectSpec, MidiNote, Project, Track,
        TrackKind,
    },
    persistence,
    time::seconds_to_ticks,
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
    #[error("invalid quantize grid ticks: {0}")]
    InvalidQuantizeGrid(u64),
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Midi,
    Wav,
    Mp3,
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
        effect: EffectSpec,
    ) -> Result<EffectSpec, EngineError> {
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

    #[instrument(skip(self, request), fields(project_id = %self.project.id, track_id = %request.track_id, clip_name = %request.name))]
    pub fn add_clip(&mut self, request: AddClipRequest) -> Result<Clip, EngineError> {
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
            payload: request.payload,
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
        for note in &mut notes {
            sanitize_note(note);
        }

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let target =
                clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
            *target = notes;
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
        sanitize_note(&mut note);

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let notes =
                clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
            notes.push(note);
            notes.sort_by_key(|candidate| candidate.start_tick);
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
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let notes =
                clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
            if note_index >= notes.len() {
                return Err(EngineError::InvalidNoteIndex(note_index));
            }
            notes.remove(note_index);
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
        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let notes =
                clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;
            for note in notes {
                let pitch = i16::from(note.pitch)
                    .saturating_add(semitones)
                    .clamp(0, 127) as u8;
                note.pitch = pitch;
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
        if grid_ticks == 0 {
            return Err(EngineError::InvalidQuantizeGrid(grid_ticks));
        }

        let updated_clip = {
            let clip = self.find_clip_mut(track_id, clip_id)?;
            let notes =
                clip_note_vec_mut(clip).ok_or(EngineError::UnsupportedClipPayload(clip_id))?;

            for note in notes.iter_mut() {
                note.start_tick = round_to_grid(note.start_tick, grid_ticks);
                note.length_ticks =
                    round_to_grid(note.length_ticks.max(1), grid_ticks).max(grid_ticks);
                sanitize_note(note);
            }
            notes.sort_by_key(|candidate| candidate.start_tick);
            clip.clone()
        };

        self.project.touch();
        info!("clip notes quantized");
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
    ) -> Result<(), EngineError> {
        match kind {
            ExportKind::Midi => export::export_midi(&self.project, output_path)?,
            ExportKind::Wav => export::export_wav(&self.project, output_path)?,
            ExportKind::Mp3 => export::export_mp3(&self.project, output_path, ffmpeg_binary)?,
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

fn sanitize_note(note: &mut MidiNote) {
    note.pitch = note.pitch.min(127);
    note.velocity = note.velocity.min(127);
    note.channel = note.channel.min(15);
    note.length_ticks = note.length_ticks.max(1);
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

fn round_to_grid(value: u64, grid: u64) -> u64 {
    ((value.saturating_add(grid / 2)) / grid) * grid
}
