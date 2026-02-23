import { useEffect, useMemo, useState } from "react";

import type { Clip, MidiNote } from "../types";

interface ClipEditorProps {
  clip: Clip | null;
  trackId: string | null;
  ppq: number;
  loading: boolean;
  onMoveClip: (trackId: string, clipId: string, startTick: number, lengthTicks: number) => void;
  onAddNote: (trackId: string, clipId: string, note: MidiNote) => void;
  onRemoveNote: (trackId: string, clipId: string, noteIndex: number) => void;
  onReplaceNotes: (trackId: string, clipId: string, notes: MidiNote[]) => void;
  onTranspose: (trackId: string, clipId: string, semitones: number) => void;
  onQuantize: (trackId: string, clipId: string, gridTicks: number) => void;
}

function clipNotes(clip: Clip | null): MidiNote[] {
  if (!clip) {
    return [];
  }
  if ("midi" in clip.payload) {
    return clip.payload.midi.notes;
  }
  if ("pattern" in clip.payload) {
    return clip.payload.pattern.notes;
  }
  return [];
}

export function ClipEditor({
  clip,
  trackId,
  ppq,
  loading,
  onMoveClip,
  onAddNote,
  onRemoveNote,
  onReplaceNotes,
  onTranspose,
  onQuantize
}: ClipEditorProps) {
  const [clipStart, setClipStart] = useState(0);
  const [clipLength, setClipLength] = useState(1_920);
  const [draftNotes, setDraftNotes] = useState<MidiNote[]>([]);

  useEffect(() => {
    setClipStart(clip?.start_tick ?? 0);
    setClipLength(clip?.length_ticks ?? 1_920);
    setDraftNotes(clipNotes(clip));
  }, [clip]);

  const isEditable = useMemo(() => {
    if (!clip) {
      return false;
    }
    return "midi" in clip.payload || "pattern" in clip.payload;
  }, [clip]);

  if (!clip || !trackId) {
    return (
      <aside className="panel panel--clip-editor">
        <h2>Clip Editor</h2>
        <p>Select a clip to edit note data and timing.</p>
      </aside>
    );
  }

  if (!isEditable) {
    return (
      <aside className="panel panel--clip-editor">
        <h2>Clip Editor</h2>
        <p>This clip type is not MIDI-editable yet.</p>
      </aside>
    );
  }

  return (
    <aside className="panel panel--clip-editor">
      <h2>Clip Editor</h2>
      <p className="clip-editor__subtitle">{clip.name}</p>

      <div className="panel__grid">
        <label className="field">
          <span>Clip Start</span>
          <input
            type="number"
            min={0}
            step={60}
            value={clipStart}
            onChange={(event) => setClipStart(Number(event.target.value))}
          />
        </label>
        <label className="field">
          <span>Clip Length</span>
          <input
            type="number"
            min={1}
            step={60}
            value={clipLength}
            onChange={(event) => setClipLength(Number(event.target.value))}
          />
        </label>
      </div>

      <div className="clip-editor__actions">
        <button
          type="button"
          className="pill"
          disabled={loading}
          onClick={() => onMoveClip(trackId, clip.id, clipStart, clipLength)}
        >
          Apply Clip Timing
        </button>
        <button
          type="button"
          className="pill"
          disabled={loading}
          onClick={() =>
            onAddNote(trackId, clip.id, {
              pitch: 60,
              velocity: 110,
              start_tick: 0,
              length_ticks: ppq,
              channel: 0
            })
          }
        >
          Add Note
        </button>
      </div>

      <div className="clip-editor__actions">
        <button type="button" className="pill" disabled={loading} onClick={() => onTranspose(trackId, clip.id, -12)}>
          -12 st
        </button>
        <button type="button" className="pill" disabled={loading} onClick={() => onTranspose(trackId, clip.id, -1)}>
          -1 st
        </button>
        <button type="button" className="pill" disabled={loading} onClick={() => onTranspose(trackId, clip.id, 1)}>
          +1 st
        </button>
        <button type="button" className="pill" disabled={loading} onClick={() => onTranspose(trackId, clip.id, 12)}>
          +12 st
        </button>
      </div>

      <div className="clip-editor__actions">
        <button type="button" className="pill" disabled={loading} onClick={() => onQuantize(trackId, clip.id, Math.max(1, ppq / 4))}>
          Quantize 1/16
        </button>
        <button type="button" className="pill" disabled={loading} onClick={() => onQuantize(trackId, clip.id, Math.max(1, ppq / 2))}>
          Quantize 1/8
        </button>
        <button
          type="button"
          className="pill"
          disabled={loading}
          onClick={() => onReplaceNotes(trackId, clip.id, draftNotes)}
        >
          Save Notes
        </button>
      </div>

      <div className="clip-editor__table-wrap">
        <table className="clip-editor__table">
          <thead>
            <tr>
              <th>#</th>
              <th>Pitch</th>
              <th>Velocity</th>
              <th>Start</th>
              <th>Length</th>
              <th>Ch</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {draftNotes.map((note, index) => (
              <tr key={`${note.pitch}-${note.start_tick}-${index}`}>
                <td>{index + 1}</td>
                <td>
                  <input
                    type="number"
                    min={0}
                    max={127}
                    value={note.pitch}
                    onChange={(event) => {
                      const next = [...draftNotes];
                      next[index] = { ...note, pitch: Number(event.target.value) };
                      setDraftNotes(next);
                    }}
                  />
                </td>
                <td>
                  <input
                    type="number"
                    min={1}
                    max={127}
                    value={note.velocity}
                    onChange={(event) => {
                      const next = [...draftNotes];
                      next[index] = { ...note, velocity: Number(event.target.value) };
                      setDraftNotes(next);
                    }}
                  />
                </td>
                <td>
                  <input
                    type="number"
                    min={0}
                    step={60}
                    value={note.start_tick}
                    onChange={(event) => {
                      const next = [...draftNotes];
                      next[index] = { ...note, start_tick: Number(event.target.value) };
                      setDraftNotes(next);
                    }}
                  />
                </td>
                <td>
                  <input
                    type="number"
                    min={1}
                    step={60}
                    value={note.length_ticks}
                    onChange={(event) => {
                      const next = [...draftNotes];
                      next[index] = { ...note, length_ticks: Number(event.target.value) };
                      setDraftNotes(next);
                    }}
                  />
                </td>
                <td>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={note.channel}
                    onChange={(event) => {
                      const next = [...draftNotes];
                      next[index] = { ...note, channel: Number(event.target.value) };
                      setDraftNotes(next);
                    }}
                  />
                </td>
                <td>
                  <button
                    type="button"
                    className="mini"
                    disabled={loading}
                    onClick={() => onRemoveNote(trackId, clip.id, index)}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
            {draftNotes.length === 0 ? (
              <tr>
                <td colSpan={7}>No notes yet.</td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </aside>
  );
}
