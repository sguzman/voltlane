use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::{
    export,
    model::{Clip, ClipPayload, DEFAULT_SAMPLE_RATE, EffectSpec, Project, Track, TrackKind},
    persistence,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("track not found: {0}")]
    TrackNotFound(Uuid),
    #[error("clip not found: {0}")]
    ClipNotFound(Uuid),
    #[error("invalid reorder from {from} to {to}")]
    InvalidReorder { from: usize, to: usize },
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
}
