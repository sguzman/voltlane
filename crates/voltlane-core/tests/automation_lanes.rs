use voltlane_core::{
    AddTrackRequest, AutomationPoint, Engine,
    model::{ClipPayload, Project, TrackKind},
};

#[test]
fn automation_clip_add_and_upsert_sorts_points() {
    let mut engine = Engine::new(Project::new("Automation", 140.0, 48_000));
    let track = engine.add_track(AddTrackRequest {
        name: "Automation".to_string(),
        color: "#a07cff".to_string(),
        kind: TrackKind::Automation,
    });

    let clip = engine
        .add_automation_clip(
            track.id,
            "Volume lane".to_string(),
            0,
            1_920,
            String::new(),
            vec![
                AutomationPoint {
                    tick: 960,
                    value: 0.25,
                },
                AutomationPoint {
                    tick: 0,
                    value: 0.8,
                },
            ],
        )
        .expect("add automation clip should succeed");

    let automation = match clip.payload {
        ClipPayload::Automation(automation) => automation,
        _ => panic!("clip payload should be automation"),
    };
    assert!(
        automation.target_parameter_id.contains("gain_db"),
        "blank target should default to a gain parameter id"
    );
    assert_eq!(automation.points[0].tick, 0);
    assert_eq!(automation.points[1].tick, 960);

    let updated = engine
        .upsert_automation_clip(
            track.id,
            clip.id,
            Some(format!("track:{}:pan", track.id)),
            vec![
                AutomationPoint {
                    tick: 120,
                    value: 0.1,
                },
                AutomationPoint {
                    tick: 60,
                    value: f32::NAN,
                },
                AutomationPoint {
                    tick: 0,
                    value: 0.95,
                },
            ],
        )
        .expect("upsert automation clip should succeed");

    let automation = match updated.payload {
        ClipPayload::Automation(automation) => automation,
        _ => panic!("clip payload should be automation"),
    };
    assert_eq!(
        automation.target_parameter_id,
        format!("track:{}:pan", track.id)
    );
    assert_eq!(automation.points.len(), 2, "non-finite points are dropped");
    assert_eq!(automation.points[0].tick, 0);
    assert_eq!(automation.points[1].tick, 120);
}

#[test]
fn automation_clip_update_rejects_wrong_payload() {
    let mut engine = Engine::new(Project::new("Automation", 140.0, 48_000));
    let midi_track = engine.add_track(AddTrackRequest {
        name: "Midi".to_string(),
        color: "#22b7ff".to_string(),
        kind: TrackKind::Midi,
    });

    let clip = engine
        .add_clip(voltlane_core::AddClipRequest {
            track_id: midi_track.id,
            name: "Midi clip".to_string(),
            start_tick: 0,
            length_ticks: 480,
            payload: ClipPayload::Midi(voltlane_core::MidiClip {
                instrument: Some("Lead".to_string()),
                notes: vec![],
            }),
        })
        .expect("add clip should succeed");

    let err = engine
        .upsert_automation_clip(
            midi_track.id,
            clip.id,
            None,
            vec![AutomationPoint {
                tick: 0,
                value: 0.4,
            }],
        )
        .expect_err("upserting automation on midi clip should fail");

    assert!(
        matches!(err, voltlane_core::EngineError::UnsupportedAutomationClip(id) if id == clip.id)
    );
}

#[test]
fn routing_cycle_is_rejected() {
    let mut engine = Engine::new(Project::new("Routing", 140.0, 48_000));
    let bus_a = engine.add_track(AddTrackRequest {
        name: "Bus A".to_string(),
        color: "#f09a59".to_string(),
        kind: TrackKind::Bus,
    });
    let bus_b = engine.add_track(AddTrackRequest {
        name: "Bus B".to_string(),
        color: "#f08a89".to_string(),
        kind: TrackKind::Bus,
    });

    engine
        .patch_track_mix(
            bus_a.id,
            voltlane_core::TrackMixPatch {
                gain_db: None,
                pan: None,
                output_bus: Some(Some(bus_b.id)),
            },
        )
        .expect("first bus route should succeed");

    let err = engine
        .patch_track_mix(
            bus_b.id,
            voltlane_core::TrackMixPatch {
                gain_db: None,
                pan: None,
                output_bus: Some(Some(bus_a.id)),
            },
        )
        .expect_err("cycle route should fail");

    assert!(matches!(
        err,
        voltlane_core::EngineError::RoutingCycleDetected
    ));
}
