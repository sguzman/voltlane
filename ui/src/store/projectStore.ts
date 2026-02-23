import { create } from "zustand";

import {
  addAutomationClip,
  addClipNote,
  addEffect,
  addMidiClip,
  addTrack,
  analyzeAudioAsset,
  autosaveProject,
  createProject,
  exportProject,
  getAutomationParameterIds,
  getAutosaveStatus,
  getProject,
  importAudioClip,
  loadProject,
  measureParity,
  moveClip,
  patchTrackMix,
  patchTrackState,
  quantizeClipNotes,
  reorderTrack,
  removeClipNote,
  removeTrackSend,
  scanAudioAssets,
  saveProject,
  setLoopRegion,
  setPlayback,
  transposeClipNotes,
  updateAutomationClip,
  updateAudioClip,
  updatePatternMacros,
  updatePatternRows,
  upsertTrackSend,
  updateClipNotes
} from "../api/tauri";
import { logger } from "../lib/logger";
import type {
  AudioAnalysis,
  AudioAssetEntry,
  AutomationPoint,
  ChipMacroLane,
  ExportKind,
  RenderMode,
  MidiNote,
  ParityReport,
  TrackSendInput,
  PatchTrackMixInput,
  Project,
  TrackerRow,
  TrackKind
} from "../types";

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
  exportRenderMode: RenderMode;
  automationParameterIds: string[];
  autosaveRecoveryPath: string | null;
  autosaveRecoveryModifiedEpochMs: number | null;
  selectedTrackId: string | null;
  selectedClipId: string | null;
  audioAssets: AudioAssetEntry[];
  audioScanDirectory: string;
  selectedAudioAssetPath: string | null;
  audioPreview: AudioAnalysis | null;
  bootstrap: () => Promise<void>;
  createNewProject: (title: string) => Promise<void>;
  addTrackByKind: (kind: TrackKind) => Promise<void>;
  setTrackFlag: (
    trackId: string,
    patch: { hidden?: boolean; mute?: boolean; enabled?: boolean }
  ) => Promise<void>;
  setTrackMix: (
    trackId: string,
    patch: { gain_db?: number; pan?: number; output_bus_id?: string | null }
  ) => Promise<void>;
  addQuickClip: (trackId: string, kind: TrackKind) => Promise<void>;
  addAutomationLaneClip: (
    trackId: string,
    targetParameterId?: string
  ) => Promise<void>;
  addBasicEffect: (trackId: string, effectName: string) => Promise<void>;
  saveTrackSend: (trackId: string, send: TrackSendInput) => Promise<void>;
  deleteTrackSend: (trackId: string, sendId: string) => Promise<void>;
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
  replacePatternRows: (
    trackId: string,
    clipId: string,
    rows: TrackerRow[],
    linesPerBeat?: number
  ) => Promise<void>;
  replacePatternMacros: (
    trackId: string,
    clipId: string,
    macros: ChipMacroLane[]
  ) => Promise<void>;
  replaceAutomationClip: (
    trackId: string,
    clipId: string,
    targetParameterId: string | undefined,
    points: AutomationPoint[]
  ) => Promise<void>;
  transposeClip: (trackId: string, clipId: string, semitones: number) => Promise<void>;
  quantizeClip: (trackId: string, clipId: string, gridTicks: number) => Promise<void>;
  scanAudioLibrary: (directory?: string) => Promise<void>;
  previewAudioAsset: (assetPath: string) => Promise<void>;
  importAudioAsset: (assetPath: string, startTick?: number) => Promise<void>;
  patchAudioClipSettings: (
    trackId: string,
    clipId: string,
    patch: {
      gain_db?: number;
      pan?: number;
      trim_start_seconds?: number;
      trim_end_seconds?: number;
      fade_in_seconds?: number;
      fade_out_seconds?: number;
      reverse?: boolean;
      stretch_ratio?: number;
    }
  ) => Promise<void>;
  runExport: (kind: ExportKind) => Promise<void>;
  setExportRenderMode: (mode: RenderMode) => void;
  saveCurrentProject: () => Promise<void>;
  loadCurrentProject: () => Promise<void>;
  runAutosave: () => Promise<void>;
  reloadAutomationTargets: () => Promise<void>;
  restoreAutosave: () => Promise<void>;
  dismissAutosaveRecovery: () => void;
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
    case "stem_wav":
      return "wav";
  }
}

function exportPathFor(kind: ExportKind, outputRoot: string, projectTitle: string): string {
  const slug = projectTitle.replace(/\s+/g, "_").toLowerCase();
  if (kind === "stem_wav") {
    return `${outputRoot}/${slug}/stems`;
  }
  return `${outputRoot}/${slug}.${extensionFor(kind)}`;
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
  outputRoot: "data/exports",
  exportRenderMode: "offline",
  automationParameterIds: [],
  autosaveRecoveryPath: null,
  autosaveRecoveryModifiedEpochMs: null,
  selectedTrackId: null,
  selectedClipId: null,
  audioAssets: [],
  audioScanDirectory: "data/audio-library",
  selectedAudioAssetPath: null,
  audioPreview: null,

  bootstrap: async () => {
    await withErrorHandling(set, async () => {
      logger.info("loading initial project");
      const project = await getProject();
      const [automationParameterIds, autosaveStatus] = await Promise.all([
        getAutomationParameterIds(),
        getAutosaveStatus()
      ]);
      set({
        project,
        automationParameterIds,
        autosaveRecoveryPath: autosaveStatus.exists ? autosaveStatus.path : null,
        autosaveRecoveryModifiedEpochMs: autosaveStatus.exists
          ? autosaveStatus.modified_epoch_ms
          : null,
        selectedTrackId: project.tracks[0]?.id ?? null,
        selectedClipId: project.tracks[0]?.clips[0]?.id ?? null
      });
      await get().refreshParity();
      await get().scanAudioLibrary();
    });
  },

  createNewProject: async (title) => {
    await withErrorHandling(set, async () => {
      const project = await createProject({ title, bpm: 140, sample_rate: 48_000 });
      const automationParameterIds = await getAutomationParameterIds();
      set({
        project,
        automationParameterIds,
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
      const automationParameterIds = await getAutomationParameterIds();
      set({
        project: updated,
        automationParameterIds,
        selectedTrackId: updated.tracks[updated.tracks.length - 1]?.id ?? null
      });
      await get().refreshParity();
    });
  },

  setTrackFlag: async (trackId, patch) => {
    await withErrorHandling(set, async () => {
      const updated = await patchTrackState({ track_id: trackId, ...patch });
      set({ project: updated });
    });
  },

  setTrackMix: async (trackId, patch) => {
    await withErrorHandling(set, async () => {
      const input: PatchTrackMixInput = {
        track_id: trackId
      };
      if (typeof patch.gain_db === "number") {
        input.gain_db = patch.gain_db;
      }
      if (typeof patch.pan === "number") {
        input.pan = patch.pan;
      }
      if (patch.output_bus_id === null) {
        input.clear_output_bus = true;
      } else if (typeof patch.output_bus_id === "string") {
        input.output_bus_id = patch.output_bus_id;
      }

      const updated = await patchTrackMix(input);
      set({ project: updated });
    });
  },

  addQuickClip: async (trackId, kind) => {
    await withErrorHandling(set, async () => {
      if (kind === "audio") {
        throw new Error("Use Audio Browser to import audio clips into audio tracks.");
      }
      if (kind === "bus") {
        throw new Error("Bus tracks are routing destinations and do not host clips.");
      }
      if (kind === "automation") {
        const updated = await addAutomationClip({
          track_id: trackId,
          name: "Automation Clip",
          start_tick: 0,
          length_ticks: 1_920,
          target_parameter_id: get().automationParameterIds[0],
          points: [
            { tick: 0, value: 1 },
            { tick: 960, value: 0.5 },
            { tick: 1_920, value: 1 }
          ]
        });
        const targetTrack = updated.tracks.find((candidate) => candidate.id === trackId);
        const selectedClipId = targetTrack?.clips[targetTrack.clips.length - 1]?.id ?? null;
        set({ project: updated, selectedTrackId: trackId, selectedClipId });
        await get().refreshParity();
        return;
      }
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

  addAutomationLaneClip: async (trackId, targetParameterId) => {
    await withErrorHandling(set, async () => {
      const updated = await addAutomationClip({
        track_id: trackId,
        name: "Automation Clip",
        start_tick: 0,
        length_ticks: 1_920,
        target_parameter_id: targetParameterId ?? get().automationParameterIds[0],
        points: [
          { tick: 0, value: 1 },
          { tick: 960, value: 0.7 },
          { tick: 1_920, value: 1 }
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
      await get().reloadAutomationTargets();
    });
  },

  saveTrackSend: async (trackId, send) => {
    await withErrorHandling(set, async () => {
      const updated = await upsertTrackSend({
        track_id: trackId,
        send
      });
      set({ project: updated });
    });
  },

  deleteTrackSend: async (trackId, sendId) => {
    await withErrorHandling(set, async () => {
      const updated = await removeTrackSend({
        track_id: trackId,
        send_id: sendId
      });
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

  replacePatternRows: async (trackId, clipId, rows, linesPerBeat) => {
    await withErrorHandling(set, async () => {
      const updated = await updatePatternRows({
        track_id: trackId,
        clip_id: clipId,
        rows,
        lines_per_beat: linesPerBeat
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  replacePatternMacros: async (trackId, clipId, macros) => {
    await withErrorHandling(set, async () => {
      const updated = await updatePatternMacros({
        track_id: trackId,
        clip_id: clipId,
        macros
      });
      set({ project: updated, selectedTrackId: trackId, selectedClipId: clipId });
      await get().refreshParity();
    });
  },

  replaceAutomationClip: async (trackId, clipId, targetParameterId, points) => {
    await withErrorHandling(set, async () => {
      const updated = await updateAutomationClip({
        track_id: trackId,
        clip_id: clipId,
        target_parameter_id: targetParameterId,
        points
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

  scanAudioLibrary: async (directory) => {
    await withErrorHandling(set, async () => {
      const nextDirectory = directory?.trim() || get().audioScanDirectory;
      const assets = await scanAudioAssets({ directory: nextDirectory });
      const previousSelection = get().selectedAudioAssetPath;
      const selectedAudioAssetPath =
        (previousSelection && assets.some((asset) => asset.path === previousSelection)
          ? previousSelection
          : assets[0]?.path) ?? null;

      const audioPreview = selectedAudioAssetPath
        ? await analyzeAudioAsset({ path: selectedAudioAssetPath })
        : null;
      set({
        audioScanDirectory: nextDirectory,
        audioAssets: assets,
        selectedAudioAssetPath,
        audioPreview
      });
    });
  },

  previewAudioAsset: async (assetPath) => {
    await withErrorHandling(set, async () => {
      const analysis = await analyzeAudioAsset({ path: assetPath });
      set({
        selectedAudioAssetPath: assetPath,
        audioPreview: analysis
      });
    });
  },

  importAudioAsset: async (assetPath, startTick) => {
    await withErrorHandling(set, async () => {
      const project = get().project;
      if (!project) {
        return;
      }

      let workingProject = project;
      let targetTrack =
        workingProject.tracks.find(
          (track) => track.id === get().selectedTrackId && track.kind === "audio"
        ) ?? workingProject.tracks.find((track) => track.kind === "audio");

      if (!targetTrack) {
        workingProject = await addTrack({
          name: nextTrackName(workingProject, "audio"),
          color: nextTrackColor(workingProject),
          kind: "audio"
        });
        targetTrack = workingProject.tracks.find((track) => track.kind === "audio");
      }
      if (!targetTrack) {
        throw new Error("failed to resolve audio track for import");
      }

      const inferredName =
        assetPath.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "Audio Clip";
      const updated = await importAudioClip({
        track_id: targetTrack.id,
        name: inferredName,
        source_path: assetPath,
        start_tick: startTick ?? workingProject.transport.playhead_tick
      });
      const updatedTrack = updated.tracks.find((track) => track.id === targetTrack.id);
      const selectedClipId = updatedTrack?.clips[updatedTrack.clips.length - 1]?.id ?? null;
      const audioPreview = await analyzeAudioAsset({ path: assetPath });

      set({
        project: updated,
        selectedTrackId: targetTrack.id,
        selectedClipId,
        selectedAudioAssetPath: assetPath,
        audioPreview
      });
      await get().refreshParity();
    });
  },

  patchAudioClipSettings: async (trackId, clipId, patch) => {
    await withErrorHandling(set, async () => {
      const updated = await updateAudioClip({
        track_id: trackId,
        clip_id: clipId,
        ...patch
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

      const path = exportPathFor(kind, get().outputRoot, project.title);
      const renderMode = get().exportRenderMode;
      await exportProject({ kind, output_path: path, render_mode: renderMode });
      logger.info("export completed", { kind, path, render_mode: renderMode });
    });
  },

  setExportRenderMode: (mode) => set({ exportRenderMode: mode }),

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
      const automationParameterIds = await getAutomationParameterIds();
      set({
        project: updated,
        automationParameterIds,
        selectedTrackId: updated.tracks[0]?.id ?? null,
        selectedClipId: updated.tracks[0]?.clips[0]?.id ?? null
      });
      await get().refreshParity();
    });
  },

  runAutosave: async () => {
    await withErrorHandling(set, async () => {
      await autosaveProject("data/autosave");
      const autosaveStatus = await getAutosaveStatus();
      set({
        autosaveRecoveryPath: autosaveStatus.exists ? autosaveStatus.path : null,
        autosaveRecoveryModifiedEpochMs: autosaveStatus.exists
          ? autosaveStatus.modified_epoch_ms
          : null
      });
      logger.info("autosave completed");
    });
  },

  reloadAutomationTargets: async () => {
    await withErrorHandling(set, async () => {
      const automationParameterIds = await getAutomationParameterIds();
      set({ automationParameterIds });
    });
  },

  restoreAutosave: async () => {
    await withErrorHandling(set, async () => {
      const autosavePath = get().autosaveRecoveryPath;
      if (!autosavePath) {
        return;
      }
      const updated = await loadProject(autosavePath);
      set({
        project: updated,
        selectedTrackId: updated.tracks[0]?.id ?? null,
        selectedClipId: updated.tracks[0]?.clips[0]?.id ?? null,
        autosaveRecoveryPath: null,
        autosaveRecoveryModifiedEpochMs: null
      });
      await get().refreshParity();
      await get().reloadAutomationTargets();
    });
  },

  dismissAutosaveRecovery: () =>
    set({ autosaveRecoveryPath: null, autosaveRecoveryModifiedEpochMs: null }),

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
