use std::{
    process::{Child, Command},
    time::Duration,
};

use elgatobar_dbus::ElgatoBarProxy;
use futures_util::StreamExt;
use serde_json::json;
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

const CHILD_MARKER: &str = "ELGATOBAR_DBUS_TEST_CHILD";

async fn light_server(serial: &str, is_on: bool) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": i32::from(is_on), "brightness": 60, "temperature": 250}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/elgato/accessory-info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "productName": "Key Light",
            "hardwareBoardType": 53,
            "firmwareBuildNumber": 218,
            "firmwareVersion": "1.0.3",
            "serialNumber": serial,
            "displayName": format!("Desk {serial}")
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/elgato/identify"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    server
}

fn daemon(data: &TempDir, config: &TempDir) -> Child {
    Command::new(env!("CARGO_BIN_EXE_elgatobar-daemon"))
        .args([
            "--data-root",
            data.path().to_str().unwrap(),
            "--config-root",
            config.path().to_str().unwrap(),
        ])
        .spawn()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn daemon_exposes_multidevice_commands_persistence_snapshots_and_signals() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD_MARKER}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "daemon_exposes_multidevice_commands_persistence_snapshots_and_signals",
                "--nocapture",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let first = light_server("DBUS-A", true).await;
    let second = light_server("DBUS-B", false).await;
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .and(body_json(
            json!({"lights": [{"on": 1, "brightness": 75, "temperature": 250}]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1, "lights": [{"on": 1, "brightness": 75, "temperature": 250}]
        })))
        .mount(&first)
        .await;
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .and(body_json(
            json!({"lights": [{"on": 0, "brightness": 60, "temperature": 250}]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1, "lights": [{"on": 0, "brightness": 60, "temperature": 250}]
        })))
        .mount(&first)
        .await;
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1, "lights": [{"on": 0, "brightness": 60, "temperature": 250}]
        })))
        .mount(&second)
        .await;

    let data = TempDir::new().unwrap();
    let config = TempDir::new().unwrap();
    let mut process = daemon(&data, &config);
    let connection = zbus::Connection::session().await.unwrap();
    let proxy = ElgatoBarProxy::new(&connection).await.unwrap();
    for _ in 0..100 {
        if proxy.list_devices().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut changes = proxy.receive_devices_changed().await.unwrap();
    let first_snapshot = proxy.add_device(&first.uri()).await.unwrap();
    let second_snapshot = proxy.add_device(&second.uri()).await.unwrap();
    assert_ne!(first_snapshot.device_id, second_snapshot.device_id);
    assert!(changes.next().await.is_some());
    assert_eq!(proxy.list_devices().await.unwrap().len(), 2);
    assert!(
        proxy
            .snapshot()
            .await
            .unwrap()
            .last_error
            .contains("multiple devices")
    );
    assert!(
        proxy
            .refresh()
            .await
            .unwrap_err()
            .to_string()
            .contains("multiple devices")
    );

    let refreshed = proxy.refresh_all().await.unwrap();
    assert_eq!(refreshed.len(), 2);
    assert!(refreshed.iter().all(|result| result.status == "succeeded"));
    let updated = proxy
        .set_device_state(&first_snapshot.device_id, false, false, 75, 0)
        .await
        .unwrap();
    assert_eq!(updated.snapshot.brightness, 75);
    let toggled = proxy.toggle_all().await.unwrap();
    assert_eq!(toggled.len(), 2);
    assert!(toggled.iter().all(|result| result.status == "succeeded"));
    assert!(toggled.iter().all(|result| !result.snapshot.is_on));
    assert_eq!(
        proxy
            .identify_device(&second_snapshot.device_id)
            .await
            .unwrap()
            .status,
        "succeeded"
    );

    process.kill().unwrap();
    process.wait().unwrap();
    let mut restarted = daemon(&data, &config);
    let reconnect = zbus::Connection::session().await.unwrap();
    let restarted_proxy = ElgatoBarProxy::new(&reconnect).await.unwrap();
    let mut restored = None;
    for _ in 0..100 {
        if let Ok(devices) = restarted_proxy.list_devices().await
            && devices.len() == 2
        {
            restored = Some(devices);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let restored = restored.expect("daemon did not reload both persisted devices");
    assert!(
        restored
            .iter()
            .any(|device| device.device_id == first_snapshot.device_id)
    );
    assert!(
        restored
            .iter()
            .any(|device| device.device_id == second_snapshot.device_id)
    );
    restarted.kill().unwrap();
    restarted.wait().unwrap();
}
