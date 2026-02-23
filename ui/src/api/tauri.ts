import { invoke } from "@tauri-apps/api/core";

import { logger } from "../lib/logger";
import type {
  AddClipNoteInput,
  AddAutomationClipInput,
  AnalyzeAudioAssetInput,
  AddMidiClipInput,
  AddTrackRequest,
  AutosaveStatus,
  AudioAnalysis,
  AudioAssetEntry,
  Clip,
  ExportProjectInput,
  ImportAudioClipInput,
  MoveClipInput,
  RemoveTrackSendInput,
  QuantizeClipNotesInput,
  ParityReport,
  PatchTrackMixInput,
  PatchTrackInput,
  Project,
  RemoveClipNoteInput,
  ScanAudioAssetsInput,
  TrackerRow,
  Track,
  TrackKind,
  TransposeClipNotesInput,
  UpdateAudioClipInput,
  UpdateAutomationClipInput,
  UpdatePatternMacrosInput,
  UpdatePatternRowsInput,
  UpsertTrackSendInput,
  UpdateClipNotesInput
} from "../types";

interface CreateProjectInput {
  title: string;
  bpm?: number;
  sample_rate?: number;
}

interface AddEffectInput {
  track_id: string;
  effect_name: string;
}

interface ReorderTrackInput {
  from: number;
  to: number;
}

const isTauriRuntime = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

function nowIso(): string {
  return new Date().toISOString();
}

function createTrack(name: string, color: string, kind: TrackKind): Track {
  return {
    id: crypto.randomUUID(),
    name,
    color,
    kind,
    hidden: false,
    mute: false,
    solo: false,
    enabled: true,
    gain_db: 0,
    pan: 0,
    output_bus: null,
    sends: [],
    effects: [],
    clips: []
  };
}

function createMockProject(title = "Voltlane Mock", bpm = 140): Project {
  const timestamp = nowIso();
  const firstTrack = createTrack("Lead", "#24d8be", "midi");

  return {
    id: crypto.randomUUID(),
    session_id: crypto.randomUUID(),
    title,
    bpm,
    ppq: 480,
    sample_rate: 48_000,
    transport: {
      playhead_tick: 0,
      loop_enabled: false,
      loop_start_tick: 0,
      loop_end_tick: 1_920,
      metronome_enabled: true,
      is_playing: false
    },
    tracks: [firstTrack],
    created_at: timestamp,
    updated_at: timestamp
  };
}

let mockProject: Project = createMockProject();

function normalizeProjectShape(project: Project): Project {
  for (const track of project.tracks) {
    if (typeof track.gain_db !== "number") track.gain_db = 0;
    if (typeof track.pan !== "number") track.pan = 0;
    if (typeof track.output_bus !== "string") track.output_bus = null;
    if (!Array.isArray(track.sends)) track.sends = [];
  }
  return project;
}

function touchProject(): void {
  mockProject.updated_at = nowIso();
}

function tinyHash(value: string): string {
  let hash = 5381;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 33) ^ value.charCodeAt(i);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function synthParity(project: Project): ParityReport {
  const projectJson = JSON.stringify(project);
  const midiJson = JSON.stringify(project.tracks.map((track) => track.clips));
  const audioJson = JSON.stringify(
    project.tracks.flatMap((track) =>
      track.clips.flatMap((clip) => {
        if ("midi" in clip.payload) {
          return clip.payload.midi.notes.map((note) => note.pitch + note.velocity + note.length_ticks);
        }
        if ("pattern" in clip.payload) {
          return clip.payload.pattern.notes.map((note) => note.pitch + note.velocity + note.length_ticks);
        }
        return [0];
      })
    )
  );

  return {
    schema_version: 1,
    project_id: project.id,
    track_count: project.tracks.length,
    clip_count: project.tracks.reduce((sum, track) => sum + track.clips.length, 0),
    note_count: project.tracks.reduce((sum, track) => {
      return (
        sum +
        track.clips.reduce((clipTotal, clip) => {
          if ("midi" in clip.payload) {
            return clipTotal + clip.payload.midi.notes.length;
          }
          if ("pattern" in clip.payload) {
            return clipTotal + clip.payload.pattern.notes.length;
          }
          return clipTotal;
        }, 0)
      );
    }, 0),
    project_hash: tinyHash(projectJson),
    midi_hash: tinyHash(midiJson),
    audio_hash: tinyHash(audioJson)
  };
}

function synthAutomationParameterIds(project: Project): string[] {
  const ids: string[] = [];
  for (const track of project.tracks) {
    ids.push(`track:${track.id}:gain_db`);
    ids.push(`track:${track.id}:pan`);
    for (const effect of track.effects) {
      for (const key of Object.keys(effect.params)) {
        ids.push(`track:${track.id}:effect:${effect.id}:${key}`);
      }
    }
  }
  return Array.from(new Set(ids)).sort();
}

function getClipRefs(project: Project, trackId: string, clipId: string): { track: Track; clip: Clip } {
  const track = project.tracks.find((candidate) => candidate.id === trackId);
  if (!track) {
    throw new Error(`track not found: ${trackId}`);
  }

  const clip = track.clips.find((candidate) => candidate.id === clipId);
  if (!clip) {
    throw new Error(`clip not found: ${clipId}`);
  }

  return { track, clip };
}

function isNoteClip(clip: Clip): clip is Clip & ({ payload: { midi: { notes: UpdateClipNotesInput["notes"] } } } | { payload: { pattern: { notes: UpdateClipNotesInput["notes"] } } }) {
  return "midi" in clip.payload || "pattern" in clip.payload;
}

function noteListFromClip(clip: Clip): UpdateClipNotesInput["notes"] {
  if ("midi" in clip.payload) {
    return clip.payload.midi.notes;
  }
  if ("pattern" in clip.payload) {
    return clip.payload.pattern.notes;
  }
  throw new Error(`clip payload is not midi/pattern: ${clip.id}`);
}

function clampNote(value: UpdateClipNotesInput["notes"][number]): UpdateClipNotesInput["notes"][number] {
  return {
    pitch: Math.max(0, Math.min(127, Math.round(value.pitch))),
    velocity: Math.max(0, Math.min(127, Math.round(value.velocity))),
    start_tick: Math.max(0, Math.round(value.start_tick)),
    length_ticks: Math.max(1, Math.round(value.length_ticks)),
    channel: Math.max(0, Math.min(15, Math.round(value.channel)))
  };
}

function notesToTrackerRows(notes: UpdateClipNotesInput["notes"], linesPerBeat = 4, ppq = 480): TrackerRow[] {
  const ticksPerRow = Math.max(1, Math.round(ppq / linesPerBeat));
  return notes
    .map((note) => ({
      row: Math.max(0, Math.round(note.start_tick / ticksPerRow)),
      note: Math.max(0, Math.min(127, Math.round(note.pitch))),
      velocity: Math.max(0, Math.min(127, Math.round(note.velocity))),
      gate: true,
      effect: null,
      effect_value: null
    }))
    .sort((left, right) => left.row - right.row);
}

function trackerRowsToNotes(rows: TrackerRow[], linesPerBeat = 4, ppq = 480): UpdateClipNotesInput["notes"] {
  const ticksPerRow = Math.max(1, Math.round(ppq / linesPerBeat));
  return rows
    .filter((row) => row.gate && typeof row.note === "number")
    .map((row) => ({
      pitch: Math.max(0, Math.min(127, Math.round(row.note ?? 60))),
      velocity: Math.max(0, Math.min(127, Math.round(row.velocity))),
      start_tick: Math.max(0, Math.round(row.row)) * ticksPerRow,
      length_ticks: ticksPerRow,
      channel: 0
    }))
    .sort((left, right) => left.start_tick - right.start_tick);
}

function syncPatternRowsFromNotes(clip: Clip): void {
  if (!("pattern" in clip.payload)) {
    return;
  }
  clip.payload.pattern.rows = notesToTrackerRows(
    clip.payload.pattern.notes,
    clip.payload.pattern.lines_per_beat
  );
}

function mockWaveformPeaks(bucketSize: number, count = 128): number[] {
  const density = Math.max(1, bucketSize);
  return Array.from({ length: count }, (_, index) => {
    const phase = (index / count) * Math.PI * 4 + density / 1_000;
    return Number((0.15 + Math.abs(Math.sin(phase)) * 0.75).toFixed(4));
  });
}

function mockAudioAnalysisFromPath(path: string, bucketSize: number): AudioAnalysis {
  const sampleRate = 48_000;
  const channels = 2;
  const durationSeconds = 6.0;
  const totalFrames = Math.round(sampleRate * durationSeconds);
  return {
    source_path: path,
    sample_rate: sampleRate,
    channels,
    total_frames: totalFrames,
    duration_seconds: durationSeconds,
    peaks: {
      bucket_size: bucketSize,
      peaks: mockWaveformPeaks(bucketSize)
    },
    cache_path: `localStorage://waveform-cache/${encodeURIComponent(path)}.json`
  };
}

async function invokeMock<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  logger.debug(`mock invoke ${command}`, args);

  switch (command) {
    case "get_project":
      return mockProject as T;

    case "create_project": {
      const input = args?.input as CreateProjectInput;
      mockProject = createMockProject(input.title, input.bpm ?? 140);
      if (input.sample_rate) {
        mockProject.sample_rate = input.sample_rate;
      }
      return mockProject as T;
    }

    case "add_track": {
      const request = args?.request as AddTrackRequest;
      mockProject.tracks.push(createTrack(request.name, request.color, request.kind));
      touchProject();
      return mockProject as T;
    }

    case "patch_track_state": {
      const input = args?.input as PatchTrackInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }
      if (typeof input.hidden === "boolean") track.hidden = input.hidden;
      if (typeof input.mute === "boolean") track.mute = input.mute;
      if (typeof input.solo === "boolean") track.solo = input.solo;
      if (typeof input.enabled === "boolean") track.enabled = input.enabled;
      touchProject();
      return mockProject as T;
    }

    case "patch_track_mix": {
      const input = args?.input as PatchTrackMixInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      if (typeof input.gain_db === "number") {
        track.gain_db = Math.max(-96, Math.min(12, input.gain_db));
      }
      if (typeof input.pan === "number") {
        track.pan = Math.max(-1, Math.min(1, input.pan));
      }

      if (input.clear_output_bus) {
        track.output_bus = null;
      } else if (typeof input.output_bus_id === "string") {
        const targetBus = mockProject.tracks.find(
          (candidate) => candidate.id === input.output_bus_id && candidate.kind === "bus"
        );
        if (!targetBus) {
          throw new Error(`invalid bus target: ${input.output_bus_id}`);
        }
        if (targetBus.id === track.id) {
          throw new Error(`track cannot route to itself: ${track.id}`);
        }
        track.output_bus = targetBus.id;
      }

      touchProject();
      return mockProject as T;
    }

    case "reorder_track": {
      const { from, to } = args?.input as ReorderTrackInput;
      const moved = mockProject.tracks.splice(from, 1)[0];
      if (moved) {
        mockProject.tracks.splice(to, 0, moved);
        touchProject();
      }
      return mockProject as T;
    }

    case "add_midi_clip": {
      const input = args?.input as AddMidiClipInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      const clip: Clip = {
        id: crypto.randomUUID(),
        name: input.name,
        start_tick: input.start_tick,
        length_ticks: input.length_ticks,
        disabled: false,
        payload: input.source_chip
          ? {
              pattern: {
                source_chip: input.source_chip,
                notes: input.notes,
                rows: notesToTrackerRows(input.notes),
                macros: [],
                lines_per_beat: 4
              }
            }
          : { midi: { instrument: input.instrument ?? null, notes: input.notes } }
      };

      track.clips.push(clip);
      touchProject();
      return mockProject as T;
    }

    case "add_automation_clip": {
      const input = args?.input as AddAutomationClipInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      track.clips.push({
        id: crypto.randomUUID(),
        name: input.name,
        start_tick: input.start_tick,
        length_ticks: Math.max(1, Math.round(input.length_ticks)),
        disabled: false,
        payload: {
          automation: {
            target_parameter_id:
              input.target_parameter_id?.trim() || `track:${track.id}:gain_db`,
            points: [...input.points]
              .filter((point) => Number.isFinite(point.value))
              .sort((left, right) => left.tick - right.tick)
          }
        }
      });
      touchProject();
      return mockProject as T;
    }

    case "get_automation_parameter_ids":
      return synthAutomationParameterIds(mockProject) as T;

    case "scan_audio_assets": {
      const input = args?.input as ScanAudioAssetsInput | undefined;
      const directory = input?.directory ?? "data/audio-library";
      const assets: AudioAssetEntry[] = [
        {
          path: `${directory}/drums/kick.wav`,
          extension: "wav",
          size_bytes: 62_400
        },
        {
          path: `${directory}/loops/breakbeat.wav`,
          extension: "wav",
          size_bytes: 480_128
        },
        {
          path: `${directory}/fx/laser.ogg`,
          extension: "ogg",
          size_bytes: 43_912
        }
      ];
      return assets as T;
    }

    case "analyze_audio_asset": {
      const input = args?.input as AnalyzeAudioAssetInput;
      const bucketSize = Math.max(1, Math.round(input.bucket_size ?? 1024));
      return mockAudioAnalysisFromPath(input.path, bucketSize) as T;
    }

    case "import_audio_clip": {
      const input = args?.input as ImportAudioClipInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }
      if (track.kind !== "audio") {
        throw new Error(`track is not audio: ${input.track_id}`);
      }

      const analysis = mockAudioAnalysisFromPath(input.source_path, input.bucket_size ?? 1024);
      const clipLengthTicks = Math.max(
        1,
        Math.round((analysis.duration_seconds * mockProject.bpm * mockProject.ppq) / 60)
      );

      track.clips.push({
        id: crypto.randomUUID(),
        name: input.name ?? input.source_path.split("/").pop() ?? "Audio Clip",
        start_tick: input.start_tick,
        length_ticks: clipLengthTicks,
        disabled: false,
        payload: {
          audio: {
            source_path: input.source_path,
            gain_db: 0,
            pan: 0,
            source_sample_rate: analysis.sample_rate,
            source_channels: analysis.channels,
            source_duration_seconds: analysis.duration_seconds,
            trim_start_seconds: 0,
            trim_end_seconds: analysis.duration_seconds,
            fade_in_seconds: 0,
            fade_out_seconds: 0,
            reverse: false,
            stretch_ratio: 1,
            waveform_bucket_size: analysis.peaks.bucket_size,
            waveform_peaks: analysis.peaks.peaks,
            waveform_cache_path: analysis.cache_path
          }
        }
      });
      touchProject();
      return mockProject as T;
    }

    case "update_audio_clip": {
      const input = args?.input as UpdateAudioClipInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!("audio" in clip.payload)) {
        throw new Error(`clip is not audio: ${input.clip_id}`);
      }

      const audio = clip.payload.audio;
      if (typeof input.gain_db === "number") audio.gain_db = input.gain_db;
      if (typeof input.pan === "number") audio.pan = input.pan;
      if (typeof input.trim_start_seconds === "number") {
        audio.trim_start_seconds = Math.max(0, input.trim_start_seconds);
      }
      if (typeof input.trim_end_seconds === "number") {
        audio.trim_end_seconds = Math.max(audio.trim_start_seconds, input.trim_end_seconds);
      }
      if (typeof input.fade_in_seconds === "number") audio.fade_in_seconds = Math.max(0, input.fade_in_seconds);
      if (typeof input.fade_out_seconds === "number") audio.fade_out_seconds = Math.max(0, input.fade_out_seconds);
      if (typeof input.reverse === "boolean") audio.reverse = input.reverse;
      if (typeof input.stretch_ratio === "number") {
        audio.stretch_ratio = Math.max(0.01, input.stretch_ratio);
      }

      const trimmedDuration = Math.max(0, audio.trim_end_seconds - audio.trim_start_seconds);
      const effectiveDuration = trimmedDuration * Math.max(0.01, audio.stretch_ratio);
      clip.length_ticks = Math.max(
        1,
        Math.round((effectiveDuration * mockProject.bpm * mockProject.ppq) / 60)
      );
      touchProject();
      return mockProject as T;
    }

    case "move_clip": {
      const input = args?.input as MoveClipInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);

      clip.start_tick = input.start_tick;
      clip.length_ticks = input.length_ticks;
      touchProject();
      return mockProject as T;
    }

    case "update_clip_notes": {
      const input = args?.input as UpdateClipNotesInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!isNoteClip(clip)) {
        throw new Error(`clip payload is not midi/pattern: ${input.clip_id}`);
      }

      const notes = input.notes.map(clampNote);
      if ("midi" in clip.payload) {
        clip.payload.midi.notes = notes;
      } else {
        clip.payload.pattern.notes = notes;
        syncPatternRowsFromNotes(clip);
      }

      touchProject();
      return mockProject as T;
    }

    case "update_automation_clip": {
      const input = args?.input as UpdateAutomationClipInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!("automation" in clip.payload)) {
        throw new Error(`clip payload is not automation: ${input.clip_id}`);
      }
      if (typeof input.target_parameter_id === "string") {
        clip.payload.automation.target_parameter_id =
          input.target_parameter_id.trim() || clip.payload.automation.target_parameter_id;
      }
      clip.payload.automation.points = [...input.points]
        .filter((point) => Number.isFinite(point.value))
        .sort((left, right) => left.tick - right.tick);
      touchProject();
      return mockProject as T;
    }

    case "update_pattern_rows": {
      const input = args?.input as UpdatePatternRowsInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!("pattern" in clip.payload)) {
        throw new Error(`clip payload is not pattern: ${input.clip_id}`);
      }
      const linesPerBeat = Math.max(1, Math.round(input.lines_per_beat ?? clip.payload.pattern.lines_per_beat));
      clip.payload.pattern.lines_per_beat = linesPerBeat;
      clip.payload.pattern.rows = input.rows
        .map((row) => ({
          row: Math.max(0, Math.round(row.row)),
          note: typeof row.note === "number" ? Math.max(0, Math.min(127, Math.round(row.note))) : null,
          velocity: Math.max(0, Math.min(127, Math.round(row.velocity))),
          gate: Boolean(row.gate),
          effect: row.effect?.trim() ? row.effect : null,
          effect_value:
            typeof row.effect_value === "number"
              ? Math.max(0, Math.min(65535, Math.round(row.effect_value)))
              : null
        }))
        .sort((left, right) => left.row - right.row);
      clip.payload.pattern.notes = trackerRowsToNotes(clip.payload.pattern.rows, linesPerBeat, mockProject.ppq);
      touchProject();
      return mockProject as T;
    }

    case "update_pattern_macros": {
      const input = args?.input as UpdatePatternMacrosInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!("pattern" in clip.payload)) {
        throw new Error(`clip payload is not pattern: ${input.clip_id}`);
      }

      clip.payload.pattern.macros = input.macros.map((lane) => ({
        target: lane.target.trim().toLowerCase(),
        enabled: Boolean(lane.enabled),
        values: lane.values
          .slice(0, 256)
          .map((value) => Math.max(-127, Math.min(127, Math.round(value)))),
        loop_start: typeof lane.loop_start === "number" ? Math.max(0, Math.round(lane.loop_start)) : null,
        loop_end: typeof lane.loop_end === "number" ? Math.max(0, Math.round(lane.loop_end)) : null
      }));
      touchProject();
      return mockProject as T;
    }

    case "add_clip_note": {
      const input = args?.input as AddClipNoteInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!isNoteClip(clip)) {
        throw new Error(`clip payload is not midi/pattern: ${input.clip_id}`);
      }
      const note = clampNote(input.note);
      noteListFromClip(clip).push(note);
      noteListFromClip(clip).sort((left, right) => left.start_tick - right.start_tick);
      syncPatternRowsFromNotes(clip);
      touchProject();
      return mockProject as T;
    }

    case "remove_clip_note": {
      const input = args?.input as RemoveClipNoteInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!isNoteClip(clip)) {
        throw new Error(`clip payload is not midi/pattern: ${input.clip_id}`);
      }
      const notes = noteListFromClip(clip);
      if (input.note_index < 0 || input.note_index >= notes.length) {
        throw new Error(`invalid note index: ${input.note_index}`);
      }
      notes.splice(input.note_index, 1);
      syncPatternRowsFromNotes(clip);
      touchProject();
      return mockProject as T;
    }

    case "transpose_clip_notes": {
      const input = args?.input as TransposeClipNotesInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!isNoteClip(clip)) {
        throw new Error(`clip payload is not midi/pattern: ${input.clip_id}`);
      }
      const notes = noteListFromClip(clip);
      for (const note of notes) {
        note.pitch = Math.max(0, Math.min(127, Math.round(note.pitch + input.semitones)));
      }
      syncPatternRowsFromNotes(clip);
      touchProject();
      return mockProject as T;
    }

    case "quantize_clip_notes": {
      const input = args?.input as QuantizeClipNotesInput;
      const { clip } = getClipRefs(mockProject, input.track_id, input.clip_id);
      if (!isNoteClip(clip)) {
        throw new Error(`clip payload is not midi/pattern: ${input.clip_id}`);
      }
      if (input.grid_ticks <= 0) {
        throw new Error(`invalid grid_ticks: ${input.grid_ticks}`);
      }
      const grid = Math.round(input.grid_ticks);
      const notes = noteListFromClip(clip);
      for (const note of notes) {
        note.start_tick = Math.max(0, Math.round(note.start_tick / grid) * grid);
        note.length_ticks = Math.max(grid, Math.round(note.length_ticks / grid) * grid);
      }
      notes.sort((left, right) => left.start_tick - right.start_tick);
      syncPatternRowsFromNotes(clip);
      touchProject();
      return mockProject as T;
    }

    case "add_effect": {
      const input = args?.input as AddEffectInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      const normalized = input.effect_name.trim().toLowerCase();
      let params: Record<string, number> = {};
      if (normalized === "eq") {
        params = {
          low_gain_db: 0,
          mid_gain_db: 0,
          high_gain_db: 0,
          low_freq_hz: 120,
          high_freq_hz: 8000
        };
      } else if (normalized === "comp" || normalized === "compressor") {
        params = {
          threshold_db: -18,
          ratio: 4,
          attack_ms: 10,
          release_ms: 120,
          makeup_db: 0
        };
      } else if (normalized === "reverb") {
        params = { mix: 0.18, room_size: 0.62, damping: 0.45, width: 0.85 };
      } else if (normalized === "delay") {
        params = { mix: 0.25, time_ms: 320, feedback: 0.38, hi_cut_hz: 6500 };
      } else if (normalized === "limiter") {
        params = { ceiling_db: -0.8, release_ms: 80 };
      } else if (normalized === "bitcrusher") {
        params = { bits: 8, downsample: 2 };
      }

      track.effects.push({
        id: crypto.randomUUID(),
        name: input.effect_name,
        enabled: true,
        params
      });
      touchProject();
      return mockProject as T;
    }

    case "upsert_track_send": {
      const input = args?.input as UpsertTrackSendInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      const targetBus = mockProject.tracks.find(
        (candidate) => candidate.id === input.send.target_bus_id && candidate.kind === "bus"
      );
      if (!targetBus) {
        throw new Error(`invalid bus target: ${input.send.target_bus_id}`);
      }
      const send = {
        id: input.send.id ?? crypto.randomUUID(),
        target_bus: input.send.target_bus_id,
        level_db: Math.max(-96, Math.min(12, input.send.level_db ?? 0)),
        pan: Math.max(-1, Math.min(1, input.send.pan ?? 0)),
        pre_fader: Boolean(input.send.pre_fader),
        enabled: input.send.enabled ?? true
      };
      const existing = track.sends.findIndex((candidate) => candidate.id === send.id);
      if (existing >= 0) {
        track.sends[existing] = send;
      } else {
        track.sends.push(send);
      }
      touchProject();
      return mockProject as T;
    }

    case "remove_track_send": {
      const input = args?.input as RemoveTrackSendInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }
      const before = track.sends.length;
      track.sends = track.sends.filter((send) => send.id !== input.send_id);
      if (track.sends.length === before) {
        throw new Error(`send not found: ${input.send_id}`);
      }
      touchProject();
      return mockProject as T;
    }

    case "set_playback": {
      const isPlaying = args?.isPlaying as boolean;
      mockProject.transport.is_playing = isPlaying;
      touchProject();
      return mockProject as T;
    }

    case "set_loop_region": {
      const loopStartTick = args?.loopStartTick as number;
      const loopEndTick = args?.loopEndTick as number;
      const loopEnabled = args?.loopEnabled as boolean;
      mockProject.transport.loop_start_tick = loopStartTick;
      mockProject.transport.loop_end_tick = loopEndTick;
      mockProject.transport.loop_enabled = loopEnabled;
      touchProject();
      return mockProject as T;
    }

    case "export_project": {
      const input = args?.input as ExportProjectInput;
      return input.output_path as T;
    }

    case "save_project": {
      localStorage.setItem("voltlane.mock.project", JSON.stringify(mockProject));
      return mockProject as T;
    }

    case "load_project": {
      const stored = localStorage.getItem("voltlane.mock.project");
      if (!stored) {
        throw new Error("no saved mock project found");
      }
      mockProject = normalizeProjectShape(JSON.parse(stored) as Project);
      return mockProject as T;
    }

    case "autosave_project": {
      localStorage.setItem("voltlane.mock.autosave", JSON.stringify(mockProject));
      return "localStorage://voltlane.mock.autosave" as T;
    }

    case "get_autosave_status": {
      const exists = localStorage.getItem("voltlane.mock.autosave") !== null;
      const status: AutosaveStatus = {
        exists,
        path: exists ? "localStorage://voltlane.mock.autosave" : null,
        modified_epoch_ms: exists ? Date.now() : null
      };
      return status as T;
    }

    case "measure_parity":
      return synthParity(mockProject) as T;

    default:
      throw new Error(`unsupported mock command: ${command}`);
  }
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauriRuntime()) {
    return invoke<T>(command, args);
  }

  return invokeMock<T>(command, args);
}

export async function getProject(): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("get_project"));
}

export async function createProject(input: CreateProjectInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("create_project", { input }));
}

export async function addTrack(request: AddTrackRequest): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("add_track", { request }));
}

export async function patchTrackState(input: PatchTrackInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("patch_track_state", { input }));
}

export async function patchTrackMix(input: PatchTrackMixInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("patch_track_mix", { input }));
}

export async function upsertTrackSend(input: UpsertTrackSendInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("upsert_track_send", { input }));
}

export async function removeTrackSend(input: RemoveTrackSendInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("remove_track_send", { input }));
}

export async function reorderTrack(input: ReorderTrackInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("reorder_track", { input }));
}

export async function addMidiClip(input: AddMidiClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("add_midi_clip", { input }));
}

export async function addAutomationClip(input: AddAutomationClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("add_automation_clip", { input }));
}

export async function getAutomationParameterIds(): Promise<string[]> {
  return invokeCommand<string[]>("get_automation_parameter_ids");
}

export async function scanAudioAssets(input?: ScanAudioAssetsInput): Promise<AudioAssetEntry[]> {
  return invokeCommand<AudioAssetEntry[]>("scan_audio_assets", { input });
}

export async function analyzeAudioAsset(input: AnalyzeAudioAssetInput): Promise<AudioAnalysis> {
  return invokeCommand<AudioAnalysis>("analyze_audio_asset", { input });
}

export async function importAudioClip(input: ImportAudioClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("import_audio_clip", { input }));
}

export async function updateAudioClip(input: UpdateAudioClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("update_audio_clip", { input }));
}

export async function moveClip(input: MoveClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("move_clip", { input }));
}

export async function updateClipNotes(input: UpdateClipNotesInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("update_clip_notes", { input }));
}

export async function updateAutomationClip(input: UpdateAutomationClipInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("update_automation_clip", { input }));
}

export async function updatePatternRows(input: UpdatePatternRowsInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("update_pattern_rows", { input }));
}

export async function updatePatternMacros(input: UpdatePatternMacrosInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("update_pattern_macros", { input }));
}

export async function addClipNote(input: AddClipNoteInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("add_clip_note", { input }));
}

export async function removeClipNote(input: RemoveClipNoteInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("remove_clip_note", { input }));
}

export async function transposeClipNotes(input: TransposeClipNotesInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("transpose_clip_notes", { input }));
}

export async function quantizeClipNotes(input: QuantizeClipNotesInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("quantize_clip_notes", { input }));
}

export async function addEffect(input: AddEffectInput): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("add_effect", { input }));
}

export async function setPlayback(isPlaying: boolean): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("set_playback", { isPlaying }));
}

export async function setLoopRegion(
  loopStartTick: number,
  loopEndTick: number,
  loopEnabled: boolean
): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("set_loop_region", {
    loopStartTick,
    loopEndTick,
    loopEnabled
  }));
}

export async function exportProject(input: ExportProjectInput): Promise<string> {
  return invokeCommand<string>("export_project", { input });
}

export async function saveProject(path: string): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("save_project", { path }));
}

export async function loadProject(path: string): Promise<Project> {
  return normalizeProjectShape(await invokeCommand<Project>("load_project", { path }));
}

export async function autosaveProject(autosaveDir: string): Promise<string> {
  return invokeCommand<string>("autosave_project", { autosaveDir });
}

export async function getAutosaveStatus(): Promise<AutosaveStatus> {
  return invokeCommand<AutosaveStatus>("get_autosave_status");
}

export async function measureParity(): Promise<ParityReport> {
  return invokeCommand<ParityReport>("measure_parity");
}
