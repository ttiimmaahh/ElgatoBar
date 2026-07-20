use std::{fs, path::PathBuf, str::FromStr};

use elgatobar_core::{
    AccessoryInfo, Brightness, DeviceEndpoint, DeviceIdentity, DeviceIdentityKind, DocumentName,
    ElgatoTemperature, INTERCHANGE_VERSION, InterchangeDocument, LightState, PersistedDevice,
    Scene, SceneLight,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn accessory(serial: &str) -> AccessoryInfo {
    AccessoryInfo {
        product_name: "Key Light".to_string(),
        hardware_board_type: 53,
        firmware_build_number: 218,
        firmware_version: "1.0.3".to_string(),
        serial_number: serial.to_string(),
        display_name: Some("Desk".to_string()),
        features: None,
        wifi_info: None,
    }
}

#[test]
fn endpoint_accepts_documented_forms_and_rejects_unsafe_components() {
    let host = DeviceEndpoint::from_str("key-light.local").unwrap();
    assert_eq!(host.host(), "key-light.local");
    assert_eq!(host.port(), 9123);

    let explicit = DeviceEndpoint::from_str("http://192.0.2.10:8123").unwrap();
    assert_eq!(explicit.to_string(), "192.0.2.10:8123");

    for value in ["http://[::1]:8123", "[::1]:8123"] {
        let ipv6 = DeviceEndpoint::from_str(value).unwrap();
        assert_eq!(ipv6.host(), "::1");
        assert_eq!(ipv6.port(), 8123);
        assert_eq!(ipv6.to_string(), "[::1]:8123");
    }

    assert!(DeviceEndpoint::from_str("https://key-light.local").is_err());
    assert!(DeviceEndpoint::from_str("http://user@key-light.local").is_err());
    assert!(DeviceEndpoint::from_str("http://key-light.local/path").is_err());
    assert!(DeviceEndpoint::from_str("http://key-light.local?next=x").is_err());
    assert!(serde_json::from_value::<DeviceEndpoint>(json!({"host": "", "port": 9123})).is_err());
    assert!(
        serde_json::from_value::<DeviceEndpoint>(json!({"host": "light.local", "port": 0}))
            .is_err()
    );
    assert!(DeviceEndpoint::new("not:ipv6", 9123).is_err());
    assert!(DeviceEndpoint::new(" light.local ", 9123).is_err());
    assert!(DeviceEndpoint::new("light.local/path", 9123).is_err());
    assert!(DeviceEndpoint::new("foo%2fbar", 9123).is_err());
    assert!(DeviceEndpoint::new("[evil]", 9123).is_err());
}

#[test]
fn brightness_and_temperature_enforce_device_ranges() {
    assert_eq!(Brightness::try_from(3).unwrap().get(), 3);
    assert_eq!(Brightness::try_from(100).unwrap().get(), 100);
    assert!(Brightness::try_from(2).is_err());
    assert!(Brightness::try_from(101).is_err());

    assert_eq!(ElgatoTemperature::try_from(143).unwrap().to_kelvin(), 6993);
    assert_eq!(ElgatoTemperature::try_from(200).unwrap().to_kelvin(), 5000);
    assert_eq!(ElgatoTemperature::try_from(344).unwrap().to_kelvin(), 2906);
    assert_eq!(ElgatoTemperature::from_kelvin(1_000).get(), 344);
    assert_eq!(ElgatoTemperature::from_kelvin(5_000).get(), 200);
    assert_eq!(ElgatoTemperature::from_kelvin(10_000).get(), 143);
}

#[test]
fn identity_uses_serial_then_stable_mdns_then_confirmed_installation_id() {
    let endpoint = DeviceEndpoint::from_str("192.0.2.10").unwrap();
    let generated = Uuid::parse_str("018f4eb6-4c42-7f6f-b7fb-0d3cae912345").unwrap();

    let serial = DeviceIdentity::select(
        &accessory("  ABC 123  "),
        Some("Desk"),
        &endpoint,
        generated,
    );
    assert_eq!(serial, DeviceIdentity::serial("abc 123").unwrap());
    assert_eq!(serial.kind(), DeviceIdentityKind::Serial);
    assert_eq!(serial.serial_value(), Some("abc 123"));
    assert!(serial.can_follow_endpoint_change());

    let mdns = DeviceIdentity::select(
        &accessory("  "),
        Some("  Desk   Light  "),
        &endpoint,
        generated,
    );
    assert_eq!(
        mdns,
        DeviceIdentity::mdns("desk light", "key light", 53).unwrap()
    );
    assert_eq!(mdns.kind(), DeviceIdentityKind::Mdns);
    assert_eq!(mdns.mdns_value(), Some(("desk light", "key light", 53)));
    assert!(mdns.can_follow_endpoint_change());

    let local = DeviceIdentity::select(&accessory(""), None, &endpoint, generated);
    assert_eq!(
        local,
        DeviceIdentity::installation_local(generated, endpoint.clone())
    );
    assert_eq!(local.kind(), DeviceIdentityKind::InstallationLocal);
    assert_eq!(
        local.installation_local_value(),
        Some((generated, &endpoint))
    );
    assert!(!local.can_follow_endpoint_change());

    let decoded: DeviceIdentity = serde_json::from_value(json!({
        "kind": "serial",
        "serial": "  ABC   123 "
    }))
    .unwrap();
    assert_eq!(decoded, serial);
    assert!(
        serde_json::from_value::<DeviceIdentity>(json!({
            "kind": "serial",
            "serial": "   "
        }))
        .is_err()
    );
}

#[test]
fn interchange_fixture_round_trips_and_rejects_future_versions() {
    let fixture = fixture_path("api-fixtures/interchange-v1.json");
    let text = fs::read_to_string(fixture).unwrap();
    let document: InterchangeDocument = serde_json::from_str(&text).unwrap();
    assert_eq!(document.version(), INTERCHANGE_VERSION);
    assert_eq!(document.devices.len(), 1);
    assert_eq!(
        document.scenes[0].lights[0].device_identity,
        document.devices[0].identity
    );

    let encoded = serde_json::to_value(&document).unwrap();
    let decoded: InterchangeDocument = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, document);

    let future = json!({"version": 2, "devices": [], "scenes": []});
    assert!(serde_json::from_value::<InterchangeDocument>(future).is_err());
}

#[test]
fn interchange_fixture_validates_against_v1_schema() {
    let schema = interchange_schema();
    let instance = interchange_fixture();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let errors: Vec<_> = validator
        .iter_errors(&instance)
        .map(|error| error.to_string())
        .collect();
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
}

#[test]
fn invalid_interchange_documents_are_rejected_by_schema_and_rust() {
    let schema = interchange_schema();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let mut cases = Vec::new();

    let mut top_extra = interchange_fixture();
    top_extra
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_string(), json!(true));
    cases.push(("top-level extra field", top_extra));

    for (label, pointer) in [
        ("device extra field", "/devices/0"),
        ("endpoint extra field", "/devices/0/endpoint"),
        ("identity extra field", "/devices/0/identity"),
        ("scene extra field", "/scenes/0"),
        ("scene-light extra field", "/scenes/0/lights/0"),
        ("state extra field", "/scenes/0/lights/0/state"),
    ] {
        let mut instance = interchange_fixture();
        instance
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), json!(true));
        cases.push((label, instance));
    }

    for (label, pointer, invalid) in [
        ("blank device name", "/devices/0/name", json!("  \t")),
        ("blank scene name", "/scenes/0/name", json!("\n  ")),
        (
            "blank serial identity",
            "/devices/0/identity/serial",
            json!("   "),
        ),
        (
            "unsafe endpoint host",
            "/devices/0/endpoint/host",
            json!("light.local/path"),
        ),
        (
            "whitespace endpoint host",
            "/devices/0/endpoint/host",
            json!(" light.local "),
        ),
        (
            "invalid IPv6 endpoint host",
            "/devices/0/endpoint/host",
            json!("not:ipv6"),
        ),
        (
            "invalid endpoint port",
            "/devices/0/endpoint/port",
            json!(0),
        ),
        (
            "invalid brightness",
            "/scenes/0/lights/0/state/brightness",
            json!(2),
        ),
        (
            "invalid temperature",
            "/scenes/0/lights/0/state/temperature",
            json!(345),
        ),
        ("invalid scene UUID", "/scenes/0/id", json!("not-a-uuid")),
    ] {
        let mut instance = interchange_fixture();
        *instance.pointer_mut(pointer).unwrap() = invalid;
        cases.push((label, instance));
    }

    let mut blank_mdns = interchange_fixture();
    *blank_mdns.pointer_mut("/devices/0/identity").unwrap() = json!({
        "kind": "mdns",
        "instance": "  ",
        "productName": "Key Light",
        "hardwareBoardType": 53
    });
    cases.push(("blank normalized mDNS identity", blank_mdns));

    for (label, pointer, invalid) in [
        (
            "Unicode-blank device name",
            "/devices/0/name",
            json!("\u{0085}"),
        ),
        (
            "Unicode-whitespace endpoint host",
            "/devices/0/endpoint/host",
            json!("\u{0085}"),
        ),
        (
            "percent-escaped endpoint host",
            "/devices/0/endpoint/host",
            json!("foo%2fbar"),
        ),
        (
            "bracketed non-IPv6 host",
            "/devices/0/endpoint/host",
            json!("[evil]"),
        ),
        (
            "invalid colon host",
            "/devices/0/endpoint/host",
            json!("not:ipv6"),
        ),
        (
            "fractional endpoint port",
            "/devices/0/endpoint/port",
            json!(9123.5),
        ),
        (
            "fractional brightness",
            "/scenes/0/lights/0/state/brightness",
            json!(3.5),
        ),
        (
            "fractional temperature",
            "/scenes/0/lights/0/state/temperature",
            json!(143.5),
        ),
        (
            "compact scene UUID",
            "/scenes/0/id",
            json!("018f4eb64c427f6fb7fb0d3cae900001"),
        ),
        (
            "braced scene UUID",
            "/scenes/0/id",
            json!("{018f4eb6-4c42-7f6f-b7fb-0d3cae900001}"),
        ),
        (
            "URN scene UUID",
            "/scenes/0/id",
            json!("urn:uuid:018f4eb6-4c42-7f6f-b7fb-0d3cae900001"),
        ),
    ] {
        let mut instance = interchange_fixture();
        *instance.pointer_mut(pointer).unwrap() = invalid;
        cases.push((label, instance));
    }

    let mut oversized_board = interchange_fixture();
    *oversized_board.pointer_mut("/devices/0/identity").unwrap() = json!({
        "kind": "mdns",
        "instance": "desk-light",
        "productName": "Key Light",
        "hardwareBoardType": 9_223_372_036_854_775_808_u64
    });
    cases.push(("hardware board above i64", oversized_board));

    let mut fractional_board = interchange_fixture();
    *fractional_board.pointer_mut("/devices/0/identity").unwrap() = json!({
        "kind": "mdns",
        "instance": "desk-light",
        "productName": "Key Light",
        "hardwareBoardType": 53.5
    });
    cases.push(("fractional hardware board", fractional_board));

    for (label, id) in [
        (
            "compact installation UUID",
            "018f4eb64c427f6fb7fb0d3cae900001",
        ),
        (
            "braced installation UUID",
            "{018f4eb6-4c42-7f6f-b7fb-0d3cae900001}",
        ),
        (
            "URN installation UUID",
            "urn:uuid:018f4eb6-4c42-7f6f-b7fb-0d3cae900001",
        ),
    ] {
        let mut instance = interchange_fixture();
        *instance.pointer_mut("/devices/0/identity").unwrap() = json!({
            "kind": "installation-local",
            "id": id,
            "confirmedEndpoint": {"host": "192.0.2.10", "port": 9123}
        });
        cases.push((label, instance));
    }

    for (label, instance) in cases {
        let schema_errors: Vec<_> = validator
            .iter_errors(&instance)
            .map(|error| error.to_string())
            .collect();
        assert!(
            !schema_errors.is_empty(),
            "schema unexpectedly accepted {label}"
        );
        assert!(
            serde_json::from_value::<InterchangeDocument>(instance).is_err(),
            "Rust unexpectedly accepted {label}"
        );
    }
}

#[test]
fn schema_and_rust_accept_integral_numeric_forms_and_valid_ipv6() {
    let schema = interchange_schema();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();

    let mut integral_numbers = interchange_fixture();
    *integral_numbers.pointer_mut("/version").unwrap() = json!(1.0);
    *integral_numbers
        .pointer_mut("/devices/0/endpoint/port")
        .unwrap() = json!(9123.0);
    *integral_numbers
        .pointer_mut("/scenes/0/lights/0/state/brightness")
        .unwrap() = json!(3.0);
    *integral_numbers
        .pointer_mut("/scenes/0/lights/0/state/temperature")
        .unwrap() = json!(143.0);
    *integral_numbers.pointer_mut("/devices/0/identity").unwrap() = json!({
        "kind": "mdns",
        "instance": "desk-light",
        "productName": "Key Light",
        "hardwareBoardType": 53.0
    });

    let mut ipv6 = interchange_fixture();
    *ipv6.pointer_mut("/devices/0/endpoint/host").unwrap() = json!("::1");

    let mut ipv4_embedded_ipv6 = interchange_fixture();
    *ipv4_embedded_ipv6
        .pointer_mut("/devices/0/endpoint/host")
        .unwrap() = json!("::ffff:192.0.2.1");

    for (label, instance) in [
        ("integral numeric encodings", integral_numbers),
        ("unbracketed IPv6 host", ipv6),
        ("IPv4-embedded IPv6 host", ipv4_embedded_ipv6),
    ] {
        let errors: Vec<_> = validator.iter_errors(&instance).collect();
        assert!(errors.is_empty(), "schema rejected {label}: {errors:#?}");
        serde_json::from_value::<InterchangeDocument>(instance)
            .unwrap_or_else(|error| panic!("Rust rejected {label}: {error}"));
    }
}

#[test]
fn public_model_construction_always_serializes_to_the_schema() {
    let endpoint = DeviceEndpoint::new("::ffff:192.0.2.1", 9123).unwrap();
    let identity = DeviceIdentity::serial("  CW12A1A00001  ").unwrap();
    let name = DocumentName::new("Desk Key Light").unwrap();
    let state = LightState {
        is_on: true,
        brightness: Brightness::try_from(75).unwrap(),
        temperature: ElgatoTemperature::try_from(200).unwrap(),
    };
    let device = PersistedDevice::new(identity.clone(), name, endpoint);
    let scene = Scene::new(
        Uuid::parse_str("018f4eb6-4c42-7f6f-b7fb-0d3cae900001").unwrap(),
        DocumentName::new("Meeting").unwrap(),
        vec![SceneLight::new(identity, state)],
    );
    let document = InterchangeDocument::new(vec![device], vec![scene]);
    let instance = serde_json::to_value(document).unwrap();

    let schema = interchange_schema();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let errors: Vec<_> = validator.iter_errors(&instance).collect();
    assert!(
        errors.is_empty(),
        "public model emitted schema errors: {errors:#?}"
    );
    assert_eq!(
        instance.pointer("/devices/0/identity/serial"),
        Some(&json!("cw12a1a00001"))
    );
    assert_eq!(
        instance.pointer("/scenes/0/id"),
        Some(&json!("018f4eb6-4c42-7f6f-b7fb-0d3cae900001"))
    );
    assert_eq!(
        instance.pointer("/devices/0/endpoint/host"),
        Some(&json!("::ffff:192.0.2.1"))
    );

    assert!(DocumentName::new("\u{0085}").is_err());
    assert!(DeviceIdentity::serial("\u{0085}").is_err());
    assert!(DeviceIdentity::mdns("desk", "\u{0085}", 53).is_err());
}

fn interchange_schema() -> Value {
    serde_json::from_str(
        &fs::read_to_string(fixture_path("schemas/elgatobar-interchange-v1.schema.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn all_published_json_schemas_are_valid_draft_2020_12_documents() {
    for name in [
        "elgatobar-interchange-v1.schema.json",
        "elgatobar-linux-devices-v1.schema.json",
        "elgatobar-linux-settings-v1.schema.json",
    ] {
        let schema: Value = serde_json::from_str(
            &fs::read_to_string(fixture_path(&format!("schemas/{name}"))).unwrap(),
        )
        .unwrap();
        jsonschema::meta::validate(&schema)
            .unwrap_or_else(|error| panic!("{name} is not a valid JSON Schema: {error}"));
    }
}

fn interchange_fixture() -> Value {
    serde_json::from_str(
        &fs::read_to_string(fixture_path("api-fixtures/interchange-v1.json")).unwrap(),
    )
    .unwrap()
}

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../shared")
        .join(relative)
}
