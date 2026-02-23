export type TrackKind = "midi" | "chip" | "audio" | "automation" | "bus";

export interface Transport {
  playhead_tick: number;
  loop_enabled: boolean;
  loop_start_tick: number;
  loop_end_tick: number;
  metronome_enabled: boolean;
  is_playing: boolean;
}

export interface MidiNote {
  pitch: number;
  velocity: number;
  start_tick: number;
  length_ticks: number;
  channel: number;
}

export interface MidiClip {
  instrument: string | null;
  notes: MidiNote[];
}

export interface PatternClip {
  source_chip: string;
  notes: MidiNote[];
}

export interface AudioClip {
  source_path: string;
  gain_db: number;
  pan: number;
}

export interface AutomationPoint {
  tick: number;
  value: number;
}

export interface AutomationClip {
  target_parameter_id: string;
  points: AutomationPoint[];
}

export type ClipPayload =
  | { midi: MidiClip }
  | { pattern: PatternClip }
  | { audio: AudioClip }
  | { automation: AutomationClip };

export interface Clip {
  id: string;
  name: string;
  start_tick: number;
  length_ticks: number;
  disabled: boolean;
  payload: ClipPayload;
}

export interface EffectSpec {
  id: string;
  name: string;
  enabled: boolean;
  params: Record<string, number>;
}

export interface Track {
  id: string;
  name: string;
  color: string;
  kind: TrackKind;
  hidden: boolean;
  mute: boolean;
  solo: boolean;
  enabled: boolean;
  effects: EffectSpec[];
  clips: Clip[];
}

export interface Project {
  id: string;
  session_id: string;
  title: string;
  bpm: number;
  ppq: number;
  sample_rate: number;
  transport: Transport;
  tracks: Track[];
  created_at: string;
  updated_at: string;
}

export interface AddTrackRequest {
  name: string;
  color: string;
  kind: TrackKind;
}

export interface CreateProjectInput {
  title: string;
  bpm?: number;
  sample_rate?: number;
}

export interface AddMidiClipInput {
  track_id: string;
  name: string;
  start_tick: number;
  length_ticks: number;
  instrument?: string;
  source_chip?: string;
  notes: MidiNote[];
}

export interface MoveClipInput {
  track_id: string;
  clip_id: string;
  start_tick: number;
  length_ticks: number;
}

export interface UpdateClipNotesInput {
  track_id: string;
  clip_id: string;
  notes: MidiNote[];
}

export interface AddClipNoteInput {
  track_id: string;
  clip_id: string;
  note: MidiNote;
}

export interface RemoveClipNoteInput {
  track_id: string;
  clip_id: string;
  note_index: number;
}

export interface TransposeClipNotesInput {
  track_id: string;
  clip_id: string;
  semitones: number;
}

export interface QuantizeClipNotesInput {
  track_id: string;
  clip_id: string;
  grid_ticks: number;
}

export type ExportKind = "midi" | "wav" | "mp3";

export interface ExportProjectInput {
  kind: ExportKind;
  output_path: string;
  ffmpeg_binary?: string;
}

export interface PatchTrackInput {
  track_id: string;
  hidden?: boolean;
  mute?: boolean;
  solo?: boolean;
  enabled?: boolean;
}

export interface ParityReport {
  schema_version: number;
  project_id: string;
  track_count: number;
  clip_count: number;
  note_count: number;
  project_hash: string;
  midi_hash: string;
  audio_hash: string;
}
