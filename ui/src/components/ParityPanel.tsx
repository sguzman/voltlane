import type { ParityReport, Project } from "../types";

interface ParityPanelProps {
  project: Project;
  parity: ParityReport | null;
  onRefreshParity: () => void;
}

export function ParityPanel({ project, parity, onRefreshParity }: ParityPanelProps) {
  return (
    <aside className="panel panel--parity">
      <h2>Parity Harness</h2>
      <p>
        Deterministic fingerprints for the current arrangement. Use this to compare engine parity
        across builds.
      </p>

      <div className="panel__grid">
        <div>
          <span className="label">Project</span>
          <strong>{project.title}</strong>
        </div>
        <div>
          <span className="label">Tracks</span>
          <strong>{parity?.track_count ?? project.tracks.length}</strong>
        </div>
        <div>
          <span className="label">Clips</span>
          <strong>{parity?.clip_count ?? "-"}</strong>
        </div>
        <div>
          <span className="label">Notes</span>
          <strong>{parity?.note_count ?? "-"}</strong>
        </div>
      </div>

      <button type="button" className="pill" onClick={onRefreshParity}>
        Refresh Parity
      </button>

      <div className="hashes">
        <p>
          <span className="label">Project Hash</span>
          <code>{parity?.project_hash ?? "not computed"}</code>
        </p>
        <p>
          <span className="label">MIDI Hash</span>
          <code>{parity?.midi_hash ?? "not computed"}</code>
        </p>
        <p>
          <span className="label">Audio Hash</span>
          <code>{parity?.audio_hash ?? "not computed"}</code>
        </p>
      </div>
    </aside>
  );
}
