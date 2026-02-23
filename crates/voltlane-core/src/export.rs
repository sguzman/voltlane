use std::{
    collections::{HashMap, VecDeque},
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
use uuid::Uuid;

use crate::{
    assets::{DecodedAudio, decode_audio_file_mono},
    engine::RenderMode,
    model::{
        AudioClip, ChipMacroLane, ClipPayload, EffectSpec, MidiNote, PatternClip, Project, Track,
        TrackKind,
    },
    time::ticks_to_samples,
};

#[derive(Debug, Clone)]
struct SynthEvent {
    start_sample: usize,
    end_sample: usize,
    amplitude: f32,
    phase_increment: u32,
    attack_frames: usize,
    release_frames: usize,
    waveform: Waveform,
    color: VoiceColor,
}

#[derive(Debug, Clone, Copy)]
enum Waveform {
    Triangle,
    Pulse { duty_cycle: f32 },
    Noise { seed: u32 },
}

#[derive(Debug, Clone, Copy)]
enum VoiceColor {
    Clean,
    GameBoyApu,
    NesApu,
    Sn76489,
}

#[derive(Debug, Clone, Copy)]
enum ChipBackend {
    GameBoyApu,
    NesApu,
    Sn76489,
    Generic,
}

#[derive(Debug, Clone)]
struct AbsoluteMidiEvent {
    tick: u64,
    order: u8,
    kind: TrackEventKind<'static>,
}

#[derive(Debug, Default)]
struct RenderStats {
    rendered_notes: usize,
    rendered_audio_clips: usize,
    routed_tracks: usize,
    processed_effect_instances: usize,
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

    let mut stats = RenderStats::default();
    let track_sources = render_track_source_buffers(project, frame_count, &mut stats);
    let track_order = track_topological_order(project);
    let mut master = vec![0.0_f32; frame_count];
    let mut pending_bus_input: HashMap<Uuid, Vec<f32>> = HashMap::new();

    for track_id in track_order {
        let Some(track) = project
            .tracks
            .iter()
            .find(|candidate| candidate.id == track_id)
        else {
            continue;
        };
        if !track.enabled || track.mute || track.hidden {
            continue;
        }

        let mut working = vec![0.0_f32; frame_count];
        if let Some(source) = track_sources.get(&track.id) {
            add_buffer_in_place(&mut working, source);
        }
        if let Some(incoming) = pending_bus_input.remove(&track.id) {
            add_buffer_in_place(&mut working, &incoming);
        }

        stats.processed_effect_instances +=
            apply_track_effect_chain(track, &mut working, project.sample_rate);

        let mut post_fader = working.clone();
        let track_gain = db_to_gain(track.gain_db) * pan_to_mono_gain(track.pan);
        scale_buffer_in_place(&mut post_fader, track_gain);

        route_buffer(
            &post_fader,
            track.output_bus,
            1.0,
            &mut pending_bus_input,
            &mut master,
        );

        for send in track.sends.iter().filter(|send| send.enabled) {
            let send_source = if send.pre_fader {
                &working
            } else {
                &post_fader
            };
            let send_gain = db_to_gain(send.level_db) * pan_to_mono_gain(send.pan);
            route_buffer(
                send_source,
                Some(send.target_bus),
                send_gain,
                &mut pending_bus_input,
                &mut master,
            );
        }

        stats.routed_tracks += 1;
    }

    for (bus_id, bus_signal) in pending_bus_input {
        warn!(track_id = %bus_id, "bus signal left unrouted; adding to master as fallback");
        add_buffer_scaled_in_place(&mut master, &bus_signal, 1.0);
    }

    for frame in &mut master {
        *frame = frame.clamp(-1.0, 1.0);
    }

    debug!(
        frames = master.len(),
        rendered_notes = stats.rendered_notes,
        rendered_audio_clips = stats.rendered_audio_clips,
        routed_tracks = stats.routed_tracks,
        processed_effect_instances = stats.processed_effect_instances,
        "audio render completed"
    );
    master
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

fn render_track_source_buffers(
    project: &Project,
    frame_count: usize,
    stats: &mut RenderStats,
) -> HashMap<Uuid, Vec<f32>> {
    let mut decoded_cache: HashMap<String, DecodedAudio> = HashMap::new();
    let mut buffers = HashMap::new();

    for track in &project.tracks {
        if !track.enabled || track.mute || track.hidden {
            continue;
        }

        let mut track_buffer = vec![0.0_f32; frame_count];
        for clip in &track.clips {
            if clip.disabled {
                continue;
            }

            match &clip.payload {
                ClipPayload::Audio(audio_clip) => {
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
                        &mut track_buffer,
                    );
                    stats.rendered_audio_clips += 1;
                }
                ClipPayload::Midi(midi_clip) => {
                    let waveform = if matches!(track.kind, TrackKind::Chip) {
                        Waveform::Pulse { duty_cycle: 0.5 }
                    } else {
                        Waveform::Triangle
                    };
                    let color = if matches!(track.kind, TrackKind::Chip) {
                        VoiceColor::Sn76489
                    } else {
                        VoiceColor::Clean
                    };
                    for note in &midi_clip.notes {
                        let event =
                            synth_event_for_note(note, clip.start_tick, project, waveform, color);
                        render_synth_event(&event, &mut track_buffer);
                        stats.rendered_notes += 1;
                    }
                }
                ClipPayload::Pattern(pattern_clip) => {
                    let backend = chip_backend_for_source(&pattern_clip.source_chip);
                    render_pattern_clip(
                        pattern_clip,
                        backend,
                        clip.start_tick,
                        project,
                        &mut track_buffer,
                        stats,
                    );
                }
                ClipPayload::Automation(_) => {}
            }
        }

        if track_buffer
            .iter()
            .any(|sample| sample.abs() > f32::EPSILON)
        {
            buffers.insert(track.id, track_buffer);
        }
    }

    buffers
}

fn render_pattern_clip(
    pattern: &PatternClip,
    backend: ChipBackend,
    clip_start_tick: u64,
    project: &Project,
    buffer: &mut [f32],
    stats: &mut RenderStats,
) {
    for note in &pattern.notes {
        let macro_note = apply_pattern_macros(note, pattern, project.ppq);
        let duty_cycle = duty_cycle_for_note(pattern, note.start_tick, project.ppq)
            .map(|value| chip_backend_duty_cycle(backend, value))
            .unwrap_or_else(|| chip_backend_default_duty(backend));
        let waveform = chip_waveform_for_note(pattern, backend, note, project.ppq, duty_cycle);
        let color = chip_backend_color(backend);
        let mut event =
            synth_event_for_note(&macro_note, clip_start_tick, project, waveform, color);
        event.amplitude *= chip_backend_level(backend);
        event.attack_frames = 8;
        event.release_frames = 64;
        render_synth_event(&event, buffer);
        stats.rendered_notes += 1;
    }
}

fn synth_event_for_note(
    note: &MidiNote,
    clip_start_tick: u64,
    project: &Project,
    waveform: Waveform,
    color: VoiceColor,
) -> SynthEvent {
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
    let (start_sample, end_sample) = if end_sample <= start_sample {
        (start_sample, start_sample.saturating_add(1))
    } else {
        (start_sample, end_sample)
    };

    SynthEvent {
        start_sample,
        end_sample,
        amplitude: (f32::from(note.velocity.min(127)) / 127.0) * 0.18,
        phase_increment,
        attack_frames: 24,
        release_frames: 72,
        waveform,
        color,
    }
}

fn render_synth_event(event: &SynthEvent, buffer: &mut [f32]) {
    let start = event.start_sample.min(buffer.len());
    let end = event.end_sample.min(buffer.len());
    if end <= start {
        return;
    }

    let total = end.saturating_sub(start);
    let attack_frames = event.attack_frames.min(total.saturating_sub(1));
    let release_frames = event.release_frames.min(total.saturating_sub(1));
    let mut phase = 0_u32;
    let mut noise_state = match event.waveform {
        Waveform::Noise { seed } => seed.max(1),
        Waveform::Triangle | Waveform::Pulse { .. } => 0x1ACE_B00C,
    };
    let mut noise_phase = 0_u32;

    for (index, frame) in buffer[start..end].iter_mut().enumerate() {
        let attack_env = if attack_frames == 0 {
            1.0
        } else {
            (index as f32 / attack_frames as f32).clamp(0.0, 1.0)
        };
        let remaining = total.saturating_sub(index + 1);
        let release_env = if release_frames == 0 {
            1.0
        } else {
            (remaining as f32 / release_frames as f32).clamp(0.0, 1.0)
        };
        let envelope = attack_env * release_env;

        let raw = match event.waveform {
            Waveform::Triangle => triangle_osc(phase),
            Waveform::Pulse { duty_cycle } => pulse_osc(phase, duty_cycle),
            Waveform::Noise { .. } => {
                noise_phase = noise_phase.wrapping_add(event.phase_increment);
                if noise_phase & 0xF000_0000 != 0 {
                    noise_state = lfsr_step(noise_state);
                    noise_phase &= 0x0FFF_FFFF;
                }
                if noise_state & 1 == 0 { 1.0 } else { -1.0 }
            }
        };

        let colored = color_sample(raw, event.color);
        *frame += colored * event.amplitude * envelope;
        phase = phase.wrapping_add(event.phase_increment);
    }
}

fn route_buffer(
    signal: &[f32],
    target_bus: Option<Uuid>,
    gain: f32,
    pending_bus_input: &mut HashMap<Uuid, Vec<f32>>,
    master: &mut [f32],
) {
    if signal.is_empty() || gain.abs() <= f32::EPSILON {
        return;
    }

    if let Some(bus_id) = target_bus {
        let entry = pending_bus_input
            .entry(bus_id)
            .or_insert_with(|| vec![0.0; signal.len()]);
        add_buffer_scaled_in_place(entry, signal, gain);
    } else {
        add_buffer_scaled_in_place(master, signal, gain);
    }
}

fn track_topological_order(project: &Project) -> Vec<Uuid> {
    let mut indegree = HashMap::<Uuid, usize>::new();
    let mut adjacency = HashMap::<Uuid, Vec<Uuid>>::new();

    for track in &project.tracks {
        indegree.insert(track.id, 0);
        adjacency.entry(track.id).or_default();
    }

    for track in &project.tracks {
        if let Some(target) = track.output_bus
            && indegree.contains_key(&target)
        {
            adjacency.entry(track.id).or_default().push(target);
            if let Some(value) = indegree.get_mut(&target) {
                *value += 1;
            }
        }
        for send in track.sends.iter().filter(|send| send.enabled) {
            if indegree.contains_key(&send.target_bus) {
                adjacency.entry(track.id).or_default().push(send.target_bus);
                if let Some(value) = indegree.get_mut(&send.target_bus) {
                    *value += 1;
                }
            }
        }
    }

    let mut queue = VecDeque::new();
    for track in &project.tracks {
        if indegree.get(&track.id).copied().unwrap_or_default() == 0 {
            queue.push_back(track.id);
        }
    }

    let mut order = Vec::with_capacity(project.tracks.len());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        if let Some(neighbors) = adjacency.get(&node) {
            for neighbor in neighbors {
                if let Some(value) = indegree.get_mut(neighbor) {
                    *value = value.saturating_sub(1);
                    if *value == 0 {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
    }

    if order.len() != project.tracks.len() {
        warn!("routing graph did not topologically sort cleanly; using project order fallback");
        for track in &project.tracks {
            if !order.contains(&track.id) {
                order.push(track.id);
            }
        }
    }

    order
}

fn apply_track_effect_chain(track: &Track, buffer: &mut [f32], sample_rate: u32) -> usize {
    let mut processed = 0_usize;
    for effect in track.effects.iter().filter(|effect| effect.enabled) {
        apply_effect(effect, buffer, sample_rate);
        processed += 1;
    }
    processed
}

fn apply_effect(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let effect_name = effect.name.trim().to_ascii_lowercase();
    match effect_name.as_str() {
        "eq" => apply_eq(effect, buffer, sample_rate),
        "comp" | "compressor" => apply_compressor(effect, buffer, sample_rate),
        "reverb" => apply_reverb(effect, buffer, sample_rate),
        "delay" => apply_delay(effect, buffer, sample_rate),
        "limiter" => apply_limiter(effect, buffer, sample_rate),
        "bitcrusher" => apply_bitcrusher(effect, buffer),
        _ => debug!(effect = %effect.name, "effect name has no built-in renderer, skipping"),
    }
}

fn apply_eq(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let low_gain = db_to_gain(effect_param(effect, "low_gain_db", 0.0));
    let mid_gain = db_to_gain(effect_param(effect, "mid_gain_db", 0.0));
    let high_gain = db_to_gain(effect_param(effect, "high_gain_db", 0.0));
    let low_freq = effect_param(effect, "low_freq_hz", 120.0).clamp(20.0, 2_000.0);
    let high_freq = effect_param(effect, "high_freq_hz", 8_000.0).clamp(400.0, 20_000.0);
    let low_alpha = one_pole_alpha(low_freq, sample_rate);
    let high_alpha = one_pole_alpha(high_freq, sample_rate);
    let mut low_state = 0.0_f32;
    let mut high_lp_state = 0.0_f32;

    for sample in buffer {
        low_state += low_alpha * (*sample - low_state);
        high_lp_state += high_alpha * (*sample - high_lp_state);
        let low = low_state;
        let high = *sample - high_lp_state;
        let mid = *sample - low - high;
        *sample = (low * low_gain) + (mid * mid_gain) + (high * high_gain);
    }
}

fn apply_compressor(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let threshold_db = effect_param(effect, "threshold_db", -18.0).clamp(-60.0, 0.0);
    let ratio = effect_param(effect, "ratio", 4.0).clamp(1.0, 24.0);
    let attack_ms = effect_param(effect, "attack_ms", 10.0).clamp(0.1, 250.0);
    let release_ms = effect_param(effect, "release_ms", 120.0).clamp(1.0, 1_500.0);
    let makeup_gain = db_to_gain(effect_param(effect, "makeup_db", 0.0).clamp(-24.0, 24.0));

    let attack_coeff = exp_smoothing_coeff(attack_ms / 1_000.0, sample_rate);
    let release_coeff = exp_smoothing_coeff(release_ms / 1_000.0, sample_rate);
    let mut envelope = 0.0_f32;

    for sample in buffer {
        let level = sample.abs().max(1e-6);
        if level > envelope {
            envelope = (attack_coeff * envelope) + ((1.0 - attack_coeff) * level);
        } else {
            envelope = (release_coeff * envelope) + ((1.0 - release_coeff) * level);
        }

        let envelope_db = linear_to_db(envelope);
        let gain_reduction_db = if envelope_db > threshold_db {
            let compressed_db = threshold_db + ((envelope_db - threshold_db) / ratio);
            compressed_db - envelope_db
        } else {
            0.0
        };
        let gain = db_to_gain(gain_reduction_db) * makeup_gain;
        *sample *= gain;
    }
}

fn apply_delay(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let mix = effect_param(effect, "mix", 0.25).clamp(0.0, 1.0);
    let time_ms = effect_param(effect, "time_ms", 320.0).clamp(1.0, 2_000.0);
    let feedback = effect_param(effect, "feedback", 0.38).clamp(0.0, 0.95);
    let hi_cut_hz = effect_param(effect, "hi_cut_hz", 6_500.0).clamp(800.0, 20_000.0);
    let delay_samples = ((time_ms / 1_000.0) * sample_rate as f32).round() as usize;
    let delay_samples = delay_samples.max(1);
    let mut line = vec![0.0_f32; delay_samples];
    let mut cursor = 0_usize;
    let alpha = one_pole_alpha(hi_cut_hz, sample_rate);
    let mut filtered_feedback = 0.0_f32;

    for sample in buffer {
        let delayed = line[cursor];
        filtered_feedback += alpha * (delayed - filtered_feedback);
        line[cursor] = *sample + (filtered_feedback * feedback);
        *sample = (*sample * (1.0 - mix)) + (delayed * mix);
        cursor += 1;
        if cursor >= line.len() {
            cursor = 0;
        }
    }
}

fn apply_reverb(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let mix = effect_param(effect, "mix", 0.18).clamp(0.0, 1.0);
    let room_size = effect_param(effect, "room_size", 0.62).clamp(0.0, 1.0);
    let damping = effect_param(effect, "damping", 0.45).clamp(0.0, 0.98);
    let width = effect_param(effect, "width", 0.85).clamp(0.0, 1.0);
    let feedback = 0.35 + (room_size * 0.5);
    let base = ((sample_rate as f32 * 0.015) + (sample_rate as f32 * 0.03 * room_size)) as usize;
    let line_len_a = base.max(1);
    let line_len_b = ((line_len_a as f32 * 1.37).round() as usize).max(1);
    let line_len_c = ((line_len_a as f32 * 1.91).round() as usize).max(1);
    let mut line_a = vec![0.0_f32; line_len_a];
    let mut line_b = vec![0.0_f32; line_len_b];
    let mut line_c = vec![0.0_f32; line_len_c];
    let mut cursor_a = 0_usize;
    let mut cursor_b = 0_usize;
    let mut cursor_c = 0_usize;
    let damping_hz = ((1.0 - damping) * 8_000.0) + 1_000.0;
    let alpha = one_pole_alpha(damping_hz, sample_rate);
    let mut damp_a = 0.0_f32;
    let mut damp_b = 0.0_f32;
    let mut damp_c = 0.0_f32;
    let wet_gain = 0.7 + (0.3 * width);

    for sample in buffer {
        let tap_a = line_a[cursor_a];
        let tap_b = line_b[cursor_b];
        let tap_c = line_c[cursor_c];

        damp_a += alpha * (tap_a - damp_a);
        damp_b += alpha * (tap_b - damp_b);
        damp_c += alpha * (tap_c - damp_c);

        line_a[cursor_a] = *sample + (damp_c * feedback);
        line_b[cursor_b] = *sample + (damp_a * feedback);
        line_c[cursor_c] = *sample + (damp_b * feedback);

        let wet = ((damp_a + damp_b + damp_c) / 3.0) * wet_gain;
        *sample = (*sample * (1.0 - mix)) + (wet * mix);

        cursor_a = (cursor_a + 1) % line_a.len();
        cursor_b = (cursor_b + 1) % line_b.len();
        cursor_c = (cursor_c + 1) % line_c.len();
    }
}

fn apply_limiter(effect: &EffectSpec, buffer: &mut [f32], sample_rate: u32) {
    let ceiling_db = effect_param(effect, "ceiling_db", -0.8).clamp(-12.0, 0.0);
    let ceiling = db_to_gain(ceiling_db);
    let release_ms = effect_param(effect, "release_ms", 80.0).clamp(1.0, 500.0);
    let release_coeff = exp_smoothing_coeff(release_ms / 1_000.0, sample_rate);
    let mut gain = 1.0_f32;

    for sample in buffer {
        let amplitude = sample.abs().max(1e-6);
        let needed = if amplitude * gain > ceiling {
            ceiling / amplitude
        } else {
            1.0
        };

        if needed < gain {
            gain = needed;
        } else {
            gain = (release_coeff * gain) + ((1.0 - release_coeff) * 1.0);
        }
        *sample *= gain;
        *sample = sample.clamp(-ceiling, ceiling);
    }
}

fn apply_bitcrusher(effect: &EffectSpec, buffer: &mut [f32]) {
    let bits = effect_param(effect, "bits", 8.0).round().clamp(2.0, 16.0) as u32;
    let downsample = effect_param(effect, "downsample", 2.0)
        .round()
        .clamp(1.0, 32.0) as usize;
    let levels = (1_u32 << bits) as f32;
    let step = 2.0 / (levels - 1.0);
    let mut held = 0.0_f32;
    let mut hold_counter = 0_usize;

    for sample in buffer {
        if hold_counter == 0 {
            held = (((sample.clamp(-1.0, 1.0) + 1.0) / step).round() * step) - 1.0;
        }
        *sample = held;
        hold_counter += 1;
        if hold_counter >= downsample {
            hold_counter = 0;
        }
    }
}

fn effect_param(effect: &EffectSpec, key: &str, default: f32) -> f32 {
    effect.params.get(key).copied().unwrap_or(default)
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

    let pan_gain = pan_to_mono_gain(audio.pan);
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

fn add_buffer_in_place(target: &mut [f32], source: &[f32]) {
    for (dest, value) in target.iter_mut().zip(source.iter().copied()) {
        *dest += value;
    }
}

fn add_buffer_scaled_in_place(target: &mut [f32], source: &[f32], gain: f32) {
    for (dest, value) in target.iter_mut().zip(source.iter().copied()) {
        *dest += value * gain;
    }
}

fn scale_buffer_in_place(buffer: &mut [f32], gain: f32) {
    for sample in buffer {
        *sample *= gain;
    }
}

fn db_to_gain(gain_db: f32) -> f32 {
    10.0_f32.powf(gain_db / 20.0)
}

fn linear_to_db(value: f32) -> f32 {
    20.0 * value.max(1e-6).log10()
}

fn pan_to_mono_gain(pan: f32) -> f32 {
    1.0 - (pan.abs().clamp(0.0, 1.0) * 0.2)
}

fn one_pole_alpha(cutoff_hz: f32, sample_rate: u32) -> f32 {
    let cutoff = cutoff_hz.max(10.0);
    let dt = 1.0 / sample_rate.max(1) as f32;
    let rc = 1.0 / (std::f32::consts::TAU * cutoff);
    (dt / (rc + dt)).clamp(0.0, 1.0)
}

fn exp_smoothing_coeff(time_seconds: f32, sample_rate: u32) -> f32 {
    let tau = time_seconds.max(1e-4);
    (-1.0 / (tau * sample_rate.max(1) as f32))
        .exp()
        .clamp(0.0, 1.0)
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

fn duty_cycle_for_note(pattern: &PatternClip, note_start_tick: u64, ppq: u16) -> Option<i16> {
    macro_lane(pattern, "duty")
        .and_then(|lane| macro_value_for_note(lane, note_start_tick, pattern.lines_per_beat, ppq))
}

fn chip_backend_for_source(source_chip: &str) -> ChipBackend {
    let normalized = source_chip.trim().to_ascii_lowercase();
    if normalized.contains("gameboy") || normalized.contains("gb_apu") {
        ChipBackend::GameBoyApu
    } else if normalized.contains("nes")
        || normalized.contains("2a03")
        || normalized.contains("vrc6")
    {
        ChipBackend::NesApu
    } else if normalized.contains("sn76489")
        || normalized.contains("psg")
        || normalized.contains("ay-3-8910")
    {
        ChipBackend::Sn76489
    } else {
        ChipBackend::Generic
    }
}

fn chip_backend_duty_cycle(backend: ChipBackend, value: i16) -> f32 {
    let value = value.clamp(-127, 127);
    match backend {
        ChipBackend::GameBoyApu | ChipBackend::NesApu => {
            let step = value.clamp(0, 3) as usize;
            [0.125, 0.25, 0.5, 0.75][step]
        }
        ChipBackend::Sn76489 | ChipBackend::Generic => {
            let normalized = (f32::from(value) + 127.0) / 254.0;
            (0.1 + (normalized * 0.8)).clamp(0.05, 0.95)
        }
    }
}

fn chip_backend_default_duty(backend: ChipBackend) -> f32 {
    match backend {
        ChipBackend::GameBoyApu => 0.5,
        ChipBackend::NesApu => 0.5,
        ChipBackend::Sn76489 => 0.5,
        ChipBackend::Generic => 0.5,
    }
}

fn chip_backend_color(backend: ChipBackend) -> VoiceColor {
    match backend {
        ChipBackend::GameBoyApu => VoiceColor::GameBoyApu,
        ChipBackend::NesApu => VoiceColor::NesApu,
        ChipBackend::Sn76489 => VoiceColor::Sn76489,
        ChipBackend::Generic => VoiceColor::Clean,
    }
}

fn chip_backend_level(backend: ChipBackend) -> f32 {
    match backend {
        ChipBackend::GameBoyApu => 0.95,
        ChipBackend::NesApu => 0.9,
        ChipBackend::Sn76489 => 0.92,
        ChipBackend::Generic => 1.0,
    }
}

fn chip_waveform_for_note(
    pattern: &PatternClip,
    backend: ChipBackend,
    note: &MidiNote,
    ppq: u16,
    duty_cycle: f32,
) -> Waveform {
    let normalized = pattern.source_chip.trim().to_ascii_lowercase();
    if normalized.contains("noise")
        || macro_lane(pattern, "noise")
            .and_then(|lane| {
                macro_value_for_note(lane, note.start_tick, pattern.lines_per_beat, ppq)
            })
            .unwrap_or_default()
            > 0
    {
        let seed = 0xBEEF_u32
            .wrapping_mul(u32::from(note.pitch).saturating_add(1))
            .wrapping_add(note.start_tick as u32);
        return Waveform::Noise { seed };
    }

    if matches!(backend, ChipBackend::NesApu) && normalized.contains("triangle") {
        return Waveform::Triangle;
    }

    Waveform::Pulse { duty_cycle }
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

fn pulse_osc(phase: u32, duty_cycle: f32) -> f32 {
    let threshold = (duty_cycle.clamp(0.01, 0.99) * u32::MAX as f32) as u32;
    if phase < threshold { 1.0 } else { -1.0 }
}

fn triangle_osc(phase: u32) -> f32 {
    let phase_unit = phase as f32 / u32::MAX as f32;
    if phase_unit < 0.5 {
        (phase_unit * 4.0) - 1.0
    } else {
        3.0 - (phase_unit * 4.0)
    }
}

fn color_sample(sample: f32, color: VoiceColor) -> f32 {
    match color {
        VoiceColor::Clean => sample,
        VoiceColor::GameBoyApu => {
            let quantized = (sample * 15.0).round() / 15.0;
            quantized * 0.95
        }
        VoiceColor::NesApu => (sample * 1.15).tanh(),
        VoiceColor::Sn76489 => (sample * 7.0).round() / 7.0,
    }
}

fn lfsr_step(state: u32) -> u32 {
    let bit = ((state >> 0) ^ (state >> 1)) & 1;
    (state >> 1) | (bit << 30)
}
