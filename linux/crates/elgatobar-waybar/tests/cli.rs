use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};

use elgatobar_waybar::WaybarOutput;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_elgatobar-waybar"))
}

#[test]
fn help_documents_continuous_and_action_modes() {
    let output = binary().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for expected in ["watch", "toggle-all", "refresh-all"] {
        assert!(stdout.contains(expected), "missing {expected} in {stdout}");
    }
}

#[test]
fn action_fails_when_the_typed_session_bus_is_unreachable() {
    let status = binary()
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/tmp/elgatobar-no-such-bus",
        )
        .arg("toggle-all")
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn default_watch_mode_immediately_emits_valid_unavailable_json() {
    let mut child = binary()
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/tmp/elgatobar-no-such-bus",
        )
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();

    let output: WaybarOutput = serde_json::from_str(&line).unwrap();
    assert_eq!(output.alt, "unavailable");
    assert_eq!(output.class, ["unavailable"]);
}
