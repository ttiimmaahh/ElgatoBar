use std::process::Command;

use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin, cargo_bin_cmd};
use elgatobar_dbus::{
    AccessorySnapshot, DeviceSnapshot, OBJECT_PATH, OperationResult, SERVICE_NAME,
};
use predicates::prelude::*;
use serde_json::Value;

const CHILD_MARKER: &str = "ELGATOBAR_CLI_DBUS_TEST_CHILD";
const FIRST_ID: &str = "serial/616263";

struct FakeService;

fn device(id: &str, is_on: bool) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: id.to_string(),
        name: "Desk".to_string(),
        endpoint: "key-light.local:9123".to_string(),
        online: true,
        has_state: true,
        is_on,
        brightness: 60,
        temperature: 250,
        consecutive_failures: 0,
        last_error: String::new(),
    }
}

fn result(id: &str, status: &str) -> OperationResult {
    OperationResult {
        device_id: id.to_string(),
        status: status.to_string(),
        snapshot: device(id, status == "succeeded"),
        error_kind: if status == "failed" {
            "connectivity".to_string()
        } else {
            String::new()
        },
        error: if status == "failed" {
            "offline".to_string()
        } else {
            String::new()
        },
    }
}

#[zbus::interface(name = "io.github.ttiimmaahh.ElgatoBar1")]
impl FakeService {
    fn add_device(&self, _endpoint: &str) -> DeviceSnapshot {
        device(FIRST_ID, true)
    }
    fn remove_device(&self, device_id: &str) -> DeviceSnapshot {
        device(device_id, true)
    }
    fn list_devices(&self) -> Vec<DeviceSnapshot> {
        vec![device(FIRST_ID, true)]
    }
    fn device_snapshot(&self, device_id: &str) -> DeviceSnapshot {
        device(device_id, true)
    }
    fn refresh_device(&self, device_id: &str) -> OperationResult {
        result(device_id, "succeeded")
    }
    fn refresh_all(&self) -> Vec<OperationResult> {
        vec![result(FIRST_ID, "succeeded")]
    }
    fn set_device_state(
        &self,
        device_id: &str,
        _has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
    ) -> OperationResult {
        let mut operation = result(device_id, "succeeded");
        operation.snapshot.is_on = power;
        operation.snapshot.brightness = brightness;
        operation.snapshot.temperature = temperature;
        operation
    }
    fn toggle_device(&self, device_id: &str) -> OperationResult {
        result(device_id, "succeeded")
    }
    fn identify_device(&self, device_id: &str) -> OperationResult {
        result(device_id, "succeeded")
    }
    fn toggle_all(&self) -> Vec<OperationResult> {
        vec![
            result(FIRST_ID, "succeeded"),
            result("serial/646566", "failed"),
        ]
    }
    fn accessory_info(&self) -> AccessorySnapshot {
        AccessorySnapshot {
            display_name: "Desk".to_string(),
            product_name: "Key Light".to_string(),
            serial_number: "ABC".to_string(),
            firmware_version: "1.0.3".to_string(),
            hardware_board_type: 53,
        }
    }
}

#[test]
fn help_describes_multidevice_daemon_workflows() {
    cargo_bin_cmd!("elgatobar")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("devices"))
        .stdout(predicate::str::contains("toggle-all"));
}

#[test]
fn invalid_set_values_fail_before_connecting_to_dbus() {
    cargo_bin_cmd!("elgatobar")
        .args(["--json", "set", FIRST_ID, "--brightness", "2"])
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalidInput"));
    cargo_bin_cmd!("elgatobar")
        .args(["set", FIRST_ID])
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("requires at least one"));
}

#[test]
fn json_parser_failures_are_valid_error_documents() {
    let output = cargo_bin_cmd!("elgatobar")
        .args(["--json", "set", FIRST_ID, "--brightness", "not-a-number"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let document: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(document["error"]["kind"], "invalidInput");
}

#[test]
fn missing_session_bus_is_a_connectivity_failure() {
    cargo_bin_cmd!("elgatobar")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .args(["--json", "devices", "list"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("connectivity"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_uses_typed_multidevice_contract_and_partial_exit_status() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD_MARKER}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli_uses_typed_multidevice_contract_and_partial_exit_status",
                "--nocapture",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let _connection = zbus::connection::Builder::session()
        .unwrap()
        .name(SERVICE_NAME)
        .unwrap()
        .serve_at(OBJECT_PATH, FakeService)
        .unwrap()
        .build()
        .await
        .unwrap();
    let binary = cargo_bin!("elgatobar");

    Command::new(binary)
        .args(["--json", "devices", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(FIRST_ID));
    Command::new(binary)
        .args(["state", FIRST_ID])
        .assert()
        .success()
        .stdout(predicate::str::contains("Brightness: 60%"));
    Command::new(binary)
        .args(["refresh", FIRST_ID])
        .assert()
        .success()
        .stdout(predicate::str::contains("succeeded"));
    Command::new(binary)
        .args([
            "set",
            FIRST_ID,
            "--on",
            "--brightness",
            "75",
            "--temperature",
            "200",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Brightness: 75%"));
    Command::new(binary)
        .args(["toggle", FIRST_ID])
        .assert()
        .success();
    Command::new(binary)
        .args(["identify", FIRST_ID])
        .assert()
        .success();
    Command::new(binary)
        .args(["--json", "toggle-all"])
        .assert()
        .code(5)
        .stdout(predicate::str::contains("\"failed\":1"));
}
