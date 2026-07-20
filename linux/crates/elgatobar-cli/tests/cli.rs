use std::process::Command;

use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd, cargo_bin};
use elgatobar_dbus::{AccessorySnapshot, LightSnapshot, OBJECT_PATH, SERVICE_NAME};
use predicates::prelude::*;
use serde_json::Value;

const CHILD_MARKER: &str = "ELGATOBAR_CLI_DBUS_TEST_CHILD";

struct FakeService;

#[zbus::interface(name = "io.github.ttiimmaahh.ElgatoBar1")]
impl FakeService {
    fn accessory_info(&self) -> AccessorySnapshot {
        AccessorySnapshot {
            display_name: "Desk".to_string(),
            product_name: "Key Light".to_string(),
            serial_number: "ABC".to_string(),
            firmware_version: "1.0.3".to_string(),
            hardware_board_type: 53,
        }
    }

    fn identify(&self) {}

    fn refresh(&self) -> LightSnapshot {
        self.snapshot()
    }

    fn set_state(
        &self,
        _has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
    ) -> LightSnapshot {
        LightSnapshot {
            endpoint: "key-light.local:9123".to_string(),
            online: true,
            is_on: power,
            brightness,
            temperature,
            last_error: String::new(),
        }
    }

    fn snapshot(&self) -> LightSnapshot {
        LightSnapshot {
            endpoint: "key-light.local:9123".to_string(),
            online: true,
            is_on: true,
            brightness: 60,
            temperature: 250,
            last_error: String::new(),
        }
    }

    fn toggle(&self) -> LightSnapshot {
        let mut snapshot = self.snapshot();
        snapshot.is_on = false;
        snapshot
    }
}

#[test]
fn help_describes_the_daemon_owned_interface() {
    cargo_bin_cmd!("elgatobar")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Control the ElgatoBar user daemon",
        ))
        .stdout(predicate::str::contains("refresh"));
}

#[test]
fn direct_endpoint_arguments_are_no_longer_accepted() {
    cargo_bin_cmd!("elgatobar")
        .args(["state", "192.0.2.10"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn invalid_set_values_fail_before_connecting_to_dbus() {
    cargo_bin_cmd!("elgatobar")
        .args(["--json", "set", "--brightness", "2"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalidInput"));

    cargo_bin_cmd!("elgatobar")
        .args(["set"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("requires at least one"));
}

#[test]
fn json_parser_failures_are_valid_error_documents() {
    let output = cargo_bin_cmd!("elgatobar")
        .args(["--json", "set", "--brightness", "not-a-number"])
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
        .args(["--json", "state"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("connectivity"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_uses_the_public_dbus_contract() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD_MARKER}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli_uses_the_public_dbus_contract",
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
        .args(["--json", "state"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"brightness\":60"));
    Command::new(binary)
        .args(["set", "--on", "--brightness", "75", "--temperature", "200"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Brightness: 75%"));
    Command::new(binary)
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("Name: Desk"));
    Command::new(binary)
        .arg("toggle")
        .assert()
        .success()
        .stdout(predicate::str::contains("Power: off"));
    Command::new(binary)
        .arg("identify")
        .assert()
        .success()
        .stdout(predicate::str::contains("Identify request sent"));
}
