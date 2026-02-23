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

export interface TrackerRow {
  row: number;
  note: number | null;
  velocity: number;
  gate: boolean;
  effect: string | null;
  effect_value: number | null;
}

export interface ChipMacroLane {
  target: string;
  enabled: boolean;
  values: number[];
  loop_start: number | null;
  loop_end: number | null;
}

export interface PatternClip {
  source_chip: string;
  notes: MidiNote[];
  rows: TrackerRow[];
  macros: ChipMacroLane[];
  lines_per_beat: number;
}

export interface AudioClip {
  source_path: string;
  gain_db: number;
  pan: number;
  source_sample_rate: number;
  source_channels: number;
  source_duration_seconds: number;
  trim_start_seconds: number;
  trim_end_seconds: number;
  fade_in_seconds: number;
  fade_out_seconds: number;
  reverse: boolean;
  stretch_ratio: number;
  waveform_bucket_size: number;
  waveform_peaks: number[];
  waveform_cache_path: string | null;
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

export interface ScanAudioAssetsInput {
  directory?: string;
}

export interface AudioAssetEntry {
  path: string;
  extension: string;
  size_bytes: number;
}

export interface AudioWaveformPeaks {
  bucket_size: number;
  peaks: number[];
}

export interface AudioAnalysis {
  source_path: string;
  sample_rate: number;
  channels: number;
  total_frames: number;
  duration_seconds: number;
  peaks: AudioWaveformPeaks;
  cache_path: string | null;
}

export interface AnalyzeAudioAssetInput {
  path: string;
  cache_dir?: string;
  bucket_size?: number;
}

export interface ImportAudioClipInput {
  track_id: string;
  name?: string;
  source_path: string;
  start_tick: number;
  cache_dir?: string;
  bucket_size?: number;
}

export interface UpdateAudioClipInput {
  track_id: string;
  clip_id: string;
  gain_db?: number;
  pan?: number;
  trim_start_seconds?: number;
  trim_end_seconds?: number;
  fade_in_seconds?: number;
  fade_out_seconds?: number;
  reverse?: boolean;
  stretch_ratio?: number;
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

export interface UpdatePatternRowsInput {
  track_id: string;
  clip_id: string;
  rows: TrackerRow[];
  lines_per_beat?: number;
}

export interface UpdatePatternMacrosInput {
  track_id: string;
  clip_id: string;
  macros: ChipMacroLane[];
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

export type ExportKind = "midi" | "wav" | "mp3" | "stem_wav";
export type RenderMode = "offline" | "realtime";

export interface ExportProjectInput {
  kind: ExportKind;
  output_path: string;
  ffmpeg_binary?: string;
  render_mode?: RenderMode;
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
