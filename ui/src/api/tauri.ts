import { invoke } from "@tauri-apps/api/core";

import { logger } from "../lib/logger";
import type {
  AddMidiClipInput,
  AddTrackRequest,
  Clip,
  ExportProjectInput,
  ParityReport,
  PatchTrackInput,
  Project,
  Track,
  TrackKind
} from "../types";

interface CreateProjectInput {
  title: string;
  bpm?: number;
  sample_rate?: number;
}

interface MoveClipInput {
  track_id: string;
  clip_id: string;
  start_tick: number;
  length_ticks: number;
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
          ? { pattern: { source_chip: input.source_chip, notes: input.notes } }
          : { midi: { instrument: input.instrument ?? null, notes: input.notes } }
      };

      track.clips.push(clip);
      touchProject();
      return mockProject as T;
    }

    case "move_clip": {
      const input = args?.input as MoveClipInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      const clip = track?.clips.find((candidate) => candidate.id === input.clip_id);
      if (!clip) {
        throw new Error(`clip not found: ${input.clip_id}`);
      }

      clip.start_tick = input.start_tick;
      clip.length_ticks = input.length_ticks;
      touchProject();
      return mockProject as T;
    }

    case "add_effect": {
      const input = args?.input as AddEffectInput;
      const track = mockProject.tracks.find((candidate) => candidate.id === input.track_id);
      if (!track) {
        throw new Error(`track not found: ${input.track_id}`);
      }

      track.effects.push({
        id: crypto.randomUUID(),
        name: input.effect_name,
        enabled: true,
        params: {}
      });
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
      mockProject = JSON.parse(stored) as Project;
      return mockProject as T;
    }

    case "autosave_project": {
      localStorage.setItem("voltlane.mock.autosave", JSON.stringify(mockProject));
      return "localStorage://voltlane.mock.autosave" as T;
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
  return invokeCommand<Project>("get_project");
}

export async function createProject(input: CreateProjectInput): Promise<Project> {
  return invokeCommand<Project>("create_project", { input });
}

export async function addTrack(request: AddTrackRequest): Promise<Project> {
  return invokeCommand<Project>("add_track", { request });
}

export async function patchTrackState(input: PatchTrackInput): Promise<Project> {
  return invokeCommand<Project>("patch_track_state", { input });
}

export async function reorderTrack(input: ReorderTrackInput): Promise<Project> {
  return invokeCommand<Project>("reorder_track", { input });
}

export async function addMidiClip(input: AddMidiClipInput): Promise<Project> {
  return invokeCommand<Project>("add_midi_clip", { input });
}

export async function moveClip(input: MoveClipInput): Promise<Project> {
  return invokeCommand<Project>("move_clip", { input });
}

export async function addEffect(input: AddEffectInput): Promise<Project> {
  return invokeCommand<Project>("add_effect", { input });
}

export async function setPlayback(isPlaying: boolean): Promise<Project> {
  return invokeCommand<Project>("set_playback", { isPlaying });
}

export async function setLoopRegion(
  loopStartTick: number,
  loopEndTick: number,
  loopEnabled: boolean
): Promise<Project> {
  return invokeCommand<Project>("set_loop_region", {
    loopStartTick,
    loopEndTick,
    loopEnabled
  });
}

export async function exportProject(input: ExportProjectInput): Promise<string> {
  return invokeCommand<string>("export_project", { input });
}

export async function saveProject(path: string): Promise<Project> {
  return invokeCommand<Project>("save_project", { path });
}

export async function loadProject(path: string): Promise<Project> {
  return invokeCommand<Project>("load_project", { path });
}

export async function autosaveProject(autosaveDir: string): Promise<string> {
  return invokeCommand<string>("autosave_project", { autosaveDir });
}

export async function measureParity(): Promise<ParityReport> {
  return invokeCommand<ParityReport>("measure_parity");
}
