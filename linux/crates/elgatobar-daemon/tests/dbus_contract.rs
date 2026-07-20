use std::{process::Command, time::Duration};

use elgatobar_dbus::ElgatoBarProxy;
use futures_util::StreamExt;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

const CHILD_MARKER: &str = "ELGATOBAR_DBUS_TEST_CHILD";

#[tokio::test(flavor = "multi_thread")]
async fn daemon_exposes_commands_snapshots_and_change_signals() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD_MARKER}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "daemon_exposes_commands_snapshots_and_change_signals",
                "--nocapture",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 1, "brightness": 60, "temperature": 250}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .and(body_json(json!({
            "lights": [{"on": 1, "brightness": 75, "temperature": 250}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 1, "brightness": 75, "temperature": 250}]
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
            "serialNumber": "ABC",
            "displayName": "Desk"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/elgato/identify"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_elgatobar-daemon"))
        .args([
            "--endpoint",
            server.uri().as_str(),
            "--poll-interval-seconds",
            "30",
        ])
        .spawn()
        .unwrap();

    let connection = zbus::Connection::session().await.unwrap();
    let proxy = ElgatoBarProxy::new(&connection).await.unwrap();
    let mut snapshot = None;
    for _ in 0..50 {
        match proxy.refresh().await {
            Ok(value) => {
                snapshot = Some(value);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    let snapshot = snapshot.expect("daemon did not acquire its D-Bus name");
    assert!(snapshot.online);
    assert_eq!(snapshot.brightness, 60);

    let mut changes = proxy.receive_state_changed().await.unwrap();
    let updated = proxy.set_state(false, false, 75, 0).await.unwrap();
    assert_eq!(updated.brightness, 75);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), changes.next())
            .await
            .unwrap()
            .is_some()
    );
    let error = proxy.set_state(false, false, 2, 0).await.unwrap_err();
    assert!(error.to_string().contains("InvalidInput"));

    let accessory = proxy.accessory_info().await.unwrap();
    assert_eq!(accessory.display_name, "Desk");
    assert_eq!(accessory.serial_number, "ABC");
    proxy.identify().await.unwrap();

    daemon.kill().unwrap();
    let _ = daemon.wait();
}
