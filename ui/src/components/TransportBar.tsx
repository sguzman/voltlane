import { useEffect, useState } from "react";

import type { Project, RenderMode } from "../types";

interface TransportBarProps {
  project: Project;
  loading: boolean;
  onPlay: (isPlaying: boolean) => void;
  onLoopToggle: (enabled: boolean, loopStartTick?: number, loopEndTick?: number) => void;
  onExport: (kind: "midi" | "wav" | "mp3" | "stem_wav") => void;
  exportRenderMode: RenderMode;
  onExportRenderModeChange: (mode: RenderMode) => void;
  onAutosave: () => void;
  onSave: () => void;
  onLoad: () => void;
}

export function TransportBar({
  project,
  loading,
  onPlay,
  onLoopToggle,
  onExport,
  exportRenderMode,
  onExportRenderModeChange,
  onAutosave,
  onSave,
  onLoad
}: TransportBarProps) {
  const playing = project.transport.is_playing;
  const [loopStartTick, setLoopStartTick] = useState(project.transport.loop_start_tick);
  const [loopEndTick, setLoopEndTick] = useState(project.transport.loop_end_tick);

  useEffect(() => {
    setLoopStartTick(project.transport.loop_start_tick);
    setLoopEndTick(project.transport.loop_end_tick);
  }, [project.transport.loop_start_tick, project.transport.loop_end_tick]);

  return (
    <header className="transport">
      <section className="transport__section">
        <h1 className="transport__title">Voltlane</h1>
        <p className="transport__subtitle">Rust Core + Tauri + React</p>
      </section>

      <section className="transport__section transport__controls">
        <button
          type="button"
          className={`pill ${playing ? "pill--active" : ""}`}
          onClick={() => onPlay(!playing)}
          disabled={loading}
        >
          {playing ? "Stop" : "Play"}
        </button>
        <button
          type="button"
          className={`pill ${project.transport.loop_enabled ? "pill--active" : ""}`}
          onClick={() =>
            onLoopToggle(!project.transport.loop_enabled, loopStartTick, loopEndTick)
          }
          disabled={loading}
        >
          Loop
        </button>
        <label className="transport__field">
          <span>Loop Start</span>
          <input
            type="number"
            value={loopStartTick}
            min={0}
            step={120}
            onChange={(event) => setLoopStartTick(Number(event.target.value))}
          />
        </label>
        <label className="transport__field">
          <span>Loop End</span>
          <input
            type="number"
            value={loopEndTick}
            min={1}
            step={120}
            onChange={(event) => setLoopEndTick(Number(event.target.value))}
          />
        </label>
        <button
          type="button"
          className="pill"
          onClick={() => onLoopToggle(project.transport.loop_enabled, loopStartTick, loopEndTick)}
          disabled={loading}
        >
          Apply Loop
        </button>
        <span className="transport__meta">BPM {project.bpm.toFixed(1)}</span>
        <span className="transport__meta">SR {project.sample_rate}Hz</span>
      </section>

      <section className="transport__section transport__actions">
        <label className="transport__field">
          <span>Render Mode</span>
          <select
            value={exportRenderMode}
            onChange={(event) => onExportRenderModeChange(event.target.value as RenderMode)}
          >
            <option value="offline">Offline</option>
            <option value="realtime">Realtime</option>
          </select>
        </label>
        <button type="button" className="pill" onClick={() => onExport("midi")} disabled={loading}>
          Export MIDI
        </button>
        <button type="button" className="pill" onClick={() => onExport("wav")} disabled={loading}>
          Export WAV
        </button>
        <button type="button" className="pill" onClick={() => onExport("mp3")} disabled={loading}>
          Export MP3
        </button>
        <button type="button" className="pill" onClick={() => onExport("stem_wav")} disabled={loading}>
          Export Stems
        </button>
        <button type="button" className="pill" onClick={onAutosave} disabled={loading}>
          Autosave
        </button>
        <button type="button" className="pill" onClick={onSave} disabled={loading}>
          Save
        </button>
        <button type="button" className="pill" onClick={onLoad} disabled={loading}>
          Load
        </button>
      </section>
    </header>
  );
}
