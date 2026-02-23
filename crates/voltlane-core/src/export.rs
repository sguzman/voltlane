use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u15, u24, u28},
};
use tracing::{debug, info, instrument, warn};

use crate::{
    assets::decode_audio_file_mono,
    engine::RenderMode,
    model::{AudioClip, ChipMacroLane, ClipPayload, MidiNote, PatternClip, Project, TrackKind},
    time::ticks_to_samples,
};

#[derive(Debug, Clone)]
struct RenderedNote {
    start_sample: usize,
    end_sample: usize,
    amplitude: f32,
    phase_increment: u32,
    waveform: Waveform,
}

#[derive(Debug, Clone, Copy)]
enum Waveform {
    Triangle,
    Square,
}

#[derive(Debug, Clone)]
struct AbsoluteMidiEvent {
    tick: u64,
    order: u8,
    kind: TrackEventKind<'static>,
}

#[instrument(skip(project), fields(project_id = %project.id))]
pub fn render_project_samples(project: &Project, tail_seconds: f64) -> Vec<f32> {
    let sample_rate = project.sample_rate.max(8_000);
    let end_tick = project.max_tick();
    let end_samples = ticks_to_samples(end_tick, project.bpm, project.ppq, sample_rate);
    let tail_samples = (tail_seconds.max(0.0) * f64::from(sample_rate)).round() as u64;
    let total_frames = end_samples
        .saturating_add(tail_samples)
        .max(u64::from(sample_rate));
    let frame_count = usize::try_from(total_frames).unwrap_or(sample_rate as usize);

    let mut buffer = vec![0.0_f32; frame_count];
    let rendered_audio_clips = mix_audio_clips(project, &mut buffer);

    let mut notes = collect_rendered_notes(project);
    for note in &mut notes {
        let mut phase = 0_u32;
        let end = note.end_sample.min(frame_count);
        for frame in &mut buffer[note.start_sample.min(frame_count)..end] {
            let osc = match note.waveform {
                Waveform::Triangle => triangle_osc(phase),
                Waveform::Square => square_osc(phase),
            };
            *frame += osc * note.amplitude;
            phase = phase.wrapping_add(note.phase_increment);
        }
    }

    for frame in &mut buffer {
        *frame = frame.clamp(-1.0, 1.0);
    }

    debug!(
        frames = buffer.len(),
        rendered_notes = notes.len(),
        rendered_audio_clips,
        "audio render completed"
    );
    buffer
}

fn render_project_samples_with_mode(
    project: &Project,
    tail_seconds: f64,
    render_mode: RenderMode,
) -> Vec<f32> {
    let rendered = render_project_samples(project, tail_seconds);
    if matches!(render_mode, RenderMode::Realtime) {
        // This keeps deterministic output while still exercising chunked realtime-style iteration.
        for _chunk in rendered.chunks(2_048) {
            std::thread::yield_now();
        }
        debug!("realtime render mode selected");
    }
    rendered
}

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display(), mode = ?render_mode))]
pub fn export_wav(project: &Project, path: &Path, render_mode: RenderMode) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create wav output directory: {}",
                parent.display()
            )
        })?;
    }

    let rendered = render_project_samples_with_mode(project, 1.0, render_mode);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: project.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("failed to create wav file: {}", path.display()))?;

    for sample in rendered {
        let quantized = (sample * f32::from(i16::MAX)).round() as i16;
        writer
            .write_sample(quantized)
            .context("failed to write left channel sample")?;
        writer
            .write_sample(quantized)
            .context("failed to write right channel sample")?;
    }

    writer.finalize().context("failed to finalize wav file")?;
    info!("wav export completed");
    Ok(())
}

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display()))]
pub fn export_midi(project: &Project, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create midi output directory: {}",
                parent.display()
            )
        })?;
    }

    let bytes = midi_bytes(project)?;
    fs::write(path, bytes)
        .with_context(|| format!("failed to write midi file: {}", path.display()))?;
    info!("midi export completed");
    Ok(())
}

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display(), mode = ?render_mode))]
pub fn export_mp3(
    project: &Project,
    path: &Path,
    ffmpeg_binary: Option<&Path>,
    render_mode: RenderMode,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create mp3 output directory: {}",
                parent.display()
            )
        })?;
    }

    let ffmpeg = ffmpeg_binary
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));

    let temp_dir = tempfile::tempdir().context("failed to create temporary export directory")?;
    let temp_wav = temp_dir.path().join("voltlane_export.wav");
    export_wav(project, &temp_wav, render_mode)?;

    let status = Command::new(&ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            temp_wav
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temporary wav path"))?,
            "-codec:a",
            "libmp3lame",
            "-qscale:a",
            "2",
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid mp3 output path"))?,
        ])
        .status()
        .with_context(|| format!("failed to spawn ffmpeg: {}", ffmpeg.display()))?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg exited with status {} while exporting mp3",
            status
        ));
    }

    info!("mp3 export completed");
    Ok(())
}

#[instrument(skip(project), fields(project_id = %project.id, output_dir = %output_dir.display(), mode = ?render_mode))]
pub fn export_stem_wav(
    project: &Project,
    output_dir: &Path,
    render_mode: RenderMode,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create stem output directory: {}",
            output_dir.display()
        )
    })?;

    let mut exported_paths = Vec::new();
    for (index, track) in project.tracks.iter().enumerate() {
        if !track.enabled || track.mute || track.hidden {
            debug!(
                track_id = %track.id,
                track_name = %track.name,
                "skipping muted/hidden/disabled track for stem export"
            );
            continue;
        }

        let mut stem_project = project.clone();
        stem_project.tracks = vec![track.clone()];
        let safe_name = sanitize_stem_name(&track.name);
        let stem_path = output_dir.join(format!("{:02}_{}.wav", index + 1, safe_name));
        export_wav(&stem_project, &stem_path, render_mode)?;
        exported_paths.push(stem_path);
    }

    info!(
        stem_count = exported_paths.len(),
        "stem wav export completed"
    );
    Ok(exported_paths)
}

#[instrument(skip(project), fields(project_id = %project.id))]
pub fn midi_bytes(project: &Project) -> Result<Vec<u8>> {
    let mut tracks = Vec::new();
    tracks.push(build_tempo_track(project));

    for (track_index, track) in project.tracks.iter().enumerate() {
        if !track.enabled || track.mute || track.hidden {
            continue;
        }

        let mut absolute_events = Vec::new();
        for clip in &track.clips {
            if clip.disabled {
                continue;
            }

            match &clip.payload {
                ClipPayload::Midi(midi_clip) => {
                    for note in &midi_clip.notes {
                        absolute_events.extend(note_to_midi_events(note, clip.start_tick));
                    }
                }
                ClipPayload::Pattern(pattern_clip) => {
                    for note in &pattern_clip.notes {
                        let macro_note = apply_pattern_macros(note, pattern_clip, project.ppq);
                        absolute_events.extend(note_to_midi_events(&macro_note, clip.start_tick));
                    }
                }
                ClipPayload::Audio(_) | ClipPayload::Automation(_) => {}
            }
        }

        if absolute_events.is_empty() {
            continue;
        }

        absolute_events.sort_by_key(|event| (event.tick, event.order));

        let mut track_events = Vec::with_capacity(absolute_events.len() + 2);
        let program = (track_index % 128) as u8;
        track_events.push(TrackEvent {
            delta: u28::from(0_u32),
            kind: TrackEventKind::Midi {
                channel: u4::from(0),
                message: MidiMessage::ProgramChange {
                    program: u7::from(program),
                },
            },
        });

        let mut previous_tick = 0_u64;
        for event in absolute_events {
            let delta = event
                .tick
                .saturating_sub(previous_tick)
                .min(u64::from(u32::MAX)) as u32;
            track_events.push(TrackEvent {
                delta: u28::from(delta),
                kind: event.kind,
            });
            previous_tick = event.tick;
        }

        track_events.push(TrackEvent {
            delta: u28::from(0_u32),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        tracks.push(track_events);
    }

    let header = Header {
        format: Format::Parallel,
        timing: Timing::Metrical(u15::from(project.ppq)),
    };

    let mut bytes = Vec::new();
    Smf { header, tracks }
        .write_std(&mut bytes)
        .context("failed to encode midi bytes")?;
    Ok(bytes)
}

fn build_tempo_track(project: &Project) -> Vec<TrackEvent<'static>> {
    let bpm = project.bpm.max(10.0);
    let micros_per_quarter = (60_000_000.0 / bpm).round() as u32;

    vec![
        TrackEvent {
            delta: u28::from(0_u32),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::from(micros_per_quarter))),
        },
        TrackEvent {
            delta: u28::from(0_u32),
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
        },
        TrackEvent {
            delta: u28::from(0_u32),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        },
    ]
}

fn note_to_midi_events(note: &MidiNote, clip_start_tick: u64) -> [AbsoluteMidiEvent; 2] {
    let channel = note.channel.min(15);
    let pitch = note.pitch.min(127);
    let velocity = note.velocity.min(127);

    let start_tick = clip_start_tick.saturating_add(note.start_tick);
    let end_tick = clip_start_tick.saturating_add(note.end_tick());

    [
        AbsoluteMidiEvent {
            tick: start_tick,
            order: 1,
            kind: TrackEventKind::Midi {
                channel: u4::from(channel),
                message: MidiMessage::NoteOn {
                    key: u7::from(pitch),
                    vel: u7::from(velocity),
                },
            },
        },
        AbsoluteMidiEvent {
            tick: end_tick,
            order: 0,
            kind: TrackEventKind::Midi {
                channel: u4::from(channel),
                message: MidiMessage::NoteOff {
                    key: u7::from(pitch),
                    vel: u7::from(0),
                },
            },
        },
    ]
}

fn mix_audio_clips(project: &Project, buffer: &mut [f32]) -> usize {
    let mut decoded_cache = HashMap::new();
    let mut rendered_audio_clips = 0_usize;

    for track in &project.tracks {
        if !track.enabled || track.mute || track.hidden {
            continue;
        }

        for clip in &track.clips {
            if clip.disabled {
                continue;
            }
            let ClipPayload::Audio(audio_clip) = &clip.payload else {
                continue;
            };

            if !decoded_cache.contains_key(&audio_clip.source_path) {
                match decode_audio_file_mono(Path::new(&audio_clip.source_path)) {
                    Ok(decoded) => {
                        decoded_cache.insert(audio_clip.source_path.clone(), decoded);
                    }
                    Err(error) => {
                        warn!(
                            path = %audio_clip.source_path,
                            ?error,
                            "failed to decode audio clip source while rendering, skipping clip"
                        );
                        continue;
                    }
                }
            }

            let Some(decoded) = decoded_cache.get(&audio_clip.source_path) else {
                continue;
            };
            if decoded.samples.is_empty() {
                continue;
            }

            mix_audio_clip_samples(
                project,
                clip.start_tick,
                clip.length_ticks,
                audio_clip,
                decoded.sample_rate,
                &decoded.samples,
                buffer,
            );
            rendered_audio_clips += 1;
        }
    }

    rendered_audio_clips
}

fn mix_audio_clip_samples(
    project: &Project,
    clip_start_tick: u64,
    clip_length_ticks: u64,
    audio: &AudioClip,
    source_sample_rate: u32,
    source_samples: &[f32],
    buffer: &mut [f32],
) {
    if source_sample_rate == 0 || source_samples.is_empty() || buffer.is_empty() {
        return;
    }

    let start_frame = ticks_to_samples(
        clip_start_tick,
        project.bpm,
        project.ppq,
        project.sample_rate,
    ) as usize;
    if start_frame >= buffer.len() {
        return;
    }

    let requested_frames = ticks_to_samples(
        clip_length_ticks.max(1),
        project.bpm,
        project.ppq,
        project.sample_rate,
    ) as usize;
    let output_frames = requested_frames.min(buffer.len().saturating_sub(start_frame));
    if output_frames == 0 {
        return;
    }

    let (trim_start_seconds, trim_end_seconds) = audio.normalized_trim_range();
    let source_start = (trim_start_seconds * f64::from(source_sample_rate)).round() as usize;
    let source_end = (trim_end_seconds * f64::from(source_sample_rate)).round() as usize;
    let source_start = source_start.min(source_samples.len().saturating_sub(1));
    let source_end = source_end.min(source_samples.len());
    if source_end <= source_start {
        return;
    }
    let source_frames = source_end.saturating_sub(source_start).max(1);

    let fade_in_frames =
        (audio.fade_in_seconds.max(0.0) * f64::from(project.sample_rate)).round() as usize;
    let fade_out_frames =
        (audio.fade_out_seconds.max(0.0) * f64::from(project.sample_rate)).round() as usize;

    let pan_gain = 1.0 - (audio.pan.abs() * 0.2);
    let clip_gain = db_to_gain(audio.gain_db) * pan_gain;

    for frame_index in 0..output_frames {
        let ratio = if output_frames > 1 {
            frame_index as f64 / (output_frames - 1) as f64
        } else {
            0.0
        };
        let source_offset = ratio * source_frames.saturating_sub(1) as f64;
        let source_index = if audio.reverse {
            source_end.saturating_sub(1) as f64 - source_offset
        } else {
            source_start as f64 + source_offset
        };

        let source_sample = sample_linear(source_samples, source_index);
        let envelope = fade_envelope(frame_index, output_frames, fade_in_frames, fade_out_frames);
        buffer[start_frame + frame_index] += source_sample * clip_gain * envelope;
    }
}

fn sample_linear(samples: &[f32], index: f64) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let max_index = samples.len().saturating_sub(1) as f64;
    let left = index.floor().clamp(0.0, max_index) as usize;
    let right = (left + 1).min(samples.len().saturating_sub(1));
    let frac = (index - left as f64).clamp(0.0, 1.0) as f32;
    let left_sample = samples[left];
    let right_sample = samples[right];
    left_sample + ((right_sample - left_sample) * frac)
}

fn fade_envelope(
    frame_index: usize,
    total_frames: usize,
    fade_in_frames: usize,
    fade_out_frames: usize,
) -> f32 {
    let mut gain = 1.0_f32;

    if fade_in_frames > 0 {
        gain *= (frame_index as f32 / fade_in_frames as f32).clamp(0.0, 1.0);
    }
    if fade_out_frames > 0 {
        let frames_to_end = total_frames.saturating_sub(frame_index + 1);
        gain *= (frames_to_end as f32 / fade_out_frames as f32).clamp(0.0, 1.0);
    }

    gain
}

fn db_to_gain(gain_db: f32) -> f32 {
    10.0_f32.powf(gain_db / 20.0)
}

fn collect_rendered_notes(project: &Project) -> Vec<RenderedNote> {
    let mut notes = Vec::new();

    for track in &project.tracks {
        if !track.enabled || track.mute || track.hidden {
            continue;
        }

        let waveform = match track.kind {
            TrackKind::Chip => Waveform::Square,
            TrackKind::Midi | TrackKind::Audio | TrackKind::Automation | TrackKind::Bus => {
                Waveform::Triangle
            }
        };

        for clip in &track.clips {
            if clip.disabled {
                continue;
            }

            match &clip.payload {
                ClipPayload::Midi(midi) => {
                    for note in &midi.notes {
                        push_rendered_note(&mut notes, note, clip.start_tick, project, waveform);
                    }
                }
                ClipPayload::Pattern(pattern) => {
                    for note in &pattern.notes {
                        let macro_note = apply_pattern_macros(note, pattern, project.ppq);
                        push_rendered_note(
                            &mut notes,
                            &macro_note,
                            clip.start_tick,
                            project,
                            waveform,
                        );
                    }
                }
                ClipPayload::Audio(_) | ClipPayload::Automation(_) => {}
            }
        }
    }

    notes
}

fn push_rendered_note(
    rendered_notes: &mut Vec<RenderedNote>,
    note: &MidiNote,
    clip_start_tick: u64,
    project: &Project,
    waveform: Waveform,
) {
    let phase_increment =
        frequency_to_phase_increment(note_frequency_hz(note.pitch), project.sample_rate);
    let clip_note_start = clip_start_tick.saturating_add(note.start_tick);
    let clip_note_end = clip_start_tick.saturating_add(note.end_tick());
    let start_sample = ticks_to_samples(
        clip_note_start,
        project.bpm,
        project.ppq,
        project.sample_rate,
    ) as usize;
    let end_sample =
        ticks_to_samples(clip_note_end, project.bpm, project.ppq, project.sample_rate) as usize;

    if end_sample <= start_sample {
        warn!(
            note_pitch = note.pitch,
            start_sample, end_sample, "skipping zero-length note in renderer"
        );
        return;
    }

    rendered_notes.push(RenderedNote {
        start_sample,
        end_sample,
        amplitude: (f32::from(note.velocity.min(127)) / 127.0) * 0.18,
        phase_increment,
        waveform,
    });
}

fn apply_pattern_macros(note: &MidiNote, pattern: &PatternClip, ppq: u16) -> MidiNote {
    let mut output = note.clone();

    if let Some(arpeggio) = macro_lane(pattern, "arpeggio")
        && let Some(offset) =
            macro_value_for_note(arpeggio, note.start_tick, pattern.lines_per_beat, ppq)
    {
        let pitch = i16::from(output.pitch).saturating_add(offset).clamp(0, 127);
        output.pitch = pitch as u8;
    }

    if let Some(env) = macro_lane(pattern, "env")
        && let Some(delta) = macro_value_for_note(env, note.start_tick, pattern.lines_per_beat, ppq)
    {
        let velocity = i16::from(output.velocity)
            .saturating_add(delta)
            .clamp(1, 127);
        output.velocity = velocity as u8;
    }

    output
}

fn macro_lane<'a>(pattern: &'a PatternClip, name: &str) -> Option<&'a ChipMacroLane> {
    pattern.macros.iter().find(|lane| {
        lane.enabled && lane.target.eq_ignore_ascii_case(name) && !lane.values.is_empty()
    })
}

fn macro_value_for_note(
    lane: &ChipMacroLane,
    note_start_tick: u64,
    lines_per_beat: u16,
    ppq: u16,
) -> Option<i16> {
    if lane.values.is_empty() || lines_per_beat == 0 {
        return None;
    }

    let ticks_per_row = (u64::from(ppq) / u64::from(lines_per_beat)).max(1);
    let step = (note_start_tick / ticks_per_row) as usize;
    Some(macro_value_at_step(lane, step))
}

fn macro_value_at_step(lane: &ChipMacroLane, step: usize) -> i16 {
    if lane.values.is_empty() {
        return 0;
    }

    if let (Some(loop_start), Some(loop_end)) = (lane.loop_start, lane.loop_end)
        && loop_start <= loop_end
        && loop_end < lane.values.len()
    {
        if step <= loop_end {
            return lane.values[step.min(lane.values.len() - 1)];
        }
        let loop_len = loop_end.saturating_sub(loop_start) + 1;
        let loop_step = loop_start + ((step - loop_start) % loop_len);
        return lane.values[loop_step.min(lane.values.len() - 1)];
    }

    lane.values[step.min(lane.values.len() - 1)]
}

fn sanitize_stem_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut previous_underscore = false;
    for ch in name.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch.is_ascii_whitespace() || ch == '_' || ch == '-' {
            Some('_')
        } else {
            None
        };

        if let Some(ch) = normalized {
            if ch == '_' {
                if previous_underscore {
                    continue;
                }
                previous_underscore = true;
            } else {
                previous_underscore = false;
            }
            out.push(ch);
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "track".to_string()
    } else {
        trimmed.to_string()
    }
}

fn frequency_to_phase_increment(frequency_hz: f64, sample_rate: u32) -> u32 {
    let normalized = frequency_hz / f64::from(sample_rate.max(1));
    let increment = normalized * f64::from(u32::MAX);
    increment.clamp(1.0, f64::from(u32::MAX)) as u32
}

fn note_frequency_hz(pitch: u8) -> f64 {
    let semitone_offset = f64::from(i16::from(pitch) - 69);
    440.0 * 2_f64.powf(semitone_offset / 12.0)
}

fn square_osc(phase: u32) -> f32 {
    if phase < 0x8000_0000 { 1.0 } else { -1.0 }
}

fn triangle_osc(phase: u32) -> f32 {
    let phase_unit = phase as f32 / u32::MAX as f32;
    if phase_unit < 0.5 {
        (phase_unit * 4.0) - 1.0
    } else {
        3.0 - (phase_unit * 4.0)
    }
}
