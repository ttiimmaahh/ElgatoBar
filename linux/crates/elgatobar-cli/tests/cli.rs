use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

async fn stalled_body_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 128\r\nConnection: close\r\n\r\n{\"numberOfLights\":1,\"lights\":[",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    });
    address.to_string()
}

#[tokio::test]
async fn state_supports_json_and_human_output() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 1, "brightness": 60, "temperature": 250}]
        })))
        .expect(2)
        .mount(&server)
        .await;

    cargo_bin_cmd!("elgatobar")
        .args(["--json", "state", &server.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\":\"state\""))
        .stdout(predicate::str::contains("\"brightness\":60"));

    cargo_bin_cmd!("elgatobar")
        .args(["state", &server.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Power: on"))
        .stdout(predicate::str::contains("Brightness: 60%"));
}

#[tokio::test]
async fn info_set_toggle_and_identify_execute_their_public_workflows() {
    let server = MockServer::start().await;
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
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 1, "brightness": 60, "temperature": 250}]
        })))
        .expect(2)
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
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .and(body_json(json!({
            "lights": [{"on": 0, "brightness": 60, "temperature": 250}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 0, "brightness": 60, "temperature": 250}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/elgato/identify"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    cargo_bin_cmd!("elgatobar")
        .args(["--json", "info", &server.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"serialNumber\":\"ABC\""));
    cargo_bin_cmd!("elgatobar")
        .args(["set", &server.uri(), "--brightness", "75"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Brightness: 75%"));
    cargo_bin_cmd!("elgatobar")
        .args(["--json", "toggle", &server.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"isOn\":false"));
    cargo_bin_cmd!("elgatobar")
        .args(["identify", &server.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Identify request sent"));
}

#[test]
fn invalid_input_uses_exit_two_and_structured_error() {
    cargo_bin_cmd!("elgatobar")
        .args(["--json", "set", "192.0.2.10", "--brightness", "2"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalidInput"));

    cargo_bin_cmd!("elgatobar")
        .args(["set", "https://192.0.2.10", "--on"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("plain http"));
}

#[test]
fn json_parser_failures_use_exit_two_and_valid_error_documents() {
    for arguments in [
        vec![
            "--json",
            "--timeout-ms",
            "not-a-number",
            "state",
            "127.0.0.1",
        ],
        vec!["--json", "set", "127.0.0.1", "--brightness", "not-a-number"],
        vec!["--json", "state"],
    ] {
        let output = cargo_bin_cmd!("elgatobar")
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let document: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(document["error"]["kind"], "invalidInput");
        assert!(document["error"]["message"].is_string());
    }
}

#[test]
fn help_stays_human_readable_and_successful_with_or_without_json_flag() {
    cargo_bin_cmd!("elgatobar")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
    cargo_bin_cmd!("elgatobar")
        .args(["--json", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn connectivity_failure_uses_exit_three() {
    cargo_bin_cmd!("elgatobar")
        .args(["--timeout-ms", "100", "state", "127.0.0.1:1"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("could not connect"));
}

#[tokio::test]
async fn body_read_timeout_uses_exit_three_and_structured_connectivity_error() {
    let endpoint = stalled_body_endpoint().await;
    let output = cargo_bin_cmd!("elgatobar")
        .args(["--json", "--timeout-ms", "50", "state", endpoint.as_str()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let document: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(document["error"]["kind"], "connectivity");
    assert!(
        document["error"]["message"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
}

#[tokio::test]
async fn malformed_response_uses_exit_four() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("bad", "application/json"))
        .mount(&server)
        .await;

    cargo_bin_cmd!("elgatobar")
        .args(["--json", "state", &server.uri()])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("malformed"));
}

#[tokio::test]
async fn direct_requests_do_not_inherit_http_proxy() {
    let target = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "numberOfLights": 1,
            "lights": [{"on": 0, "brightness": 50, "temperature": 200}]
        })))
        .expect(1)
        .mount(&target)
        .await;
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(502).set_body_string("proxy used"))
        .expect(0)
        .mount(&proxy)
        .await;

    cargo_bin_cmd!("elgatobar")
        .env("HTTP_PROXY", proxy.uri())
        .env("http_proxy", proxy.uri())
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .args(["state", &target.uri()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Power: off"));
}
