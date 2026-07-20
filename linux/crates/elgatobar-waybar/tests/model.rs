use elgatobar_dbus::DeviceSnapshot;
use elgatobar_waybar::{Availability, WaybarOutput, render};

fn device(id: &str, online: bool, has_state: bool, on: bool, brightness: u8) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: id.into(),
        name: format!("Light {id}"),
        endpoint: format!("{id}.local"),
        online,
        has_state,
        is_on: on,
        brightness,
        temperature: 250,
        consecutive_failures: if online { 0 } else { 2 },
        last_error: if online {
            String::new()
        } else {
            "unreachable".into()
        },
    }
}

#[test]
fn renders_unconfigured_unavailable_and_stale_states() {
    let unconfigured = render(&[], Availability::Available);
    assert_eq!(unconfigured.alt, "unconfigured");
    assert_eq!(unconfigured.class, ["unconfigured"]);

    let unavailable = render(&[], Availability::Unavailable("missing"));
    assert_eq!(unavailable.alt, "unavailable");
    assert_eq!(unavailable.class, ["unavailable"]);

    let stale = render(
        &[device("a", true, true, true, 60)],
        Availability::Unavailable("stopped"),
    );
    assert_eq!(stale.text, "Lights stale");
    assert_eq!(stale.alt, "stale");
    assert!(stale.class.contains(&"stale".into()));
    assert!(stale.tooltip.contains("showing stale values"));
    assert_eq!(stale.percentage, 60);
}

#[test]
fn classifies_power_and_connectivity_and_averages_enabled_brightness() {
    let off = render(
        &[device("a", true, true, false, 30)],
        Availability::Available,
    );
    assert_eq!(off.text, "Lights off");
    assert_eq!(off.class, ["off"]);
    assert_eq!(off.percentage, 0);

    let on = render(
        &[
            device("a", true, true, true, 40),
            device("b", true, true, true, 80),
        ],
        Availability::Available,
    );
    assert_eq!(on.text, "Lights 2/2");
    assert_eq!(on.class, ["on"]);
    assert_eq!(on.percentage, 60);

    let mixed = render(
        &[
            device("a", true, true, true, 40),
            device("b", true, true, false, 80),
        ],
        Availability::Available,
    );
    assert_eq!(mixed.class, ["mixed"]);
    assert_eq!(mixed.percentage, 40);

    let partial = render(
        &[
            device("a", true, true, true, 40),
            device("b", false, true, false, 80),
        ],
        Availability::Available,
    );
    assert_eq!(partial.class, ["on", "partial-offline"]);
    assert!(partial.tooltip.contains("offline"));

    let offline = render(
        &[device("a", false, true, true, 40)],
        Availability::Available,
    );
    assert_eq!(offline.text, "Lights offline");
    assert_eq!(offline.class, ["offline"]);
}

#[test]
fn output_is_one_waybar_json_object_with_escaped_multiline_tooltip() {
    let output = render(
        &[
            device("a", true, true, true, 40),
            device("b", false, false, false, 0),
        ],
        Availability::Available,
    );
    let encoded = serde_json::to_string(&output).unwrap();
    assert!(!encoded.contains('\n'));
    let decoded: WaybarOutput = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, output);
    assert!(decoded.tooltip.contains('\r'));
    assert!(!decoded.tooltip.contains(".local"));
}
