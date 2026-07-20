use std::{str::FromStr, sync::Arc, time::Duration};

use elgatobar_core::{
    ApplicationController, Brightness, CommandResult, DeviceCommand, DeviceEndpoint,
    ElgatoTemperature, ReqwestLightTransport, SetLightState, TransportError,
};
use elgatobar_dbus::{AccessorySnapshot, INTERFACE_NAME, LightSnapshot, OBJECT_PATH, SERVICE_NAME};
use tokio::sync::{Mutex, RwLock};
use zbus::{DBusError, interface, object_server::SignalEmitter};

#[derive(Debug, DBusError)]
#[zbus(prefix = "io.github.ttiimmaahh.ElgatoBar1.Error")]
pub enum ServiceError {
    Connectivity(String),
    Protocol(String),
    InvalidInput(String),
}

impl From<TransportError> for ServiceError {
    fn from(error: TransportError) -> Self {
        if error.is_connectivity() {
            Self::Connectivity(error.to_string())
        } else {
            Self::Protocol(error.to_string())
        }
    }
}

struct Inner {
    endpoint: DeviceEndpoint,
    controller: ApplicationController<ReqwestLightTransport>,
    snapshot: RwLock<LightSnapshot>,
    operation: Mutex<()>,
}

#[derive(Clone)]
pub struct ControlService {
    inner: Arc<Inner>,
}

impl ControlService {
    pub fn new(endpoint: DeviceEndpoint, timeout: Duration) -> Result<Self, TransportError> {
        let transport = ReqwestLightTransport::with_timeout(timeout)?;
        let snapshot = LightSnapshot::unavailable(endpoint.to_string(), "waiting for first poll");
        Ok(Self {
            inner: Arc::new(Inner {
                endpoint,
                controller: ApplicationController::new(transport),
                snapshot: RwLock::new(snapshot),
                operation: Mutex::new(()),
            }),
        })
    }

    async fn run_state_command(
        &self,
        command: DeviceCommand,
    ) -> Result<LightSnapshot, ServiceError> {
        let _operation = self.inner.operation.lock().await;
        match self
            .inner
            .controller
            .execute(&self.inner.endpoint, command)
            .await
        {
            Ok(CommandResult::State { state }) => {
                let snapshot = LightSnapshot {
                    endpoint: self.inner.endpoint.to_string(),
                    online: true,
                    is_on: state.is_on,
                    brightness: state.brightness.get(),
                    temperature: state.temperature.get(),
                    last_error: String::new(),
                };
                *self.inner.snapshot.write().await = snapshot.clone();
                Ok(snapshot)
            }
            Ok(_) => Err(ServiceError::Protocol(
                "device command returned an unexpected response".to_string(),
            )),
            Err(error) => {
                let mut snapshot = self.inner.snapshot.write().await;
                snapshot.online = false;
                snapshot.last_error = error.to_string();
                Err(error.into())
            }
        }
    }

    async fn emit_snapshot(
        &self,
        emitter: &SignalEmitter<'_>,
        result: Result<LightSnapshot, ServiceError>,
    ) -> Result<LightSnapshot, ServiceError> {
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let snapshot = self.inner.snapshot.read().await.clone();
                let _ = emitter.state_changed(snapshot).await;
                return Err(error);
            }
        };
        let _ = emitter.state_changed(snapshot.clone()).await;
        Ok(snapshot)
    }

    pub async fn poll(&self, emitter: &SignalEmitter<'_>) {
        let result = self.run_state_command(DeviceCommand::State).await;
        let _ = self.emit_snapshot(emitter, result).await;
    }
}

#[interface(name = "io.github.ttiimmaahh.ElgatoBar1")]
impl ControlService {
    async fn accessory_info(&self) -> Result<AccessorySnapshot, ServiceError> {
        let _operation = self.inner.operation.lock().await;
        match self
            .inner
            .controller
            .execute(&self.inner.endpoint, DeviceCommand::AccessoryInfo)
            .await?
        {
            CommandResult::AccessoryInfo { accessory } => Ok(AccessorySnapshot {
                display_name: accessory.best_name().to_string(),
                product_name: accessory.product_name,
                serial_number: accessory.serial_number,
                firmware_version: accessory.firmware_version,
                hardware_board_type: accessory.hardware_board_type,
            }),
            _ => Err(ServiceError::Protocol(
                "accessory command returned an unexpected response".to_string(),
            )),
        }
    }

    async fn snapshot(&self) -> LightSnapshot {
        self.inner.snapshot.read().await.clone()
    }

    async fn refresh(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let result = self.run_state_command(DeviceCommand::State).await;
        self.emit_snapshot(&emitter, result).await
    }

    async fn set_state(
        &self,
        has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let brightness = if brightness == 0 {
            None
        } else {
            Some(
                Brightness::try_from(brightness)
                    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?,
            )
        };
        let temperature = if temperature == 0 {
            None
        } else {
            Some(
                ElgatoTemperature::try_from(temperature)
                    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?,
            )
        };
        if !has_power && brightness.is_none() && temperature.is_none() {
            return Err(ServiceError::InvalidInput(
                "set requires at least one state field".to_string(),
            ));
        }
        let result = self
            .run_state_command(DeviceCommand::Set(SetLightState {
                power: has_power.then_some(power),
                brightness,
                temperature,
            }))
            .await;
        self.emit_snapshot(&emitter, result).await
    }

    async fn toggle(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let result = self.run_state_command(DeviceCommand::Toggle).await;
        self.emit_snapshot(&emitter, result).await
    }

    async fn identify(&self) -> Result<(), ServiceError> {
        let _operation = self.inner.operation.lock().await;
        self.inner
            .controller
            .execute(&self.inner.endpoint, DeviceCommand::Identify)
            .await?;
        Ok(())
    }

    #[zbus(signal)]
    async fn state_changed(
        signal_emitter: &SignalEmitter<'_>,
        snapshot: LightSnapshot,
    ) -> zbus::Result<()>;
}

pub async fn serve(
    endpoint: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = DeviceEndpoint::from_str(endpoint)?;
    let service = ControlService::new(endpoint, timeout)?;
    let poller = service.clone();
    let connection = zbus::connection::Builder::session()?
        .name(SERVICE_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;
    let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;

    poller.poll(&emitter).await;
    let mut interval = tokio::time::interval(poll_interval);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => poller.poll(&emitter).await,
            result = tokio::signal::ctrl_c() => {
                result?;
                break;
            }
        }
    }
    connection.graceful_shutdown().await;
    Ok(())
}

#[must_use]
pub fn contract_names() -> (&'static str, &'static str, &'static str) {
    (SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
}
