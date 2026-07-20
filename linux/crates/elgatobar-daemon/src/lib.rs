mod storage;

pub use storage::{FileDeviceStore, StoragePaths, load_settings, save_settings};

use std::{str::FromStr, sync::Arc, time::Duration};

use elgatobar_core::{
    Brightness, DeviceEndpoint, DeviceOperationResult as CoreOperationResult,
    DeviceSnapshot as CoreDeviceSnapshot, ElgatoTemperature, ManagerError, MultiDeviceController,
    OperationStatus, ReqwestLightTransport, SetLightState, SettingsDocument, TransportError,
};
use elgatobar_dbus::{
    AccessorySnapshot, DeviceSnapshot, INTERFACE_NAME, LightSnapshot, OBJECT_PATH, OperationResult,
    SERVICE_NAME,
};
use zbus::{DBusError, interface, object_server::SignalEmitter};

type Manager = MultiDeviceController<ReqwestLightTransport, FileDeviceStore>;

#[derive(Debug, DBusError)]
#[zbus(prefix = "io.github.ttiimmaahh.ElgatoBar1.Error")]
pub enum ServiceError {
    Connectivity(String),
    Protocol(String),
    InvalidInput(String),
    Storage(String),
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

impl From<ManagerError> for ServiceError {
    fn from(error: ManagerError) -> Self {
        match error {
            ManagerError::InvalidInput(message)
            | ManagerError::NotFound(message)
            | ManagerError::Duplicate(message) => Self::InvalidInput(message),
            ManagerError::Storage(message) => Self::Storage(message),
            ManagerError::Transport(error) => error.into(),
        }
    }
}

#[derive(Clone)]
pub struct ControlService {
    manager: Arc<Manager>,
}

impl ControlService {
    pub fn new(paths: &StoragePaths, timeout: Duration) -> Result<Self, ManagerError> {
        let transport = ReqwestLightTransport::with_timeout(timeout)?;
        let store = FileDeviceStore::new(paths.device_file());
        Ok(Self {
            manager: Arc::new(MultiDeviceController::load(transport, store)?),
        })
    }

    async fn only_device_id(&self) -> Result<String, ServiceError> {
        let snapshots = self.manager.snapshots().await;
        match snapshots.as_slice() {
            [snapshot] => Ok(snapshot.device_id.clone()),
            [] => Err(ServiceError::InvalidInput(
                "no device is configured; use `elgatobar devices add ENDPOINT`".to_string(),
            )),
            _ => Err(ServiceError::InvalidInput(
                "multiple devices are configured; use a manager method with a stable device ID"
                    .to_string(),
            )),
        }
    }

    async fn emit_snapshots(&self, emitter: &SignalEmitter<'_>) {
        let snapshots = self
            .manager
            .snapshots()
            .await
            .into_iter()
            .map(device_snapshot)
            .collect::<Vec<_>>();
        let _ = emitter.devices_changed(snapshots.clone()).await;
        if let [snapshot] = snapshots.as_slice() {
            let _ = emitter
                .state_changed(dbus_to_legacy(snapshot.clone()))
                .await;
        }
    }

    async fn legacy_result(
        &self,
        result: CoreOperationResult,
    ) -> Result<LightSnapshot, ServiceError> {
        match result.status {
            OperationStatus::Succeeded => Ok(legacy_snapshot(result.snapshot)),
            OperationStatus::Failed => Err(operation_error(&result)),
            OperationStatus::SkippedOffline => Err(ServiceError::Connectivity(result.error)),
        }
    }

    pub async fn poll(&self, emitter: &SignalEmitter<'_>) {
        let _ = self.manager.refresh_all().await;
        self.emit_snapshots(emitter).await;
    }
}

fn device_snapshot(snapshot: CoreDeviceSnapshot) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: snapshot.device_id,
        name: snapshot.name,
        endpoint: snapshot.endpoint,
        online: snapshot.online,
        has_state: snapshot.has_state,
        is_on: snapshot.is_on,
        brightness: snapshot.brightness,
        temperature: snapshot.temperature,
        consecutive_failures: snapshot.consecutive_failures,
        last_error: snapshot.last_error,
    }
}

fn legacy_snapshot(snapshot: CoreDeviceSnapshot) -> LightSnapshot {
    LightSnapshot {
        endpoint: snapshot.endpoint,
        online: snapshot.online,
        is_on: snapshot.is_on,
        brightness: snapshot.brightness,
        temperature: snapshot.temperature,
        last_error: snapshot.last_error,
    }
}

fn operation_result(result: CoreOperationResult) -> OperationResult {
    OperationResult {
        device_id: result.device_id,
        status: match result.status {
            OperationStatus::Succeeded => "succeeded",
            OperationStatus::Failed => "failed",
            OperationStatus::SkippedOffline => "skipped-offline",
        }
        .to_string(),
        snapshot: device_snapshot(result.snapshot),
        error_kind: result.error_kind,
        error: result.error,
    }
}

fn dbus_to_legacy(snapshot: DeviceSnapshot) -> LightSnapshot {
    LightSnapshot {
        endpoint: snapshot.endpoint,
        online: snapshot.online,
        is_on: snapshot.is_on,
        brightness: snapshot.brightness,
        temperature: snapshot.temperature,
        last_error: snapshot.last_error,
    }
}

fn operation_error(result: &CoreOperationResult) -> ServiceError {
    if result.error_kind == "connectivity" {
        ServiceError::Connectivity(result.error.clone())
    } else {
        ServiceError::Protocol(result.error.clone())
    }
}

fn state_update(
    has_power: bool,
    power: bool,
    brightness: u8,
    temperature: u16,
) -> Result<SetLightState, ServiceError> {
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
    Ok(SetLightState {
        power: has_power.then_some(power),
        brightness,
        temperature,
    })
}

#[interface(name = "io.github.ttiimmaahh.ElgatoBar1")]
impl ControlService {
    async fn add_device(
        &self,
        endpoint: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<DeviceSnapshot, ServiceError> {
        let endpoint = DeviceEndpoint::from_str(endpoint)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let result = device_snapshot(self.manager.add(endpoint).await?);
        self.emit_snapshots(&emitter).await;
        Ok(result)
    }

    async fn remove_device(
        &self,
        device_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<DeviceSnapshot, ServiceError> {
        let result = device_snapshot(self.manager.remove(device_id).await?);
        self.emit_snapshots(&emitter).await;
        Ok(result)
    }

    async fn list_devices(&self) -> Vec<DeviceSnapshot> {
        self.manager
            .snapshots()
            .await
            .into_iter()
            .map(device_snapshot)
            .collect()
    }

    async fn device_snapshot(&self, device_id: &str) -> Result<DeviceSnapshot, ServiceError> {
        self.manager
            .snapshot(device_id)
            .await
            .map(device_snapshot)
            .map_err(Into::into)
    }

    async fn refresh_device(
        &self,
        device_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<OperationResult, ServiceError> {
        let result = operation_result(self.manager.refresh(device_id).await?);
        self.emit_snapshots(&emitter).await;
        Ok(result)
    }

    async fn refresh_all(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Vec<OperationResult> {
        let results = self
            .manager
            .refresh_all()
            .await
            .into_iter()
            .map(operation_result)
            .collect();
        self.emit_snapshots(&emitter).await;
        results
    }

    async fn set_device_state(
        &self,
        device_id: &str,
        has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<OperationResult, ServiceError> {
        let update = state_update(has_power, power, brightness, temperature)?;
        let result = operation_result(self.manager.set(device_id, update).await?);
        self.emit_snapshots(&emitter).await;
        Ok(result)
    }

    async fn toggle_device(
        &self,
        device_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<OperationResult, ServiceError> {
        let result = operation_result(self.manager.toggle(device_id).await?);
        self.emit_snapshots(&emitter).await;
        Ok(result)
    }

    async fn identify_device(&self, device_id: &str) -> Result<OperationResult, ServiceError> {
        self.manager
            .identify(device_id)
            .await
            .map(operation_result)
            .map_err(Into::into)
    }

    async fn toggle_all(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Vec<OperationResult> {
        let results = self
            .manager
            .toggle_all()
            .await
            .into_iter()
            .map(operation_result)
            .collect();
        self.emit_snapshots(&emitter).await;
        results
    }

    // Compatibility methods: valid only when the inventory contains exactly one device.
    async fn accessory_info(&self) -> Result<AccessorySnapshot, ServiceError> {
        let id = self.only_device_id().await?;
        let accessory = self.manager.accessory_info(&id).await?;
        Ok(AccessorySnapshot {
            display_name: accessory.best_name().to_string(),
            product_name: accessory.product_name,
            serial_number: accessory.serial_number,
            firmware_version: accessory.firmware_version,
            hardware_board_type: accessory.hardware_board_type,
        })
    }

    async fn snapshot(&self) -> LightSnapshot {
        match self.only_device_id().await {
            Ok(id) => self
                .manager
                .snapshot(&id)
                .await
                .map(legacy_snapshot)
                .unwrap_or_else(|error| {
                    LightSnapshot::unavailable(String::new(), error.to_string())
                }),
            Err(error) => LightSnapshot::unavailable(String::new(), error.to_string()),
        }
    }

    async fn refresh(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let id = self.only_device_id().await?;
        let result = self.manager.refresh(&id).await?;
        self.emit_snapshots(&emitter).await;
        self.legacy_result(result).await
    }

    async fn set_state(
        &self,
        has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let id = self.only_device_id().await?;
        let result = self
            .manager
            .set(
                &id,
                state_update(has_power, power, brightness, temperature)?,
            )
            .await?;
        self.emit_snapshots(&emitter).await;
        self.legacy_result(result).await
    }

    async fn toggle(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<LightSnapshot, ServiceError> {
        let id = self.only_device_id().await?;
        let result = self.manager.toggle(&id).await?;
        self.emit_snapshots(&emitter).await;
        self.legacy_result(result).await
    }

    async fn identify(&self) -> Result<(), ServiceError> {
        let id = self.only_device_id().await?;
        let result = self.manager.identify(&id).await?;
        if result.status == OperationStatus::Succeeded {
            Ok(())
        } else {
            Err(operation_error(&result))
        }
    }

    #[zbus(signal)]
    async fn state_changed(
        signal_emitter: &SignalEmitter<'_>,
        snapshot: LightSnapshot,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn devices_changed(
        signal_emitter: &SignalEmitter<'_>,
        snapshots: Vec<DeviceSnapshot>,
    ) -> zbus::Result<()>;
}

pub async fn serve(
    paths: StoragePaths,
    timeout: Duration,
    settings: SettingsDocument,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = ControlService::new(&paths, timeout)?;
    let poller = service.clone();
    let connection = zbus::connection::Builder::session()?
        .name(SERVICE_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;
    let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;

    poller.poll(&emitter).await;
    let mut interval =
        tokio::time::interval(Duration::from_secs(settings.refresh_interval_seconds));
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
