import { useEffect, useMemo, useState } from "react";

import { ParityPanel } from "./components/ParityPanel";
import { TrackLane } from "./components/TrackLane";
import { TransportBar } from "./components/TransportBar";
import { useProjectStore } from "./store/projectStore";
import type { TrackKind } from "./types";

const TRACK_KINDS: TrackKind[] = ["midi", "chip", "audio", "automation"];

export default function App() {
  const [newProjectName, setNewProjectName] = useState("Voltlane Session");

  const {
    project,
    parity,
    loading,
    error,
    selectedTrackId,
    bootstrap,
    createNewProject,
    addTrackByKind,
    addQuickClip,
    addBasicEffect,
    setTrackFlag,
    shiftTrack,
    setPlaybackState,
    configureLoop,
    runExport,
    saveCurrentProject,
    loadCurrentProject,
    runAutosave,
    refreshParity,
    clearError,
    setSelectedTrack
  } = useProjectStore();

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  const selectedTrack = useMemo(
    () => project?.tracks.find((track) => track.id === selectedTrackId) ?? null,
    [project, selectedTrackId]
  );

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
        onLoopToggle={(enabled) => void configureLoop(enabled)}
        onExport={(kind) => void runExport(kind)}
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
              </div>
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
                onSelect={(trackId) => setSelectedTrack(trackId)}
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

        <ParityPanel project={project} parity={parity} onRefreshParity={() => void refreshParity()} />
      </section>

      {error ? (
        <div className="toast" role="alert">
          <span>{error}</span>
          <button type="button" onClick={clearError}>
            Dismiss
          </button>
        </div>
      ) : null}
    </main>
  );
}
