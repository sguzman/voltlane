use voltlane_core::{
    export::render_project_samples,
    model::{
        ChipMacroLane, Clip, ClipPayload, DEFAULT_SAMPLE_RATE, MidiNote, PatternClip, Project,
        Track, TrackKind,
    },
};

fn chip_project(source_chip: &str, macros: Vec<ChipMacroLane>) -> Project {
    let mut project = Project::new(format!("Chip {source_chip}"), 132.0, DEFAULT_SAMPLE_RATE);
    let mut track = Track::new("Chip", "#f57f20", TrackKind::Chip);
    track.clips.push(Clip {
        id: uuid::Uuid::new_v4(),
        name: "chip-pattern".to_string(),
        start_tick: 0,
        length_ticks: 1_920,
        disabled: false,
        payload: ClipPayload::Pattern(PatternClip {
            source_chip: source_chip.to_string(),
            notes: vec![
                MidiNote {
                    pitch: 48,
                    velocity: 112,
                    start_tick: 0,
                    length_ticks: 360,
                    channel: 0,
                },
                MidiNote {
                    pitch: 55,
                    velocity: 108,
                    start_tick: 360,
                    length_ticks: 360,
                    channel: 0,
                },
                MidiNote {
                    pitch: 60,
                    velocity: 106,
                    start_tick: 720,
                    length_ticks: 360,
                    channel: 0,
                },
            ],
            rows: Vec::new(),
            macros,
            lines_per_beat: 8,
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

#[test]
fn source_chip_backend_changes_render_signature() {
    let gb_project = chip_project(
        "gameboy_apu",
        vec![ChipMacroLane {
            target: "duty".to_string(),
            enabled: true,
            values: vec![0, 1, 2, 3],
            loop_start: Some(0),
            loop_end: Some(3),
        }],
    );
    let nes_project = chip_project(
        "nes_2a03_pulse",
        vec![ChipMacroLane {
            target: "duty".to_string(),
            enabled: true,
            values: vec![3, 2, 1, 0],
            loop_start: Some(0),
            loop_end: Some(3),
        }],
    );

    let gb = render_project_samples(&gb_project, 1.0);
    let nes = render_project_samples(&nes_project, 1.0);
    let difference = mean_abs_diff(&gb, &nes);

    assert!(
        difference > 0.005,
        "different chip backends should produce measurably different renders"
    );
}

#[test]
fn duty_macro_changes_chip_waveform_output() {
    let low_duty = chip_project(
        "gameboy_apu",
        vec![ChipMacroLane {
            target: "duty".to_string(),
            enabled: true,
            values: vec![0],
            loop_start: None,
            loop_end: None,
        }],
    );
    let high_duty = chip_project(
        "gameboy_apu",
        vec![ChipMacroLane {
            target: "duty".to_string(),
            enabled: true,
            values: vec![3],
            loop_start: None,
            loop_end: None,
        }],
    );

    let low = render_project_samples(&low_duty, 1.0);
    let high = render_project_samples(&high_duty, 1.0);
    let difference = mean_abs_diff(&low, &high);

    assert!(
        difference > 0.002,
        "duty macro variants should alter chip waveform output"
    );
}
