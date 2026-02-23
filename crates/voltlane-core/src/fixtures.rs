use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::{
    Clip, ClipPayload, DEFAULT_SAMPLE_RATE, MidiClip, MidiNote, PatternClip, Project, Track,
    TrackKind,
};

#[must_use]
pub fn demo_project() -> Project {
    let mut project = Project::new("Voltlane Demo", 138.0, DEFAULT_SAMPLE_RATE);
    project.id = Uuid::parse_str("9ed0a3fa-4064-458f-b95f-1fdd0bc4f0be")
        .expect("fixture project id should be valid");
    project.session_id = Uuid::parse_str("11eb0ce5-cdb7-4f30-bc14-53a3a1e10de3")
        .expect("fixture session id should be valid");
    let fixed_timestamp = DateTime::parse_from_rfc3339("2026-02-23T00:00:00Z")
        .expect("fixture timestamp should be valid")
        .with_timezone(&Utc);
    project.created_at = fixed_timestamp;
    project.updated_at = fixed_timestamp;

    let mut lead_track = Track::new("Lead", "#00d1b2", TrackKind::Midi);
    lead_track.id = Uuid::parse_str("a959fd97-0e35-445d-a7e8-fe6d81d49235")
        .expect("fixture lead track id should be valid");
    lead_track.clips.push(Clip {
        id: Uuid::parse_str("fbf41a8f-c5b4-464b-a9f3-6e62eebf6efb")
            .expect("fixture lead clip id should be valid"),
        name: "Lead phrase".to_string(),
        start_tick: 0,
        length_ticks: 1_920,
        disabled: false,
        payload: ClipPayload::Midi(MidiClip {
            instrument: Some("Pulse Lead".to_string()),
            notes: vec![
                MidiNote {
                    pitch: 72,
                    velocity: 118,
                    start_tick: 0,
                    length_ticks: 240,
                    channel: 0,
                },
                MidiNote {
                    pitch: 74,
                    velocity: 118,
                    start_tick: 240,
                    length_ticks: 240,
                    channel: 0,
                },
                MidiNote {
                    pitch: 79,
                    velocity: 110,
                    start_tick: 480,
                    length_ticks: 720,
                    channel: 0,
                },
                MidiNote {
                    pitch: 81,
                    velocity: 104,
                    start_tick: 1_200,
                    length_ticks: 720,
                    channel: 0,
                },
            ],
        }),
    });

    let mut chip_track = Track::new("Chip Bass", "#f77f00", TrackKind::Chip);
    chip_track.id = Uuid::parse_str("2695613e-3bef-4f17-b44d-c8e753f2268e")
        .expect("fixture chip track id should be valid");
    chip_track.clips.push(Clip {
        id: Uuid::parse_str("0caa5e8d-6ec2-4b74-9e87-d7f60111f3f2")
            .expect("fixture chip clip id should be valid"),
        name: "Bassline".to_string(),
        start_tick: 0,
        length_ticks: 1_920,
        disabled: false,
        payload: ClipPayload::Pattern(PatternClip {
            source_chip: "gameboy_apu".to_string(),
            notes: vec![
                MidiNote {
                    pitch: 36,
                    velocity: 100,
                    start_tick: 0,
                    length_ticks: 480,
                    channel: 1,
                },
                MidiNote {
                    pitch: 43,
                    velocity: 95,
                    start_tick: 480,
                    length_ticks: 480,
                    channel: 1,
                },
                MidiNote {
                    pitch: 41,
                    velocity: 95,
                    start_tick: 960,
                    length_ticks: 480,
                    channel: 1,
                },
                MidiNote {
                    pitch: 38,
                    velocity: 98,
                    start_tick: 1_440,
                    length_ticks: 480,
                    channel: 1,
                },
            ],
        }),
    });

    project.tracks.extend([lead_track, chip_track]);
    project
}
