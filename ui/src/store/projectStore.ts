import { create } from "zustand";

import {
  addClipNote,
  addEffect,
  addMidiClip,
  addTrack,
  autosaveProject,
  createProject,
  exportProject,
  getProject,
  loadProject,
  measureParity,
  moveClip,
  patchTrackState,
  quantizeClipNotes,
  reorderTrack,
  removeClipNote,
  saveProject,
  setLoopRegion,
  setPlayback,
  transposeClipNotes,
  updateClipNotes
} from "../api/tauri";
import { logger } from "../lib/logger";
import type { ExportKind, MidiNote, ParityReport, Project, TrackKind } from "../types";

const TRACK_COLORS = [
  "#20d0ba",
  "#f89a17",
  "#11a8fd",
  "#f04f8f",
  "#90bf2f",
  "#ff6f4c"
];

interface ProjectStore {
  project: Project | null;
  parity: ParityReport | null;
  loading: boolean;
  error: string | null;
  outputRoot: string;
  selectedTrackId: string | null;
  selectedClipId: string | null;
  bootstrap: () => Promise<void>;
  createNewProject: (title: string) => Promise<void>;
  addTrackByKind: (kind: TrackKind) => Promise<void>;
  setTrackFlag: (
    trackId: string,
    patch: { hidden?: boolean; mute?: boolean; enabled?: boolean }
  ) => Promise<void>;
  addQuickClip: (trackId: string, kind: TrackKind) => Promise<void>;
  addBasicEffect: (trackId: string, effectName: string) => Promise<void>;
  shiftTrack: (index: number, direction: "up" | "down") => Promise<void>;
  setPlaybackState: (isPlaying: boolean) => Promise<void>;
  configureLoop: (enabled: boolean, loopStartTick?: number, loopEndTick?: number) => Promise<void>;
  moveClipTiming: (
    trackId: string,
    clipId: string,
    startTick: number,
    lengthTicks: number
  ) => Promise<void>;
  addNoteToClip: (trackId: string, clipId: string, note: MidiNote) => Promise<void>;
  removeNoteAt: (trackId: string, clipId: string, noteIndex: number) => Promise<void>;
  replaceClipNotes: (trackId: string, clipId: string, notes: MidiNote[]) => Promise<void>;
  transposeClip: (trackId: string, clipId: string, semitones: number) => Promise<void>;
  quantizeClip: (trackId: string, clipId: string, gridTicks: number) => Promise<void>;
  runExport: (kind: ExportKind) => Promise<void>;
  saveCurrentProject: () => Promise<void>;
  loadCurrentProject: () => Promise<void>;
  runAutosave: () => Promise<void>;
  refreshParity: () => Promise<void>;
  clearError: () => void;
  setSelectedTrack: (trackId: string | null) => void;
  setSelectedClip: (clipId: string | null) => void;
}

function nextTrackName(project: Project | null, kind: TrackKind): string {
  const base = kind.charAt(0).toUpperCase() + kind.slice(1);
  const count = project?.tracks.filter((track) => track.kind === kind).length ?? 0;
  return `${base} ${count + 1}`;
}

function nextTrackColor(project: Project | null): string {
  const index = project?.tracks.length ?? 0;
  return TRACK_COLORS[index % TRACK_COLORS.length];
}

function extensionFor(kind: ExportKind): string {
  switch (kind) {
    case "midi":
      return "mid";
    case "wav":
      return "wav";
    case "mp3":
      return "mp3";
  }
}

async function withErrorHandling(
  set: (partial: Partial<ProjectStore>) => void,
  task: () => Promise<void>
): Promise<void> {
  set({ loading: true, error: null });
  try {
    await task();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    logger.error("store action failed", { message, error });
    set({ error: message });
  } finally {
    set({ loading: false });
  }
}

export const useProjectStore = create<ProjectStore>((set, get) => ({
  project: null,
  parity: null,
  loading: false,
  error: null,
  outputRoot: "tmp/out",
  selectedTrackId: null,
  selectedClipId: null,

  bootstrap: async () => {
    await withErrorHandling(set, async () => {
      logger.info("loading initial project");
      const project = await getProject();
      set({
        project,
        selectedTrackId: project.tracks[0]?.id ?? null,
        selectedClipId: project.tracks[0]?.clips[0]?.id ?? null
      });
      await get().refreshParity();
    });
  },

  createNewProject: async (title) => {
    await withErrorHandling(set, async () => {
      const project = await createProject({ title, bpm: 140, sample_rate: 48_000 });
      set({
        project,
        selectedTrackId: project.tracks[0]?.id ?? null,
        selectedClipId: project.tracks[0]?.clips[0]?.id ?? null
      });
      await get().refreshParity();
    });
  },

  addTrackByKind: async (kind) => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      const updated = await addTrack({
        name: nextTrackName(project, kind),
        color: nextTrackColor(project),
        kind
      });
      set({ project: updated, selectedTrackId: updated.tracks[updated.tracks.length - 1]?.id ?? null });
      await get().refreshParity();
    });
  },

  setTrackFlag: async (trackId, patch) => {
    await withErrorHandling(set, async () => {
      const updated = await patchTrackState({ track_id: trackId, ...patch });
      set({ project: updated });
    });
  },

  addQuickClip: async (trackId, kind) => {
    await withErrorHandling(set, async () => {
      const isChip = kind === "chip";
      const updated = await addMidiClip({
        track_id: trackId,
        name: isChip ? "Chip Pattern" : "MIDI Clip",
        start_tick: 0,
        length_ticks: 1_920,
        source_chip: isChip ? "gameboy_apu" : undefined,
        instrument: isChip ? undefined : "Pulse Lead",
        notes: [
          { pitch: 60, velocity: 112, start_tick: 0, length_ticks: 480, channel: 0 },
          { pitch: 64, velocity: 112, start_tick: 480, length_ticks: 480, channel: 0 },
          { pitch: 67, velocity: 112, start_tick: 960, length_ticks: 480, channel: 0 },
          { pitch: 72, velocity: 112, start_tick: 1_440, length_ticks: 480, channel: 0 }
        ]
      });
      const targetTrack = updated.tracks.find((candidate) => candidate.id === trackId);
      const selectedClipId = targetTrack?.clips[targetTrack.clips.length - 1]?.id ?? null;
      set({ project: updated, selectedTrackId: trackId, selectedClipId });
      await get().refreshParity();
    });
  },

  addBasicEffect: async (trackId, effectName) => {
    await withErrorHandling(set, async () => {
      const updated = await addEffect({ track_id: trackId, effect_name: effectName });
      set({ project: updated });
    });
  },

  shiftTrack: async (index, direction) => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      if (!project) {
        return;
      }
      const target = direction === "up" ? index - 1 : index + 1;
      if (target < 0 || target >= project.tracks.length) {
        return;
      }

      const updated = await reorderTrack({ from: index, to: target });
      set({ project: updated });
    });
  },

  setPlaybackState: async (isPlaying) => {
    await withErrorHandling(set, async () => {
      const updated = await setPlayback(isPlaying);
      set({ project: updated });
    });
  },

  configureLoop: async (enabled, loopStartTick, loopEndTick) => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      if (!project) {
        return;
      }

      const start = loopStartTick ?? project.transport.loop_start_tick;
      const end = loopEndTick ?? project.transport.loop_end_tick;
      const updated = await setLoopRegion(start, end, enabled);
      set({ project: updated });
    });
  },

  moveClipTiming: async (trackId, clipId, startTick, lengthTicks) => {
    await withErrorHandling(set, async () => {
      const updated = await moveClip({
        track_id: trackId,
        clip_id: clipId,
        start_tick: startTick,
        length_ticks: lengthTicks
      });
      set({ project: updated });
    });
  },

  addNoteToClip: async (trackId, clipId, note) => {
    await withErrorHandling(set, async () => {
      const updated = await addClipNote({
        track_id: trackId,
        clip_id: clipId,
        note
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  removeNoteAt: async (trackId, clipId, noteIndex) => {
    await withErrorHandling(set, async () => {
      const updated = await removeClipNote({
        track_id: trackId,
        clip_id: clipId,
        note_index: noteIndex
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  replaceClipNotes: async (trackId, clipId, notes) => {
    await withErrorHandling(set, async () => {
      const updated = await updateClipNotes({
        track_id: trackId,
        clip_id: clipId,
        notes
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  transposeClip: async (trackId, clipId, semitones) => {
    await withErrorHandling(set, async () => {
      const updated = await transposeClipNotes({
        track_id: trackId,
        clip_id: clipId,
        semitones
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  quantizeClip: async (trackId, clipId, gridTicks) => {
    await withErrorHandling(set, async () => {
      const updated = await quantizeClipNotes({
        track_id: trackId,
        clip_id: clipId,
        grid_ticks: gridTicks
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  runExport: async (kind) => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      if (!project) {
        return;
      }

      const path = `${get().outputRoot}/${project.title.replace(/\s+/g, "_").toLowerCase()}.${extensionFor(
        kind
      )}`;
      await exportProject({ kind, output_path: path });
      logger.info("export completed", { kind, path });
    });
  },

  saveCurrentProject: async () => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      if (!project) {
        return;
      }
      const path = `${get().outputRoot}/${project.title.replace(/\s+/g, "_").toLowerCase()}.voltlane.json`;
      const updated = await saveProject(path);
      set({ project: updated });
    });
  },

  loadCurrentProject: async () => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      const title = project?.title.replace(/\s+/g, "_").toLowerCase() ?? "voltlane_mock";
      const path = `${get().outputRoot}/${title}.voltlane.json`;
      const updated = await loadProject(path);
      set({
        project: updated,
        selectedTrackId: updated.tracks[0]?.id ?? null,
        selectedClipId: updated.tracks[0]?.clips[0]?.id ?? null
      });
      await get().refreshParity();
    });
  },

  runAutosave: async () => {
    await withErrorHandling(set, async () => {
      await autosaveProject("tmp/autosave");
      logger.info("autosave completed");
    });
  },

  refreshParity: async () => {
    try {
      const parity = await measureParity();
      set({ parity });
    } catch (error) {
      logger.warn("parity measurement failed", error);
    }
  },

  clearError: () => set({ error: null }),
  setSelectedTrack: (trackId) =>
    set((state) => {
      const targetTrack = state.project?.tracks.find((track) => track.id === trackId) ?? null;
      return {
        selectedTrackId: trackId,
        selectedClipId: targetTrack?.clips[0]?.id ?? null
      };
    }),
  setSelectedClip: (clipId) => set({ selectedClipId: clipId })
}));
