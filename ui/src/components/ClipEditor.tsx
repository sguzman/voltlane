import { useEffect, useMemo, useRef, useState } from "react";

import type { AutomationPoint, ChipMacroLane, Clip, MidiNote, TrackerRow } from "../types";

interface ClipEditorProps {
  clip: Clip | null;
  trackId: string | null;
  ppq: number;
  loading: boolean;
  onMoveClip: (trackId: string, clipId: string, startTick: number, lengthTicks: number) => void;
  onAddNote: (trackId: string, clipId: string, note: MidiNote) => void;
  onRemoveNote: (trackId: string, clipId: string, noteIndex: number) => void;
  onReplaceNotes: (trackId: string, clipId: string, notes: MidiNote[]) => void;
  onReplacePatternRows: (
    trackId: string,
    clipId: string,
    rows: TrackerRow[],
    linesPerBeat?: number
  ) => void;
  onReplacePatternMacros: (trackId: string, clipId: string, macros: ChipMacroLane[]) => void;
  automationParameterIds: string[];
  onReplaceAutomationClip: (
    trackId: string,
    clipId: string,
    targetParameterId: string | undefined,
    points: AutomationPoint[]
  ) => void;
  onTranspose: (trackId: string, clipId: string, semitones: number) => void;
  onQuantize: (trackId: string, clipId: string, gridTicks: number) => void;
  onPatchAudioClip: (
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
  ) => void;
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

function clipTrackerRows(clip: Clip | null): TrackerRow[] {
  if (!clip || !("pattern" in clip.payload)) {
    return [];
  }
  return clip.payload.pattern.rows;
}

function clipPatternMacros(clip: Clip | null): ChipMacroLane[] {
  if (!clip || !("pattern" in clip.payload)) {
    return [];
  }
  return clip.payload.pattern.macros;
}

function clipAutomationPoints(clip: Clip | null): AutomationPoint[] {
  if (!clip || !("automation" in clip.payload)) {
    return [];
  }
  return clip.payload.automation.points;
}

function clipAutomationTarget(clip: Clip | null): string {
  if (!clip || !("automation" in clip.payload)) {
    return "";
  }
  return clip.payload.automation.target_parameter_id;
}

const PIANO_MIN_PITCH = 24;
const PIANO_MAX_PITCH = 96;
const PIANO_ROW_HEIGHT = 14;

type PianoDragState = {
  mode: "move" | "resize";
  noteIndex: number;
  startClientX: number;
  startClientY: number;
  originalNote: MidiNote;
};

export function ClipEditor({
  clip,
  trackId,
  ppq,
  loading,
  onMoveClip,
  onAddNote,
  onRemoveNote,
  onReplaceNotes,
  onReplacePatternRows,
  onReplacePatternMacros,
  automationParameterIds,
  onReplaceAutomationClip,
  onTranspose,
  onQuantize,
  onPatchAudioClip
}: ClipEditorProps) {
  const [clipStart, setClipStart] = useState(0);
  const [clipLength, setClipLength] = useState(1_920);
  const [draftNotes, setDraftNotes] = useState<MidiNote[]>([]);
  const [draftRows, setDraftRows] = useState<TrackerRow[]>([]);
  const [draftMacros, setDraftMacros] = useState<ChipMacroLane[]>([]);
  const [draftAutomationPoints, setDraftAutomationPoints] = useState<AutomationPoint[]>([]);
  const [draftAutomationTarget, setDraftAutomationTarget] = useState("");
  const [pianoSnapTicks, setPianoSnapTicks] = useState(120);
  const [pianoDrag, setPianoDrag] = useState<PianoDragState | null>(null);
  const pianoRollRef = useRef<HTMLDivElement | null>(null);
  const [linesPerBeat, setLinesPerBeat] = useState(4);
  const [audioGainDb, setAudioGainDb] = useState(0);
  const [audioPan, setAudioPan] = useState(0);
  const [trimStartSeconds, setTrimStartSeconds] = useState(0);
  const [trimEndSeconds, setTrimEndSeconds] = useState(0);
  const [fadeInSeconds, setFadeInSeconds] = useState(0);
  const [fadeOutSeconds, setFadeOutSeconds] = useState(0);
  const [stretchRatio, setStretchRatio] = useState(1);
  const [reverse, setReverse] = useState(false);

  useEffect(() => {
    setClipStart(clip?.start_tick ?? 0);
    setClipLength(clip?.length_ticks ?? 1_920);
    setDraftNotes(clipNotes(clip));
    setDraftRows(clipTrackerRows(clip));
    setDraftMacros(clipPatternMacros(clip));
    setDraftAutomationPoints(clipAutomationPoints(clip));
    setDraftAutomationTarget(clipAutomationTarget(clip));
    setPianoDrag(null);
    if (clip && "pattern" in clip.payload) {
      setLinesPerBeat(clip.payload.pattern.lines_per_beat);
    } else {
      setLinesPerBeat(4);
    }

    if (clip && "audio" in clip.payload) {
      const audio = clip.payload.audio;
      setAudioGainDb(audio.gain_db);
      setAudioPan(audio.pan);
      setTrimStartSeconds(audio.trim_start_seconds);
      setTrimEndSeconds(audio.trim_end_seconds);
      setFadeInSeconds(audio.fade_in_seconds);
      setFadeOutSeconds(audio.fade_out_seconds);
      setStretchRatio(audio.stretch_ratio);
      setReverse(audio.reverse);
    }
  }, [clip]);

  useEffect(() => {
    setPianoSnapTicks(Math.max(1, Math.round(ppq / 4)));
  }, [ppq]);

  const isMidiEditable = useMemo(() => {
    if (!clip) {
      return false;
    }
    return "midi" in clip.payload;
  }, [clip]);

  const isPatternEditable = useMemo(() => {
    if (!clip) {
      return false;
    }
    return "pattern" in clip.payload;
  }, [clip]);

  const isAutomationEditable = useMemo(() => {
    if (!clip) {
      return false;
    }
    return "automation" in clip.payload;
  }, [clip]);

  const isAudioEditable = useMemo(() => {
    return clip ? "audio" in clip.payload : false;
  }, [clip]);

  const pianoVisibleTicks = useMemo(() => {
    const noteEnd = draftNotes.reduce((max, note) => Math.max(max, note.start_tick + note.length_ticks), 0);
    return Math.max(ppq * 4, clipLength, noteEnd + ppq);
  }, [clipLength, draftNotes, ppq]);

  const pianoRows = PIANO_MAX_PITCH - PIANO_MIN_PITCH + 1;

  useEffect(() => {
    if (!pianoDrag) {
      return;
    }

    const onPointerMove = (event: MouseEvent) => {
      const container = pianoRollRef.current;
      if (!container) {
        return;
      }
      const width = container.clientWidth;
      if (width <= 0) {
        return;
      }

      const dx = event.clientX - pianoDrag.startClientX;
      const dy = event.clientY - pianoDrag.startClientY;
      const ticksPerPixel = pianoVisibleTicks / width;
      const tickDeltaRaw = Math.round(dx * ticksPerPixel);
      const tickDelta =
        Math.round(tickDeltaRaw / Math.max(1, pianoSnapTicks)) * Math.max(1, pianoSnapTicks);
      const rowDelta = Math.round(dy / PIANO_ROW_HEIGHT);

      setDraftNotes((notes) =>
        notes.map((note, noteIndex) => {
          if (noteIndex !== pianoDrag.noteIndex) {
            return note;
          }

          if (pianoDrag.mode === "move") {
            const nextPitch = Math.max(
              PIANO_MIN_PITCH,
              Math.min(PIANO_MAX_PITCH, pianoDrag.originalNote.pitch - rowDelta)
            );
            return {
              ...note,
              pitch: nextPitch,
              start_tick: Math.max(0, pianoDrag.originalNote.start_tick + tickDelta)
            };
          }

          return {
            ...note,
            length_ticks: Math.max(
              Math.max(1, pianoSnapTicks),
              pianoDrag.originalNote.length_ticks + tickDelta
            )
          };
        })
      );
    };

    const onPointerUp = () => {
      setPianoDrag(null);
    };

    window.addEventListener("mousemove", onPointerMove);
    window.addEventListener("mouseup", onPointerUp);
    return () => {
      window.removeEventListener("mousemove", onPointerMove);
      window.removeEventListener("mouseup", onPointerUp);
    };
  }, [pianoDrag, pianoVisibleTicks, pianoSnapTicks]);

  if (!clip || !trackId) {
    return (
      <aside className="panel panel--clip-editor">
        <h2>Clip Editor</h2>
        <p>Select a clip to edit note data and timing.</p>
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
      </div>

      {isMidiEditable ? (
        <>
          <div className="clip-editor__actions">
            <label className="field">
              <span>Piano Snap (ticks)</span>
              <input
                type="number"
                min={1}
                step={1}
                value={pianoSnapTicks}
                onChange={(event) => setPianoSnapTicks(Math.max(1, Number(event.target.value)))}
              />
            </label>
          </div>

          <div
            className="piano-roll"
            ref={pianoRollRef}
            style={{ height: `${pianoRows * PIANO_ROW_HEIGHT}px` }}
          >
            {Array.from({ length: pianoRows }, (_, rowIndex) => {
              const pitch = PIANO_MAX_PITCH - rowIndex;
              return (
                <div
                  key={`row-${pitch}`}
                  className={`piano-roll__row ${pitch % 12 === 0 ? "piano-roll__row--octave" : ""}`}
                  style={{ top: `${rowIndex * PIANO_ROW_HEIGHT}px` }}
                />
              );
            })}
            {draftNotes.map((note, noteIndex) => {
              const clampedPitch = Math.max(PIANO_MIN_PITCH, Math.min(PIANO_MAX_PITCH, note.pitch));
              const left = (note.start_tick / pianoVisibleTicks) * 100;
              const width = Math.max((note.length_ticks / pianoVisibleTicks) * 100, 0.8);
              const top = (PIANO_MAX_PITCH - clampedPitch) * PIANO_ROW_HEIGHT;
              return (
                <div
                  key={`piano-note-${noteIndex}-${note.start_tick}-${note.pitch}`}
                  className="piano-roll__note"
                  style={{
                    left: `${left}%`,
                    width: `${width}%`,
                    top: `${top}px`,
                    height: `${PIANO_ROW_HEIGHT - 1}px`
                  }}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    setPianoDrag({
                      mode: "move",
                      noteIndex,
                      startClientX: event.clientX,
                      startClientY: event.clientY,
                      originalNote: { ...note }
                    });
                  }}
                >
                  <span className="piano-roll__note-label">{note.pitch}</span>
                  <button
                    type="button"
                    className="piano-roll__resize"
                    onMouseDown={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      setPianoDrag({
                        mode: "resize",
                        noteIndex,
                        startClientX: event.clientX,
                        startClientY: event.clientY,
                        originalNote: { ...note }
                      });
                    }}
                  />
                </div>
              );
            })}
          </div>

          <div className="clip-editor__actions">
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
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onTranspose(trackId, clip.id, -12)}
            >
              -12 st
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onTranspose(trackId, clip.id, -1)}
            >
              -1 st
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onTranspose(trackId, clip.id, 1)}
            >
              +1 st
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onTranspose(trackId, clip.id, 12)}
            >
              +12 st
            </button>
          </div>

          <div className="clip-editor__actions">
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onQuantize(trackId, clip.id, Math.max(1, ppq / 4))}
            >
              Quantize 1/16
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onQuantize(trackId, clip.id, Math.max(1, ppq / 2))}
            >
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
        </>
      ) : null}

      {isPatternEditable ? (
        <>
          <div className="clip-editor__actions">
            <label className="field">
              <span>Lines / Beat</span>
              <input
                type="number"
                min={1}
                max={64}
                value={linesPerBeat}
                onChange={(event) => setLinesPerBeat(Number(event.target.value))}
              />
            </label>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => {
                const nextRow = (draftRows[draftRows.length - 1]?.row ?? -1) + 1;
                setDraftRows([
                  ...draftRows,
                  {
                    row: Math.max(0, nextRow),
                    note: 60,
                    velocity: 100,
                    gate: true,
                    effect: null,
                    effect_value: null
                  }
                ]);
              }}
            >
              Add Tracker Row
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onReplacePatternRows(trackId, clip.id, draftRows, linesPerBeat)}
            >
              Save Tracker Grid
            </button>
          </div>

          <div className="clip-editor__table-wrap">
            <table className="clip-editor__table clip-editor__table--tracker">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Row</th>
                  <th>Note</th>
                  <th>Vel</th>
                  <th>Gate</th>
                  <th>FX</th>
                  <th>Val</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {draftRows.map((row, index) => (
                  <tr key={`${row.row}-${index}`}>
                    <td>{index + 1}</td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        value={row.row}
                        onChange={(event) => {
                          const next = [...draftRows];
                          next[index] = { ...row, row: Number(event.target.value) };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        max={127}
                        value={row.note ?? 60}
                        onChange={(event) => {
                          const next = [...draftRows];
                          next[index] = { ...row, note: Number(event.target.value) };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        max={127}
                        value={row.velocity}
                        onChange={(event) => {
                          const next = [...draftRows];
                          next[index] = { ...row, velocity: Number(event.target.value) };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="checkbox"
                        checked={row.gate}
                        onChange={(event) => {
                          const next = [...draftRows];
                          next[index] = { ...row, gate: event.target.checked };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        value={row.effect ?? ""}
                        onChange={(event) => {
                          const value = event.target.value.trim();
                          const next = [...draftRows];
                          next[index] = { ...row, effect: value.length > 0 ? value : null };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        max={65535}
                        value={row.effect_value ?? 0}
                        onChange={(event) => {
                          const next = [...draftRows];
                          next[index] = { ...row, effect_value: Number(event.target.value) };
                          setDraftRows(next);
                        }}
                      />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="mini"
                        disabled={loading}
                        onClick={() => {
                          const next = draftRows.filter((_, candidateIndex) => candidateIndex !== index);
                          setDraftRows(next);
                        }}
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
                {draftRows.length === 0 ? (
                  <tr>
                    <td colSpan={8}>No tracker rows yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>

          <h3 className="clip-editor__subheading">Chip Macros</h3>
          <div className="clip-editor__actions">
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => {
                setDraftMacros([
                  ...draftMacros,
                  {
                    target: "arpeggio",
                    enabled: true,
                    values: [0, 4, 7],
                    loop_start: 0,
                    loop_end: 2
                  }
                ]);
              }}
            >
              Add Macro Lane
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() => onReplacePatternMacros(trackId, clip.id, draftMacros)}
            >
              Save Macros
            </button>
          </div>

          <div className="clip-editor__table-wrap">
            <table className="clip-editor__table clip-editor__table--tracker">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Target</th>
                  <th>Enabled</th>
                  <th>Values</th>
                  <th>Loop Start</th>
                  <th>Loop End</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {draftMacros.map((lane, index) => (
                  <tr key={`${lane.target}-${index}`}>
                    <td>{index + 1}</td>
                    <td>
                      <input
                        value={lane.target}
                        onChange={(event) => {
                          const next = [...draftMacros];
                          next[index] = { ...lane, target: event.target.value };
                          setDraftMacros(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="checkbox"
                        checked={lane.enabled}
                        onChange={(event) => {
                          const next = [...draftMacros];
                          next[index] = { ...lane, enabled: event.target.checked };
                          setDraftMacros(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        value={lane.values.join(",")}
                        onChange={(event) => {
                          const values = event.target.value
                            .split(",")
                            .map((value) => Number(value.trim()))
                            .filter((value) => Number.isFinite(value));
                          const next = [...draftMacros];
                          next[index] = { ...lane, values };
                          setDraftMacros(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        value={lane.loop_start ?? 0}
                        onChange={(event) => {
                          const next = [...draftMacros];
                          next[index] = { ...lane, loop_start: Number(event.target.value) };
                          setDraftMacros(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        value={lane.loop_end ?? 0}
                        onChange={(event) => {
                          const next = [...draftMacros];
                          next[index] = { ...lane, loop_end: Number(event.target.value) };
                          setDraftMacros(next);
                        }}
                      />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="mini"
                        disabled={loading}
                        onClick={() => {
                          const next = draftMacros.filter((_, candidateIndex) => candidateIndex !== index);
                          setDraftMacros(next);
                        }}
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
                {draftMacros.length === 0 ? (
                  <tr>
                    <td colSpan={7}>No macro lanes yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </>
      ) : null}

      {isAutomationEditable ? (
        <>
          <h3 className="clip-editor__subheading">Automation Lane</h3>
          <div className="panel__grid">
            <label className="field">
              <span>Target Parameter</span>
              <select
                value={draftAutomationTarget}
                onChange={(event) => setDraftAutomationTarget(event.target.value)}
              >
                {automationParameterIds.length === 0 ? (
                  <option value={draftAutomationTarget}>No automatable params</option>
                ) : null}
                {automationParameterIds.map((target) => (
                  <option key={target} value={target}>
                    {target}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() =>
                setDraftAutomationPoints([
                  ...draftAutomationPoints,
                  { tick: 0, value: 1 }
                ])
              }
            >
              Add Point
            </button>
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() =>
                onReplaceAutomationClip(
                  trackId,
                  clip.id,
                  draftAutomationTarget,
                  draftAutomationPoints
                )
              }
            >
              Save Automation
            </button>
          </div>

          <div className="clip-editor__table-wrap">
            <table className="clip-editor__table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Tick</th>
                  <th>Value</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {draftAutomationPoints.map((point, index) => (
                  <tr key={`${point.tick}-${index}`}>
                    <td>{index + 1}</td>
                    <td>
                      <input
                        type="number"
                        min={0}
                        step={30}
                        value={point.tick}
                        onChange={(event) => {
                          const next = [...draftAutomationPoints];
                          next[index] = { ...point, tick: Math.max(0, Number(event.target.value)) };
                          setDraftAutomationPoints(next);
                        }}
                      />
                    </td>
                    <td>
                      <input
                        type="number"
                        step={0.01}
                        value={point.value}
                        onChange={(event) => {
                          const next = [...draftAutomationPoints];
                          next[index] = { ...point, value: Number(event.target.value) };
                          setDraftAutomationPoints(next);
                        }}
                      />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="mini"
                        disabled={loading}
                        onClick={() =>
                          setDraftAutomationPoints(
                            draftAutomationPoints.filter(
                              (_, candidateIndex) => candidateIndex !== index
                            )
                          )
                        }
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
                {draftAutomationPoints.length === 0 ? (
                  <tr>
                    <td colSpan={4}>No automation points yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </>
      ) : null}

      {isAudioEditable ? (
        <>
          <div className="panel__grid">
            <label className="field">
              <span>Gain (dB)</span>
              <input
                type="number"
                step={0.1}
                value={audioGainDb}
                onChange={(event) => setAudioGainDb(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Pan</span>
              <input
                type="number"
                min={-1}
                max={1}
                step={0.1}
                value={audioPan}
                onChange={(event) => setAudioPan(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Trim Start (s)</span>
              <input
                type="number"
                min={0}
                step={0.01}
                value={trimStartSeconds}
                onChange={(event) => setTrimStartSeconds(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Trim End (s)</span>
              <input
                type="number"
                min={0}
                step={0.01}
                value={trimEndSeconds}
                onChange={(event) => setTrimEndSeconds(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Fade In (s)</span>
              <input
                type="number"
                min={0}
                step={0.01}
                value={fadeInSeconds}
                onChange={(event) => setFadeInSeconds(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Fade Out (s)</span>
              <input
                type="number"
                min={0}
                step={0.01}
                value={fadeOutSeconds}
                onChange={(event) => setFadeOutSeconds(Number(event.target.value))}
              />
            </label>
            <label className="field">
              <span>Stretch Ratio</span>
              <input
                type="number"
                min={0.01}
                step={0.01}
                value={stretchRatio}
                onChange={(event) => setStretchRatio(Number(event.target.value))}
              />
            </label>
            <label className="field field--checkbox">
              <span>Reverse</span>
              <input
                type="checkbox"
                checked={reverse}
                onChange={(event) => setReverse(event.target.checked)}
              />
            </label>
          </div>
          <div className="clip-editor__actions">
            <button
              type="button"
              className="pill"
              disabled={loading}
              onClick={() =>
                onPatchAudioClip(trackId, clip.id, {
                  gain_db: audioGainDb,
                  pan: audioPan,
                  trim_start_seconds: trimStartSeconds,
                  trim_end_seconds: trimEndSeconds,
                  fade_in_seconds: fadeInSeconds,
                  fade_out_seconds: fadeOutSeconds,
                  stretch_ratio: stretchRatio,
                  reverse
                })
              }
            >
              Apply Audio Edits
            </button>
          </div>
        </>
      ) : null}

      {!isMidiEditable && !isPatternEditable && !isAutomationEditable && !isAudioEditable ? (
        <p>This clip type is not editable yet.</p>
      ) : null}
    </aside>
  );
}
