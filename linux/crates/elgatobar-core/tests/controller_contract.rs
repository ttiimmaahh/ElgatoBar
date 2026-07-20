use std::{str::FromStr, sync::Mutex};

use async_trait::async_trait;
use elgatobar_core::{
    AccessoryInfo, ApplicationController, Brightness, CommandResult, DeviceCommand, DeviceEndpoint,
    ElgatoTemperature, LightState, LightTransport, SetLightState, TransportError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    Get,
    Put(LightState),
    Info,
    Identify,
}

struct FakeTransport {
    state: Mutex<LightState>,
    calls: Mutex<Vec<Call>>,
}

impl FakeTransport {
    fn new(state: LightState) -> Self {
        Self {
            state: Mutex::new(state),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl LightTransport for &FakeTransport {
    async fn accessory_info(&self, _: &DeviceEndpoint) -> Result<AccessoryInfo, TransportError> {
        self.calls.lock().unwrap().push(Call::Info);
        Ok(AccessoryInfo {
            product_name: "Key Light".to_string(),
            hardware_board_type: 1,
            firmware_build_number: 2,
            firmware_version: "1.0".to_string(),
            serial_number: "ABC".to_string(),
            display_name: None,
            features: None,
            wifi_info: None,
        })
    }

    async fn light_state(&self, _: &DeviceEndpoint) -> Result<LightState, TransportError> {
        self.calls.lock().unwrap().push(Call::Get);
        Ok(*self.state.lock().unwrap())
    }

    async fn set_light_state(
        &self,
        _: &DeviceEndpoint,
        state: LightState,
    ) -> Result<LightState, TransportError> {
        self.calls.lock().unwrap().push(Call::Put(state));
        *self.state.lock().unwrap() = state;
        Ok(state)
    }

    async fn identify(&self, _: &DeviceEndpoint) -> Result<(), TransportError> {
        self.calls.lock().unwrap().push(Call::Identify);
        Ok(())
    }
}

fn state() -> LightState {
    LightState {
        is_on: true,
        brightness: Brightness::try_from(60).unwrap(),
        temperature: ElgatoTemperature::try_from(250).unwrap(),
    }
}

#[tokio::test]
async fn toggle_reads_then_writes_full_state_preserving_brightness_and_temperature() {
    let transport = FakeTransport::new(state());
    let controller = ApplicationController::new(&transport);
    let endpoint = DeviceEndpoint::from_str("192.0.2.10").unwrap();

    let result = controller
        .execute(&endpoint, DeviceCommand::Toggle)
        .await
        .unwrap();
    let expected = LightState {
        is_on: false,
        ..state()
    };
    assert_eq!(result, CommandResult::State { state: expected });
    assert_eq!(transport.calls(), vec![Call::Get, Call::Put(expected)]);
}

#[tokio::test]
async fn set_reads_then_updates_only_requested_fields() {
    let transport = FakeTransport::new(state());
    let controller = ApplicationController::new(&transport);
    let endpoint = DeviceEndpoint::from_str("192.0.2.10").unwrap();
    let brightness = Brightness::try_from(75).unwrap();

    let result = controller
        .execute(
            &endpoint,
            DeviceCommand::Set(SetLightState {
                brightness: Some(brightness),
                ..SetLightState::default()
            }),
        )
        .await
        .unwrap();
    let expected = LightState {
        brightness,
        ..state()
    };
    assert_eq!(result, CommandResult::State { state: expected });
    assert_eq!(transport.calls(), vec![Call::Get, Call::Put(expected)]);
}

#[tokio::test]
async fn info_state_and_identify_delegate_through_the_same_public_interface() {
    let transport = FakeTransport::new(state());
    let controller = ApplicationController::new(&transport);
    let endpoint = DeviceEndpoint::from_str("192.0.2.10").unwrap();

    assert!(matches!(
        controller
            .execute(&endpoint, DeviceCommand::AccessoryInfo)
            .await
            .unwrap(),
        CommandResult::AccessoryInfo { .. }
    ));
    assert_eq!(
        controller
            .execute(&endpoint, DeviceCommand::State)
            .await
            .unwrap(),
        CommandResult::State { state: state() }
    );
    assert_eq!(
        controller
            .execute(&endpoint, DeviceCommand::Identify)
            .await
            .unwrap(),
        CommandResult::Identified
    );
    assert_eq!(
        transport.calls(),
        vec![Call::Info, Call::Get, Call::Identify]
    );
}
