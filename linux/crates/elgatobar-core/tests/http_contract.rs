use std::{str::FromStr, time::Duration};

use elgatobar_core::{
    Brightness, DeviceEndpoint, ElgatoTemperature, LightState, LightTransport,
    ReqwestLightTransport, TransportError,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

fn endpoint(server: &MockServer) -> DeviceEndpoint {
    DeviceEndpoint::from_str(&server.uri()).unwrap()
}

fn fixture(relative: &str) -> serde_json::Value {
    match relative {
        "lights-response.json" => serde_json::from_str(include_str!(
            "../../../../shared/api-fixtures/lights-response.json"
        )),
        "lights-put-request.json" => serde_json::from_str(include_str!(
            "../../../../shared/api-fixtures/lights-put-request.json"
        )),
        "lights-put-response.json" => serde_json::from_str(include_str!(
            "../../../../shared/api-fixtures/lights-put-response.json"
        )),
        "lights-empty-response.json" => serde_json::from_str(include_str!(
            "../../../../shared/api-fixtures/lights-empty-response.json"
        )),
        _ => panic!("unknown fixture {relative}"),
    }
    .unwrap()
}

fn response_state() -> serde_json::Value {
    fixture("lights-response.json")
}

async fn stalled_body_endpoint() -> DeviceEndpoint {
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
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    DeviceEndpoint::new(address.ip().to_string(), address.port()).unwrap()
}

#[tokio::test]
async fn adapter_uses_exact_paths_methods_and_full_put_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_state()))
        .expect(1)
        .mount(&server)
        .await;
    let desired = LightState {
        is_on: false,
        brightness: Brightness::try_from(75).unwrap(),
        temperature: ElgatoTemperature::try_from(200).unwrap(),
    };
    Mock::given(method("PUT"))
        .and(path("/elgato/lights"))
        .and(body_json(fixture("lights-put-request.json")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("lights-put-response.json")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/elgato/identify"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let transport = ReqwestLightTransport::new().unwrap();
    let endpoint = endpoint(&server);
    assert!(transport.light_state(&endpoint).await.unwrap().is_on);
    assert_eq!(
        transport.set_light_state(&endpoint, desired).await.unwrap(),
        desired
    );
    transport.identify(&endpoint).await.unwrap();
}

#[tokio::test]
async fn state_reads_only_the_first_light() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "numberOfLights": 2,
            "lights": [
                {"on": 1, "brightness": 60, "temperature": 250},
                {"on": 0, "brightness": 25, "temperature": 344}
            ]
        })))
        .mount(&server)
        .await;

    let state = ReqwestLightTransport::new()
        .unwrap()
        .light_state(&endpoint(&server))
        .await
        .unwrap();
    assert!(state.is_on);
    assert_eq!(state.brightness.get(), 60);
    assert_eq!(state.temperature.get(), 250);
}

#[tokio::test]
async fn accessory_info_decodes_the_recorded_fixture() {
    let server = MockServer::start().await;
    let fixture = include_str!("../../../../shared/api-fixtures/accessory-info-response.json");
    Mock::given(method("GET"))
        .and(path("/elgato/accessory-info"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let info = ReqwestLightTransport::new()
        .unwrap()
        .accessory_info(&endpoint(&server))
        .await
        .unwrap();
    assert_eq!(info.best_name(), "Desk Key Light");
    assert_eq!(info.serial_number, "CW12A1A00001");
}

#[tokio::test]
async fn adapter_maps_malformed_empty_status_redirect_timeout_and_unreachable_errors() {
    let malformed = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("not-json", "application/json"))
        .mount(&malformed)
        .await;
    assert!(matches!(
        ReqwestLightTransport::new()
            .unwrap()
            .light_state(&endpoint(&malformed))
            .await,
        Err(TransportError::MalformedResponse { .. })
    ));

    let empty = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("lights-empty-response.json")),
        )
        .mount(&empty)
        .await;
    assert!(matches!(
        ReqwestLightTransport::new()
            .unwrap()
            .light_state(&endpoint(&empty))
            .await,
        Err(TransportError::InvalidResponse { .. })
    ));

    let status = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(503).set_body_string("warming up"))
        .mount(&status)
        .await;
    assert!(matches!(
        ReqwestLightTransport::new()
            .unwrap()
            .light_state(&endpoint(&status))
            .await,
        Err(TransportError::HttpStatus { status: 503, .. })
    ));

    let redirect = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/elsewhere"))
        .mount(&redirect)
        .await;
    assert!(matches!(
        ReqwestLightTransport::new()
            .unwrap()
            .light_state(&endpoint(&redirect))
            .await,
        Err(TransportError::HttpStatus { status: 302, .. })
    ));

    let slow = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/elgato/lights"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(response_state()),
        )
        .mount(&slow)
        .await;
    assert!(matches!(
        ReqwestLightTransport::with_timeout(Duration::from_millis(25))
            .unwrap()
            .light_state(&endpoint(&slow))
            .await,
        Err(TransportError::Timeout { .. })
    ));

    let unreachable = DeviceEndpoint::from_str("127.0.0.1:1").unwrap();
    assert!(matches!(
        ReqwestLightTransport::with_timeout(Duration::from_millis(100))
            .unwrap()
            .light_state(&unreachable)
            .await,
        Err(TransportError::Connectivity { .. })
    ));
}

#[tokio::test]
async fn timeout_while_reading_body_remains_a_connectivity_error() {
    let endpoint = stalled_body_endpoint().await;
    let error = ReqwestLightTransport::with_timeout(Duration::from_millis(50))
        .unwrap()
        .light_state(&endpoint)
        .await
        .unwrap_err();
    assert!(matches!(error, TransportError::Timeout { .. }));
    assert!(error.is_connectivity());
}
