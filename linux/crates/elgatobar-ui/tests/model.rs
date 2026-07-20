use elgatobar_dbus::{DeviceSnapshot, OperationResult};
use elgatobar_ui::model::*;
use std::time::Duration;

fn device(
    id: &str,
    online: bool,
    has_state: bool,
    on: bool,
    brightness: u8,
    temperature: u16,
) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: id.into(),
        name: format!("Light {id}"),
        endpoint: format!("{id}.local"),
        online,
        has_state,
        is_on: on,
        brightness,
        temperature,
        consecutive_failures: if online { 0 } else { 2 },
        last_error: if online {
            String::new()
        } else {
            "unreachable".into()
        },
    }
}

fn state(devices: Vec<DeviceSnapshot>) -> (Controller, WindowModel) {
    let mut c = Controller::loading();
    c.connection(ConnectionState::Available);
    c.replace(devices);
    let m = c.model();
    (c, m)
}

#[test]
fn maps_all_top_level_inventory_states() {
    assert_eq!(Controller::loading().model().state, InventoryState::Loading);
    let mut unavailable = Controller::loading();
    unavailable.connection(ConnectionState::Unavailable("missing".into()));
    assert_eq!(unavailable.model().state, InventoryState::Unavailable);
    let unconfigured = state(vec![]).1;
    assert_eq!(unconfigured.state, InventoryState::Unconfigured);
    assert_eq!(
        unconfigured.summary,
        "Add a light by hostname or IP address to start testing."
    );
    assert_eq!(
        state(vec![
            device("a", true, true, true, 50, 250),
            device("b", true, true, true, 60, 240)
        ])
        .1
        .state,
        InventoryState::AllOnlineOn
    );
    assert_eq!(
        state(vec![device("a", true, true, false, 50, 250)]).1.state,
        InventoryState::AllOnlineOff
    );
    assert_eq!(
        state(vec![
            device("a", true, true, true, 50, 250),
            device("b", true, true, false, 60, 240)
        ])
        .1
        .state,
        InventoryState::Mixed
    );
    assert_eq!(
        state(vec![
            device("a", true, true, true, 50, 250),
            device("b", false, true, false, 60, 240)
        ])
        .1
        .state,
        InventoryState::PartialOffline
    );
    assert_eq!(
        state(vec![device("a", false, true, true, 50, 250)]).1.state,
        InventoryState::AllOffline
    );
}

#[test]
fn offline_and_daemon_loss_retain_last_known_values_but_disable_mutations() {
    let (mut c, m) = state(vec![device("a", false, true, true, 61, 250)]);
    assert_eq!(m.rows[0].state.as_ref().unwrap().brightness, "61%");
    assert!(!m.rows[0].mutations_enabled);
    c.connection(ConnectionState::Unavailable("stopped".into()));
    let stale = c.model();
    assert!(stale.stale);
    assert_eq!(stale.rows[0].status, "Stale");
    assert!(!stale.rows[0].mutations_enabled);
}

#[test]
fn missing_state_never_displays_zero_placeholders() {
    let (_, m) = state(vec![device("a", false, false, false, 0, 0)]);
    assert!(m.rows[0].state.is_none());
}

#[test]
fn maps_native_temperature_and_kelvin_endpoints() {
    assert_eq!(native_to_kelvin(143), 6993);
    assert_eq!(native_to_kelvin(344), 2906);
    assert_eq!(native_to_kelvin(250), 4000);
    assert_eq!(kelvin_to_native(7000), 143);
    assert_eq!(kelvin_to_native(2900), 344);
    assert_eq!(kelvin_to_native(4000), 250);
}

#[test]
fn intents_are_typed_range_safe_and_rendering_is_passive() {
    let (mut c, _) = state(vec![device("a", true, true, true, 50, 250)]);
    let before = c.generation();
    let _ = c.model();
    assert_eq!(c.generation(), before);
    assert!(matches!(c.intent(Intent::Toggle("a".into())),Some(Command::Toggle{id,..}) if id=="a"));
    let generation = match c.complete("a", c.generation()) {
        None => c.generation(),
        Some(_) => panic!(),
    };
    assert!(matches!(
        c.intent(Intent::Brightness("a".into(), 1)),
        Some(Command::SetBrightness { value: 3, .. })
    ));
    let pending = c.generation();
    assert!(c.intent(Intent::Brightness("a".into(), 101)).is_none());
    assert!(matches!(
        c.complete("a", pending),
        Some(Command::SetBrightness { value: 100, .. })
    ));
    let pending = c.generation();
    assert!(c.intent(Intent::Temperature("a".into(), 400)).is_none());
    assert!(matches!(
        c.complete("a", pending),
        Some(Command::SetTemperature { value: 344, .. })
    ));
    assert!(generation > before);
}

#[test]
fn every_daemon_operation_has_a_correct_typed_command() {
    let (mut c, _) = state(vec![device("a", true, true, true, 50, 250)]);
    let Some(Command::RefreshAll { generation }) = c.intent(Intent::RefreshAll) else {
        panic!()
    };
    c.complete_aggregate(generation);
    assert!(matches!(
        c.intent(Intent::ToggleAll),
        Some(Command::ToggleAll { .. })
    ));
    assert!(matches!(
        c.intent(Intent::Identify("a".into())),
        Some(Command::Identify { .. })
    ));
    let g = c.generation();
    c.complete("a", g);
    assert!(
        matches!(c.intent(Intent::Add("host.local".into())),Some(Command::Add{endpoint,..}) if endpoint=="host.local")
    );
    assert!(matches!(c.intent(Intent::Remove("a".into())),Some(Command::Remove{id,..}) if id=="a"));
}

#[test]
fn replacement_removes_absent_devices_and_stale_completion_cannot_change_it() {
    let (mut c, _) = state(vec![
        device("a", true, true, true, 50, 250),
        device("b", true, true, false, 60, 240),
    ]);
    let Command::Toggle { id, generation } = c.intent(Intent::Toggle("a".into())).unwrap() else {
        panic!()
    };
    c.replace(vec![device("b", true, true, false, 77, 240)]);
    assert!(c.snapshot("a").is_none());
    assert_eq!(c.snapshot("b").unwrap().brightness, 77);
    assert!(c.complete(&id, generation).is_none());
    assert_eq!(c.snapshot("b").unwrap().brightness, 77);
}

#[test]
fn identical_replacement_does_not_change_the_rendered_view_state() {
    let snapshot = device("a", true, true, true, 50, 250);
    let (mut controller, _) = state(vec![snapshot.clone()]);
    let before = controller.view_state();

    controller.replace(vec![snapshot]);

    assert_eq!(controller.view_state(), before);
}

#[test]
fn feedback_covers_success_partial_complete_skip_and_error_categories() {
    let result = |status: &str, kind: &str| OperationResult {
        device_id: status.into(),
        status: status.into(),
        snapshot: device(status, true, true, true, 50, 250),
        error_kind: kind.into(),
        error: kind.into(),
    };
    assert_eq!(
        operation_feedback(&[result("succeeded", "")]).kind,
        FeedbackKind::Success
    );
    assert_eq!(
        operation_feedback(&[result("succeeded", ""), result("failed", "protocol")]).kind,
        FeedbackKind::PartialFailure
    );
    assert_eq!(
        operation_feedback(&[result("failed", "connectivity")]).kind,
        FeedbackKind::CompleteFailure
    );
    assert_eq!(
        operation_feedback(&[result("skipped-offline", "offline")]).kind,
        FeedbackKind::CompleteFailure
    );
    assert_eq!(
        error_feedback("InvalidInput: bad").kind,
        FeedbackKind::InvalidInput
    );
    assert_eq!(error_feedback("Protocol: bad").kind, FeedbackKind::Protocol);
    assert_eq!(error_feedback("Storage: bad").kind, FeedbackKind::Storage);
    assert_eq!(error_feedback("gone").kind, FeedbackKind::Connectivity);
}

#[test]
fn reconnect_backoff_starts_at_one_second_caps_at_thirty_and_resets() {
    let mut backoff = ReconnectBackoff::default();
    assert_eq!(backoff.current(), Duration::from_secs(1));
    for _ in 0..10 {
        backoff.advance();
    }
    assert_eq!(backoff.current(), Duration::from_secs(30));
    backoff.reset();
    assert_eq!(backoff.current(), Duration::from_secs(1));
}

#[test]
fn aggregate_and_add_commands_suppress_duplicates_until_completion() {
    let (mut c, _) = state(vec![device("a", true, true, true, 50, 250)]);
    let Command::RefreshAll { generation } = c.intent(Intent::RefreshAll).unwrap() else {
        panic!()
    };
    assert!(c.intent(Intent::ToggleAll).is_none());
    c.complete_aggregate(generation);
    assert!(matches!(
        c.intent(Intent::ToggleAll),
        Some(Command::ToggleAll { .. })
    ));
    c.complete_aggregate(c.generation());
    let Command::Add { generation, .. } = c.intent(Intent::Add("one.local".into())).unwrap() else {
        panic!()
    };
    assert!(c.intent(Intent::Add("two.local".into())).is_none());
    c.complete_configuration(generation);
    assert!(c.intent(Intent::Add("two.local".into())).is_some());
}
