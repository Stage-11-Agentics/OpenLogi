//! The shared Actions Ring: one top-level ring that every device opens.

use super::*;
use crate::binding::{Action, ActionRingConfig, ActionRingLayout, ActionRingSlot, RingAction};

fn shared() -> Config {
    Config {
        action_ring: Some(ActionRingConfig::default()),
        ..Config::default()
    }
}

fn new_tab() -> RingAction {
    RingAction::new(Action::NewTab).expect("new tab must be a valid ring action")
}

#[test]
fn without_a_shared_ring_each_device_keeps_its_own() {
    let mut cfg = Config::default();
    cfg.set_action_ring_slot("mouse", ActionRingSlot::Top, Some(new_tab()));
    assert_eq!(
        cfg.action_ring("mouse").default.slots[&ActionRingSlot::Top].action(),
        &Action::NewTab
    );
    assert_ne!(cfg.action_ring("trackball"), cfg.action_ring("mouse"));
    assert!(cfg.action_ring.is_none());
}

#[test]
fn a_shared_ring_wins_over_every_device_ring() {
    let mut cfg = Config::default();
    cfg.set_action_ring_haptics("mouse", false);
    cfg.action_ring = Some(ActionRingConfig::default());
    cfg.set_action_ring_slot("trackball", ActionRingSlot::Top, Some(new_tab()));

    let mouse = cfg.action_ring("mouse");
    assert_eq!(mouse, cfg.action_ring("trackball"));
    assert_eq!(mouse, cfg.action_ring("never-seen"));
    assert_eq!(
        mouse.default.slots[&ActionRingSlot::Top].action(),
        &Action::NewTab
    );
    assert!(mouse.haptics, "the device's own ring no longer applies");
    assert!(
        !cfg.devices.contains_key("trackball"),
        "an edit to the shared ring creates no device entry"
    );
}

#[test]
fn edits_through_any_device_reach_the_shared_ring() {
    let mut cfg = shared();
    cfg.action_ring_mut("mouse")
        .per_app
        .insert("com.example".to_owned(), ActionRingLayout::default());
    cfg.set_action_ring_enabled("trackball", false);

    let shared = cfg.action_ring.as_ref().expect("shared ring");
    assert!(shared.per_app.contains_key("com.example"));
    assert!(!shared.enabled);
}

#[test]
fn shared_ring_roundtrips_and_is_omitted_when_unset() {
    let mut cfg = Config::default();
    let text = toml::to_string(&cfg).expect("serialize");
    assert!(!text.contains("action_ring"));

    cfg.action_ring = Some(ActionRingConfig::default());
    cfg.set_action_ring_slot("mouse", ActionRingSlot::Top, Some(new_tab()));
    let restored = write_and_read(&cfg);
    assert_eq!(restored.action_ring, cfg.action_ring);
    assert_eq!(restored.action_ring("trackball"), cfg.action_ring("mouse"));
}
