import { useEffect, useMemo, useState } from "react";

import { AudioBrowserPanel } from "./components/AudioBrowserPanel";
import { ClipEditor } from "./components/ClipEditor";
import { ParityPanel } from "./components/ParityPanel";
import { TrackLane } from "./components/TrackLane";
import { TransportBar } from "./components/TransportBar";
import { useProjectStore } from "./store/projectStore";
import type { TrackKind } from "./types";

const TRACK_KINDS: TrackKind[] = ["midi", "chip", "audio", "automation", "bus"];

export default function App() {
  const [newProjectName, setNewProjectName] = useState("Voltlane Session");

  const {
    project,
    parity,
    loading,
    error,
    exportRenderMode,
    automationParameterIds,
    autosaveRecoveryPath,
    autosaveRecoveryModifiedEpochMs,
    selectedTrackId,
    selectedClipId,
    audioAssets,
    audioScanDirectory,
    selectedAudioAssetPath,
    audioPreview,
    bootstrap,
    createNewProject,
    addTrackByKind,
    addQuickClip,
    addAutomationLaneClip,
    addBasicEffect,
    setTrackFlag,
    setTrackMix,
    saveTrackSend,
    deleteTrackSend,
    shiftTrack,
    setPlaybackState,
    configureLoop,
    moveClipTiming,
    addNoteToClip,
    removeNoteAt,
    replaceClipNotes,
    replacePatternRows,
    replacePatternMacros,
    replaceAutomationClip,
    transposeClip,
    quantizeClip,
    scanAudioLibrary,
    previewAudioAsset,
    importAudioAsset,
    patchAudioClipSettings,
    runExport,
    setExportRenderMode,
    saveCurrentProject,
    loadCurrentProject,
    runAutosave,
    restoreAutosave,
    dismissAutosaveRecovery,
    refreshParity,
    clearError,
    setSelectedTrack,
    setSelectedClip
  } = useProjectStore();

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  const selectedTrack = useMemo(
    () => project?.tracks.find((track) => track.id === selectedTrackId) ?? null,
    [project, selectedTrackId]
  );

  const selectedClip = useMemo(() => {
    if (!selectedTrack) {
      return null;
    }
    return selectedTrack.clips.find((clip) => clip.id === selectedClipId) ?? null;
  }, [selectedTrack, selectedClipId]);

  if (!project) {
    return <main className="shell">Loading Voltlane...</main>;
  }

  return (
    <main className="shell">
      <div className="backdrop" />

      <TransportBar
        project={project}
        loading={loading}
        onPlay={(isPlaying) => void setPlaybackState(isPlaying)}
        onLoopToggle={(enabled, loopStartTick, loopEndTick) =>
          void configureLoop(enabled, loopStartTick, loopEndTick)
        }
        onExport={(kind) => void runExport(kind)}
        exportRenderMode={exportRenderMode}
        onExportRenderModeChange={setExportRenderMode}
        onAutosave={() => void runAutosave()}
        onSave={() => void saveCurrentProject()}
        onLoad={() => void loadCurrentProject()}
      />

      <section className="workspace">
        <aside className="panel panel--controls">
          <h2>Session Controls</h2>
          <label className="field">
            <span>Project Name</span>
            <input
              value={newProjectName}
              onChange={(event) => setNewProjectName(event.target.value)}
              placeholder="Project title"
            />
          </label>
          <button type="button" className="pill" onClick={() => void createNewProject(newProjectName)}>
            New Project
          </button>

          <div className="divider" />

          <h3>Add Track</h3>
          <div className="button-grid">
            {TRACK_KINDS.map((kind) => (
              <button key={kind} type="button" className="pill" onClick={() => void addTrackByKind(kind)}>
                + {kind}
              </button>
            ))}
          </div>

          {selectedTrack ? (
            <>
              <div className="divider" />
              <h3>Selected Track</h3>
              <p className="selected-name">{selectedTrack.name}</p>
              <div className="button-grid">
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackFlag(selectedTrack.id, {
                      mute: !selectedTrack.mute
                    })
                  }
                >
                  Toggle Mute
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackFlag(selectedTrack.id, {
                      hidden: !selectedTrack.hidden
                    })
                  }
                >
                  Toggle Hidden
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackFlag(selectedTrack.id, {
                      enabled: !selectedTrack.enabled
                    })
                  }
                >
                  Toggle Enabled
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() => void addQuickClip(selectedTrack.id, selectedTrack.kind)}
                >
                  Add Clip
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() => void addBasicEffect(selectedTrack.id, "bitcrusher")}
                >
                  Add Bitcrusher
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackMix(selectedTrack.id, {
                      gain_db: selectedTrack.gain_db + 1
                    })
                  }
                >
                  Gain +1dB
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackMix(selectedTrack.id, {
                      gain_db: selectedTrack.gain_db - 1
                    })
                  }
                >
                  Gain -1dB
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackMix(selectedTrack.id, {
                      pan: Math.max(-1, selectedTrack.pan - 0.1)
                    })
                  }
                >
                  Pan Left
                </button>
                <button
                  type="button"
                  className="pill"
                  onClick={() =>
                    void setTrackMix(selectedTrack.id, {
                      pan: Math.min(1, selectedTrack.pan + 0.1)
                    })
                  }
                >
                  Pan Right
                </button>
                {selectedTrack.kind === "automation" ? (
                  <button
                    type="button"
                    className="pill"
                    onClick={() => void addAutomationLaneClip(selectedTrack.id)}
                  >
                    Add Auto Clip
                  </button>
                ) : null}
              </div>
              {selectedTrack.kind !== "bus" ? (
                <div className="panel__grid">
                  <label className="field">
                    <span>Output Bus</span>
                    <select
                      value={selectedTrack.output_bus ?? ""}
                      onChange={(event) =>
                        void setTrackMix(selectedTrack.id, {
                          output_bus_id: event.target.value || null
                        })
                      }
                    >
                      <option value="">Master</option>
                      {project.tracks
                        .filter((candidate) => candidate.kind === "bus" && candidate.id !== selectedTrack.id)
                        .map((bus) => (
                          <option key={bus.id} value={bus.id}>
                            {bus.name}
                          </option>
                        ))}
                    </select>
                  </label>
                </div>
              ) : null}
              {selectedTrack.kind !== "bus" && project.tracks.some((candidate) => candidate.kind === "bus") ? (
                <>
                  <h3>Sends</h3>
                  <div className="button-grid">
                    {project.tracks
                      .filter((candidate) => candidate.kind === "bus" && candidate.id !== selectedTrack.id)
                      .map((bus) => (
                        <button
                          key={bus.id}
                          type="button"
                          className="pill"
                          onClick={() =>
                            void saveTrackSend(selectedTrack.id, {
                              target_bus_id: bus.id,
                              level_db: -9,
                              pan: 0,
                              enabled: true
                            })
                          }
                        >
                          Send to {bus.name}
                        </button>
                      ))}
                  </div>
                  {selectedTrack.sends.map((send) => (
                    <div key={send.id} className="clip-editor__actions">
                      <span className="transport__meta">
                        {project.tracks.find((candidate) => candidate.id === send.target_bus)?.name ?? send.target_bus}{" "}
                        {send.level_db.toFixed(1)} dB
                      </span>
                      <button
                        type="button"
                        className="mini"
                        onClick={() => void deleteTrackSend(selectedTrack.id, send.id)}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </>
              ) : null}
            </>
          ) : null}
        </aside>

        <section className="playlist">
          <header className="playlist__header">
            <h2>Playlist</h2>
            <p>FL-inspired lane workflow with clip colors and track controls.</p>
          </header>

          <div className="playlist__tracks">
            {project.tracks.map((track, index) => (
              <TrackLane
                key={track.id}
                track={track}
                index={index}
                selected={track.id === selectedTrackId}
                selectedClipId={selectedClipId}
                onSelect={(trackId) => setSelectedTrack(trackId)}
                onSelectClip={(trackId, clipId) => {
                  setSelectedTrack(trackId);
                  setSelectedClip(clipId);
                }}
                onToggleMute={(trackId, mute) => void setTrackFlag(trackId, { mute })}
                onToggleHidden={(trackId, hidden) => void setTrackFlag(trackId, { hidden })}
                onToggleEnabled={(trackId, enabled) => void setTrackFlag(trackId, { enabled })}
                onAddClip={(trackId, kind) => void addQuickClip(trackId, kind)}
                onAddEffect={(trackId) => void addBasicEffect(trackId, "delay")}
                onShiftTrack={(trackIndex, direction) => void shiftTrack(trackIndex, direction)}
              />
            ))}
          </div>
        </section>

        <ClipEditor
          clip={selectedClip}
          trackId={selectedTrackId}
          ppq={project.ppq}
          loading={loading}
          onMoveClip={(trackId, clipId, startTick, lengthTicks) =>
            void moveClipTiming(trackId, clipId, startTick, lengthTicks)
          }
          onAddNote={(trackId, clipId, note) => void addNoteToClip(trackId, clipId, note)}
          onRemoveNote={(trackId, clipId, noteIndex) => void removeNoteAt(trackId, clipId, noteIndex)}
          onReplaceNotes={(trackId, clipId, notes) => void replaceClipNotes(trackId, clipId, notes)}
          onReplacePatternRows={(trackId, clipId, rows, linesPerBeat) =>
            void replacePatternRows(trackId, clipId, rows, linesPerBeat)
          }
          onReplacePatternMacros={(trackId, clipId, macros) =>
            void replacePatternMacros(trackId, clipId, macros)
          }
          automationParameterIds={automationParameterIds}
          onReplaceAutomationClip={(trackId, clipId, targetParameterId, points) =>
            void replaceAutomationClip(trackId, clipId, targetParameterId, points)
          }
          onTranspose={(trackId, clipId, semitones) => void transposeClip(trackId, clipId, semitones)}
          onQuantize={(trackId, clipId, gridTicks) => void quantizeClip(trackId, clipId, gridTicks)}
          onPatchAudioClip={(trackId, clipId, patch) =>
            void patchAudioClipSettings(trackId, clipId, patch)
          }
        />
        <div className="sidebar-stack">
          <AudioBrowserPanel
            directory={audioScanDirectory}
            assets={audioAssets}
            selectedAssetPath={selectedAudioAssetPath}
            preview={audioPreview}
            loading={loading}
            onScan={(directory) => void scanAudioLibrary(directory)}
            onSelectAsset={(assetPath) => void previewAudioAsset(assetPath)}
            onImportAsset={(assetPath) => void importAudioAsset(assetPath)}
          />
          <ParityPanel project={project} parity={parity} onRefreshParity={() => void refreshParity()} />
        </div>
      </section>

      {error ? (
        <div className="toast" role="alert">
          <span>{error}</span>
          <button type="button" onClick={clearError}>
            Dismiss
          </button>
        </div>
      ) : null}

      {autosaveRecoveryPath ? (
        <div className="toast" role="alert">
          <span>
            Autosave found
            {autosaveRecoveryModifiedEpochMs
              ? ` (${new Date(autosaveRecoveryModifiedEpochMs).toLocaleString()})`
              : ""}
          </span>
          <button type="button" onClick={() => void restoreAutosave()}>
            Restore
          </button>
          <button type="button" onClick={dismissAutosaveRecovery}>
            Ignore
          </button>
        </div>
      ) : null}
    </main>
  );
}
