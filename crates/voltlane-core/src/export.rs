use std::{
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
    model::{ClipPayload, MidiNote, Project, TrackKind},
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

    let mut notes = collect_rendered_notes(project);
    let mut buffer = vec![0.0_f32; frame_count];
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
        "audio render completed"
    );
    buffer
}

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display()))]
pub fn export_wav(project: &Project, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create wav output directory: {}",
                parent.display()
            )
        })?;
    }

    let rendered = render_project_samples(project, 1.0);
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

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display()))]
pub fn export_mp3(project: &Project, path: &Path, ffmpeg_binary: Option<&Path>) -> Result<()> {
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
    export_wav(project, &temp_wav)?;

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
                        absolute_events.extend(note_to_midi_events(note, clip.start_tick));
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

            let iter: Box<dyn Iterator<Item = &MidiNote>> = match &clip.payload {
                ClipPayload::Midi(midi) => Box::new(midi.notes.iter()),
                ClipPayload::Pattern(pattern) => Box::new(pattern.notes.iter()),
                ClipPayload::Audio(_) | ClipPayload::Automation(_) => Box::new([].iter()),
            };

            for note in iter {
                let phase_increment = frequency_to_phase_increment(
                    note_frequency_hz(note.pitch),
                    project.sample_rate,
                );
                let clip_note_start = clip.start_tick.saturating_add(note.start_tick);
                let clip_note_end = clip.start_tick.saturating_add(note.end_tick());
                let start_sample = ticks_to_samples(
                    clip_note_start,
                    project.bpm,
                    project.ppq,
                    project.sample_rate,
                ) as usize;
                let end_sample =
                    ticks_to_samples(clip_note_end, project.bpm, project.ppq, project.sample_rate)
                        as usize;

                if end_sample <= start_sample {
                    warn!(
                        note_pitch = note.pitch,
                        start_sample, end_sample, "skipping zero-length note in renderer"
                    );
                    continue;
                }

                notes.push(RenderedNote {
                    start_sample,
                    end_sample,
                    amplitude: (f32::from(note.velocity.min(127)) / 127.0) * 0.18,
                    phase_increment,
                    waveform,
                });
            }
        }
    }

    notes
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
