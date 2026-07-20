use serde::Serialize;

use crate::{
    AccessoryInfo, Brightness, DeviceEndpoint, ElgatoTemperature, LightState, LightTransport,
    TransportError,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetLightState {
    pub power: Option<bool>,
    pub brightness: Option<Brightness>,
    pub temperature: Option<ElgatoTemperature>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCommand {
    AccessoryInfo,
    State,
    Set(SetLightState),
    Toggle,
    Identify,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommandResult {
    AccessoryInfo { accessory: AccessoryInfo },
    State { state: LightState },
    Identified,
}

pub struct ApplicationController<T> {
    transport: T,
}

impl<T> ApplicationController<T>
where
    T: LightTransport,
{
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub async fn execute(
        &self,
        endpoint: &DeviceEndpoint,
        command: DeviceCommand,
    ) -> Result<CommandResult, TransportError> {
        match command {
            DeviceCommand::AccessoryInfo => {
                let accessory = self.transport.accessory_info(endpoint).await?;
                Ok(CommandResult::AccessoryInfo { accessory })
            }
            DeviceCommand::State => {
                let state = self.transport.light_state(endpoint).await?;
                Ok(CommandResult::State { state })
            }
            DeviceCommand::Set(update) => {
                let current = self.transport.light_state(endpoint).await?;
                let desired = LightState {
                    is_on: update.power.unwrap_or(current.is_on),
                    brightness: update.brightness.unwrap_or(current.brightness),
                    temperature: update.temperature.unwrap_or(current.temperature),
                };
                let state = self.transport.set_light_state(endpoint, desired).await?;
                Ok(CommandResult::State { state })
            }
            DeviceCommand::Toggle => {
                let current = self.transport.light_state(endpoint).await?;
                let desired = LightState {
                    is_on: !current.is_on,
                    ..current
                };
                let state = self.transport.set_light_state(endpoint, desired).await?;
                Ok(CommandResult::State { state })
            }
            DeviceCommand::Identify => {
                self.transport.identify(endpoint).await?;
                Ok(CommandResult::Identified)
            }
        }
    }
}
