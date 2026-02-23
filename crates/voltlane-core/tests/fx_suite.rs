use voltlane_core::{
    export::render_project_samples,
    model::{
        Clip, ClipPayload, DEFAULT_SAMPLE_RATE, EffectSpec, MidiClip, MidiNote, Project, Track,
        TrackKind,
    },
};

fn midi_project_with_effects(effects: Vec<EffectSpec>, gain_db: f32) -> Project {
    let mut project = Project::new("FX Suite", 128.0, DEFAULT_SAMPLE_RATE);
    let mut track = Track::new("Lead", "#2ad9b8", TrackKind::Midi);
    track.gain_db = gain_db;
    track.effects = effects;
    track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "phrase".to_string(),
        start_tick: 0,
        length_ticks: 1_920,
        disabled: false,
        payload: ClipPayload::Midi(MidiClip {
            instrument: Some("Saw".to_string()),
            notes: vec![
                MidiNote {
                    pitch: 60,
                    velocity: 120,
                    start_tick: 0,
                    length_ticks: 960,
                    channel: 0,
                },
                MidiNote {
                    pitch: 67,
                    velocity: 120,
                    start_tick: 240,
                    length_ticks: 960,
                    channel: 0,
                },
                MidiNote {
                    pitch: 72,
                    velocity: 120,
                    start_tick: 480,
                    length_ticks: 960,
                    channel: 0,
                },
            ],
        }),
    });
    project.tracks.push(track);
    project
}

fn mean_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    let frames = a.len().min(b.len()).max(1);
    a.iter()
        .zip(b.iter())
        .take(frames)
        .map(|(left, right)| (left - right).abs())
        .sum::<f32>()
        / frames as f32
}

fn peak_amplitude(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max)
}

#[test]
fn built_in_effect_chain_changes_rendered_audio() {
    let dry = midi_project_with_effects(Vec::new(), 0.0);
    let wet = midi_project_with_effects(
        vec![
            EffectSpec::new("eq"),
            EffectSpec::new("compressor"),
            EffectSpec::new("delay"),
            EffectSpec::new("reverb"),
            EffectSpec::new("bitcrusher"),
        ],
        0.0,
    );

    let dry_samples = render_project_samples(&dry, 1.0);
    let wet_samples = render_project_samples(&wet, 1.0);
    let difference = mean_abs_diff(&dry_samples, &wet_samples);

    assert!(
        difference > 0.005,
        "effect chain should change rendered output compared to dry signal"
    );
}

#[test]
fn limiter_reduces_peak_amplitude_on_hot_signal() {
    let dry_hot = midi_project_with_effects(Vec::new(), 0.0);

    let mut limiter = EffectSpec::new("limiter");
    limiter.params.insert("ceiling_db".to_string(), -10.0);
    limiter.params.insert("release_ms".to_string(), 50.0);
    let limited_hot = midi_project_with_effects(vec![limiter], 0.0);

    let dry_samples = render_project_samples(&dry_hot, 1.0);
    let limited_samples = render_project_samples(&limited_hot, 1.0);
    let dry_peak = peak_amplitude(&dry_samples);
    let limited_peak = peak_amplitude(&limited_samples);

    assert!(
        limited_peak < dry_peak,
        "limiter should reduce peak amplitude on the insert signal path"
    );
    assert!(
        limited_peak <= 0.4,
        "limiter ceiling should keep peaks below a conservative threshold"
    );
}
