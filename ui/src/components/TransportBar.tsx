import type { Project } from "../types";

interface TransportBarProps {
  project: Project;
  loading: boolean;
  onPlay: (isPlaying: boolean) => void;
  onLoopToggle: (enabled: boolean) => void;
  onExport: (kind: "midi" | "wav" | "mp3") => void;
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
  onAutosave,
  onSave,
  onLoad
}: TransportBarProps) {
  const playing = project.transport.is_playing;

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
          onClick={() => onLoopToggle(!project.transport.loop_enabled)}
          disabled={loading}
        >
          Loop
        </button>
        <span className="transport__meta">BPM {project.bpm.toFixed(1)}</span>
        <span className="transport__meta">SR {project.sample_rate}Hz</span>
      </section>

      <section className="transport__section transport__actions">
        <button type="button" className="pill" onClick={() => onExport("midi")} disabled={loading}>
          Export MIDI
        </button>
        <button type="button" className="pill" onClick={() => onExport("wav")} disabled={loading}>
          Export WAV
        </button>
        <button type="button" className="pill" onClick={() => onExport("mp3")} disabled={loading}>
          Export MP3
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
