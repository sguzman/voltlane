use uuid::Uuid;
use voltlane_core::{
    AddTrackRequest, Engine, EngineError, TrackMixPatch, TrackSend,
    model::{Project, TrackKind},
};

fn base_engine() -> Engine {
    let mut engine = Engine::new(Project::new("Routing", 140.0, 48_000));
    let _midi = engine.add_track(AddTrackRequest {
        name: "Lead".to_string(),
        color: "#22c7b8".to_string(),
        kind: TrackKind::Midi,
    });
    let _bus = engine.add_track(AddTrackRequest {
        name: "Bus A".to_string(),
        color: "#ffaa66".to_string(),
        kind: TrackKind::Bus,
    });
    engine
}

#[test]
fn track_mix_patch_sets_bus_gain_and_pan() {
    let mut engine = base_engine();
    let track_id = engine.project().tracks[0].id;
    let bus_id = engine.project().tracks[1].id;

    let updated = engine
        .patch_track_mix(
            track_id,
            TrackMixPatch {
                gain_db: Some(-6.0),
                pan: Some(0.25),
                output_bus: Some(Some(bus_id)),
            },
        )
        .expect("patching track mix should succeed");

    assert_eq!(updated.output_bus, Some(bus_id));
    assert_eq!(updated.gain_db, -6.0);
    assert_eq!(updated.pan, 0.25);
}

#[test]
fn track_mix_patch_rejects_invalid_bus_targets() {
    let mut engine = base_engine();
    let track_id = engine.project().tracks[0].id;

    let err = engine
        .patch_track_mix(
            track_id,
            TrackMixPatch {
                gain_db: None,
                pan: None,
                output_bus: Some(Some(Uuid::new_v4())),
            },
        )
        .expect_err("unknown bus should fail");

    assert!(matches!(err, EngineError::InvalidBusTarget { .. }));
}

#[test]
fn add_and_remove_track_send_roundtrip() {
    let mut engine = base_engine();
    let track_id = engine.project().tracks[0].id;
    let bus_id = engine.project().tracks[1].id;

    let updated = engine
        .upsert_track_send(
            track_id,
            TrackSend {
                id: Uuid::new_v4(),
                target_bus: bus_id,
                level_db: -9.0,
                pan: -0.2,
                pre_fader: true,
                enabled: true,
            },
        )
        .expect("upsert send should succeed");
    assert_eq!(updated.sends.len(), 1);

    let send_id = updated.sends[0].id;
    let updated = engine
        .remove_track_send(track_id, send_id)
        .expect("remove send should succeed");
    assert!(updated.sends.is_empty());
}

#[test]
fn automation_parameter_ids_include_track_mix_and_fx_defaults() {
    let mut engine = base_engine();
    let track_id = engine.project().tracks[0].id;

    engine
        .add_effect(track_id, voltlane_core::EffectSpec::new("eq"))
        .expect("adding built-in eq should succeed");

    let ids = engine.automation_parameter_ids();
    let gain_id = format!("track:{}:gain_db", track_id);
    let pan_id = format!("track:{}:pan", track_id);
    assert!(ids.iter().any(|id| id == &gain_id));
    assert!(ids.iter().any(|id| id == &pan_id));
    assert!(
        ids.iter()
            .any(|id| id.contains(":effect:") && id.ends_with(":high_gain_db")),
        "eq defaults should expose automatable params"
    );
}
