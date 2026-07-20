use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    AccessoryInfo, ApplicationController, DeviceCommand, DeviceEndpoint, DeviceIdentity,
    DocumentName, LightState, LightTransport, PersistedDevice, SetLightState, TransportError,
};

pub const MAX_CONCURRENT_OPERATIONS: usize = 8;
pub const REFRESH_RETRY_DELAY: Duration = Duration::from_millis(500);
pub const DEVICE_STORAGE_VERSION: u32 = 1;
pub const SETTINGS_STORAGE_VERSION: u32 = 1;
pub const DEFAULT_REFRESH_INTERVAL_SECONDS: u64 = 5;
pub const SUPPORTED_REFRESH_INTERVAL_SECONDS: [u64; 4] = [3, 5, 10, 30];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStorageDocument {
    version: u32,
    pub devices: Vec<PersistedDevice>,
}

impl DeviceStorageDocument {
    #[must_use]
    pub fn new(devices: Vec<PersistedDevice>) -> Self {
        Self {
            version: DEVICE_STORAGE_VERSION,
            devices,
        }
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDeviceStorageDocument {
    version: u32,
    devices: Vec<PersistedDevice>,
}

impl<'de> Deserialize<'de> for DeviceStorageDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawDeviceStorageDocument::deserialize(deserializer)?;
        if raw.version != DEVICE_STORAGE_VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported device storage version {}; expected {}; upgrade ElgatoBar before retrying",
                raw.version, DEVICE_STORAGE_VERSION
            )));
        }
        Ok(Self::new(raw.devices))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDocument {
    version: u32,
    pub refresh_interval_seconds: u64,
}

impl SettingsDocument {
    pub fn new(refresh_interval_seconds: u64) -> Result<Self, ManagerError> {
        if !SUPPORTED_REFRESH_INTERVAL_SECONDS.contains(&refresh_interval_seconds) {
            return Err(ManagerError::InvalidInput(format!(
                "refresh interval must be one of 3, 5, 10, or 30 seconds; got {refresh_interval_seconds}"
            )));
        }
        Ok(Self {
            version: SETTINGS_STORAGE_VERSION,
            refresh_interval_seconds,
        })
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self::new(DEFAULT_REFRESH_INTERVAL_SECONDS).expect("default interval is supported")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSettingsDocument {
    version: u32,
    refresh_interval_seconds: u64,
}

impl<'de> Deserialize<'de> for SettingsDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawSettingsDocument::deserialize(deserializer)?;
        if raw.version != SETTINGS_STORAGE_VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported settings version {}; expected {}; upgrade ElgatoBar before retrying",
                raw.version, SETTINGS_STORAGE_VERSION
            )));
        }
        Self::new(raw.refresh_interval_seconds).map_err(serde::de::Error::custom)
    }
}

pub trait DeviceStore: Send + Sync {
    fn load(&self) -> Result<DeviceStorageDocument, String>;
    fn save(&self, document: &DeviceStorageDocument) -> Result<(), String>;
}

pub trait RetryClock: Send + Sync {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioRetryClock;

impl RetryClock for TokioRetryClock {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub device_id: String,
    pub name: String,
    pub endpoint: String,
    pub online: bool,
    pub has_state: bool,
    pub is_on: bool,
    pub brightness: u8,
    pub temperature: u16,
    pub consecutive_failures: u32,
    pub last_error: String,
}

impl DeviceSnapshot {
    fn waiting(device: &PersistedDevice) -> Self {
        Self {
            device_id: device.identity.canonical_id(),
            name: device.name.to_string(),
            endpoint: device.endpoint.to_string(),
            online: false,
            has_state: false,
            is_on: false,
            brightness: 0,
            temperature: 0,
            consecutive_failures: 0,
            last_error: "waiting for first successful refresh".to_string(),
        }
    }

    fn apply_success(&mut self, state: LightState) {
        self.online = true;
        self.has_state = true;
        self.is_on = state.is_on;
        self.brightness = state.brightness.get();
        self.temperature = state.temperature.get();
        self.consecutive_failures = 0;
        self.last_error.clear();
    }

    fn apply_refresh_failure(&mut self, error: &TransportError) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= 2 {
            self.online = false;
        }
        self.last_error = error.to_string();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatus {
    Succeeded,
    Failed,
    SkippedOffline,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOperationResult {
    pub device_id: String,
    pub status: OperationStatus,
    pub snapshot: DeviceSnapshot,
    pub error_kind: String,
    pub error: String,
}

impl DeviceOperationResult {
    fn succeeded(snapshot: DeviceSnapshot) -> Self {
        Self {
            device_id: snapshot.device_id.clone(),
            status: OperationStatus::Succeeded,
            snapshot,
            error_kind: String::new(),
            error: String::new(),
        }
    }

    fn failed(snapshot: DeviceSnapshot, error: &TransportError) -> Self {
        Self {
            device_id: snapshot.device_id.clone(),
            status: OperationStatus::Failed,
            snapshot,
            error_kind: if error.is_connectivity() {
                "connectivity".to_string()
            } else {
                "protocol".to_string()
            },
            error: error.to_string(),
        }
    }

    fn skipped(snapshot: DeviceSnapshot) -> Self {
        Self {
            device_id: snapshot.device_id.clone(),
            status: OperationStatus::SkippedOffline,
            error: snapshot.last_error.clone(),
            snapshot,
            error_kind: "offline".to_string(),
        }
    }

    #[must_use]
    pub fn is_failure(&self) -> bool {
        self.status == OperationStatus::Failed
    }
}

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("device {0} is not configured")]
    NotFound(String),
    #[error("device {0} is already configured")]
    Duplicate(String),
    #[error("persistent storage failed: {0}")]
    Storage(String),
    #[error(transparent)]
    Transport(#[from] TransportError),
}

struct ManagedDevice {
    persisted: RwLock<PersistedDevice>,
    snapshot: RwLock<DeviceSnapshot>,
    operation: Mutex<()>,
}

impl ManagedDevice {
    fn new(device: PersistedDevice) -> Self {
        Self {
            snapshot: RwLock::new(DeviceSnapshot::waiting(&device)),
            persisted: RwLock::new(device),
            operation: Mutex::new(()),
        }
    }
}

pub struct MultiDeviceController<T, S, C = TokioRetryClock> {
    controller: ApplicationController<Arc<T>>,
    store: Arc<S>,
    clock: C,
    devices: RwLock<BTreeMap<String, Arc<ManagedDevice>>>,
    configuration: Mutex<()>,
}

impl<T, S> MultiDeviceController<T, S, TokioRetryClock>
where
    T: LightTransport + 'static,
    S: DeviceStore + 'static,
{
    pub fn load(transport: T, store: S) -> Result<Self, ManagerError> {
        Self::load_with_clock(transport, store, TokioRetryClock)
    }
}

impl<T, S, C> MultiDeviceController<T, S, C>
where
    T: LightTransport + 'static,
    S: DeviceStore + 'static,
    C: RetryClock,
{
    pub fn load_with_clock(transport: T, store: S, clock: C) -> Result<Self, ManagerError> {
        let document = store.load().map_err(ManagerError::Storage)?;
        let mut devices = BTreeMap::new();
        for persisted in document.devices {
            if let Some((_, confirmed_endpoint)) = persisted.identity.installation_local_value()
                && confirmed_endpoint != &persisted.endpoint
            {
                return Err(ManagerError::Storage(format!(
                    "installation-local identity {} is tied to {}, not {}; explicit removal and re-addition is required",
                    persisted.identity.canonical_id(),
                    confirmed_endpoint,
                    persisted.endpoint
                )));
            }
            let id = persisted.identity.canonical_id();
            if devices
                .insert(id.clone(), Arc::new(ManagedDevice::new(persisted)))
                .is_some()
            {
                return Err(ManagerError::Storage(format!(
                    "device document contains duplicate stable identity {id}"
                )));
            }
        }
        let transport = Arc::new(transport);
        Ok(Self {
            controller: ApplicationController::new(transport),
            store: Arc::new(store),
            clock,
            devices: RwLock::new(devices),
            configuration: Mutex::new(()),
        })
    }

    pub async fn snapshots(&self) -> Vec<DeviceSnapshot> {
        let devices: Vec<_> = self.devices.read().await.values().cloned().collect();
        let mut snapshots = Vec::with_capacity(devices.len());
        for device in devices {
            snapshots.push(device.snapshot.read().await.clone());
        }
        snapshots
    }

    pub async fn snapshot(&self, id: &str) -> Result<DeviceSnapshot, ManagerError> {
        let device = self.device(id).await?;
        let snapshot = device.snapshot.read().await.clone();
        Ok(snapshot)
    }

    pub async fn accessory_info(&self, id: &str) -> Result<AccessoryInfo, ManagerError> {
        let device = self.device(id).await?;
        let _operation = device.operation.lock().await;
        let endpoint = device.persisted.read().await.endpoint.clone();
        match self
            .controller
            .execute(&endpoint, DeviceCommand::AccessoryInfo)
            .await?
        {
            crate::CommandResult::AccessoryInfo { accessory } => Ok(accessory),
            _ => unreachable!("accessory command always returns accessory information"),
        }
    }

    pub async fn add(&self, endpoint: DeviceEndpoint) -> Result<DeviceSnapshot, ManagerError> {
        let _configuration = self.configuration.lock().await;
        let accessory = self
            .controller
            .execute(&endpoint, DeviceCommand::AccessoryInfo)
            .await?;
        let state = self
            .controller
            .execute(&endpoint, DeviceCommand::State)
            .await?;
        let crate::CommandResult::AccessoryInfo { accessory } = accessory else {
            return Err(ManagerError::InvalidInput(
                "accessory validation returned an unexpected result".to_string(),
            ));
        };
        let crate::CommandResult::State { state } = state else {
            return Err(ManagerError::InvalidInput(
                "light-state validation returned an unexpected result".to_string(),
            ));
        };
        let identity = DeviceIdentity::select(&accessory, None, &endpoint, Uuid::new_v4());
        let id = identity.canonical_id();
        let name = DocumentName::new(accessory.best_name().to_string())
            .map_err(|error| ManagerError::InvalidInput(error.to_string()))?;
        let mut devices = self.devices.write().await;
        if let Some(existing) = devices.get(&id) {
            let _operation = existing.operation.lock().await;
            let mut persisted = existing.persisted.write().await;
            if !persisted.identity.can_follow_endpoint_change() && persisted.endpoint != endpoint {
                return Err(ManagerError::Duplicate(format!(
                    "{id}; installation-local identities cannot move endpoints"
                )));
            }
            let previous_persisted = persisted.clone();
            persisted.endpoint = endpoint;
            persisted.name = name;
            let mut snapshot = existing.snapshot.write().await;
            let previous_snapshot = snapshot.clone();
            snapshot.name = persisted.name.to_string();
            snapshot.endpoint = persisted.endpoint.to_string();
            snapshot.apply_success(state);
            let result = snapshot.clone();
            drop(snapshot);
            drop(persisted);
            if let Err(error) = self.persist_locked(&devices) {
                *existing.persisted.write().await = previous_persisted;
                *existing.snapshot.write().await = previous_snapshot;
                return Err(error);
            }
            return Ok(result);
        }
        // A local identity is tied to its confirmed endpoint. Re-adding that endpoint is a
        // duplicate, never an implicit reassociation to a newly generated local UUID.
        for existing in devices.values() {
            let persisted = existing.persisted.read().await;
            if persisted.identity.installation_local_value().is_some()
                && persisted.endpoint == endpoint
            {
                return Err(ManagerError::Duplicate(persisted.identity.canonical_id()));
            }
        }
        let persisted = PersistedDevice::new(identity, name, endpoint);
        let managed = Arc::new(ManagedDevice::new(persisted));
        managed.snapshot.write().await.apply_success(state);
        let snapshot = managed.snapshot.read().await.clone();
        devices.insert(id, managed);
        if let Err(error) = self.persist_locked(&devices) {
            devices.remove(&snapshot.device_id);
            return Err(error);
        }
        Ok(snapshot)
    }

    pub async fn remove(&self, id: &str) -> Result<DeviceSnapshot, ManagerError> {
        let _configuration = self.configuration.lock().await;
        let mut devices = self.devices.write().await;
        let removed = devices
            .remove(id)
            .ok_or_else(|| ManagerError::NotFound(id.to_string()))?;
        if let Err(error) = self.persist_locked(&devices) {
            devices.insert(id.to_string(), removed.clone());
            return Err(error);
        }
        let snapshot = removed.snapshot.read().await.clone();
        Ok(snapshot)
    }

    pub async fn refresh(&self, id: &str) -> Result<DeviceOperationResult, ManagerError> {
        let device = self.device(id).await?;
        Ok(self.refresh_device(device).await)
    }

    pub async fn refresh_all(&self) -> Vec<DeviceOperationResult> {
        self.for_all(|device| async move { self.refresh_device(device).await })
            .await
    }

    pub async fn set(
        &self,
        id: &str,
        update: SetLightState,
    ) -> Result<DeviceOperationResult, ManagerError> {
        let device = self.device(id).await?;
        Ok(self.run_mutation(device, DeviceCommand::Set(update)).await)
    }

    pub async fn toggle(&self, id: &str) -> Result<DeviceOperationResult, ManagerError> {
        let device = self.device(id).await?;
        Ok(self.run_mutation(device, DeviceCommand::Toggle).await)
    }

    pub async fn identify(&self, id: &str) -> Result<DeviceOperationResult, ManagerError> {
        let device = self.device(id).await?;
        let _operation = device.operation.lock().await;
        let endpoint = device.persisted.read().await.endpoint.clone();
        match self
            .controller
            .execute(&endpoint, DeviceCommand::Identify)
            .await
        {
            Ok(crate::CommandResult::Identified) => Ok(DeviceOperationResult::succeeded(
                device.snapshot.read().await.clone(),
            )),
            Ok(_) => Err(ManagerError::InvalidInput(
                "identify returned an unexpected result".to_string(),
            )),
            Err(error) => {
                let mut snapshot = device.snapshot.write().await;
                snapshot.last_error = error.to_string();
                Ok(DeviceOperationResult::failed(snapshot.clone(), &error))
            }
        }
    }

    pub async fn toggle_all(&self) -> Vec<DeviceOperationResult> {
        let devices: Vec<_> = self.devices.read().await.values().cloned().collect();
        let mut available = Vec::new();
        let mut skipped = Vec::new();
        let mut any_on = false;
        for device in devices {
            let snapshot = device.snapshot.read().await.clone();
            if snapshot.online && snapshot.has_state {
                any_on |= snapshot.is_on;
                available.push(device);
            } else {
                skipped.push(DeviceOperationResult::skipped(snapshot));
            }
        }
        let target = !any_on;
        let mut results = stream::iter(available)
            .map(|device| async move {
                self.run_mutation(
                    device,
                    DeviceCommand::Set(SetLightState {
                        power: Some(target),
                        ..SetLightState::default()
                    }),
                )
                .await
            })
            .buffer_unordered(MAX_CONCURRENT_OPERATIONS)
            .collect::<Vec<_>>()
            .await;
        results.extend(skipped);
        results.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        results
    }

    async fn refresh_device(&self, device: Arc<ManagedDevice>) -> DeviceOperationResult {
        let _operation = device.operation.lock().await;
        let endpoint = device.persisted.read().await.endpoint.clone();
        let first = self
            .controller
            .execute(&endpoint, DeviceCommand::State)
            .await;
        let result = match first {
            Ok(result) => Ok(result),
            Err(_) => {
                self.clock.sleep(REFRESH_RETRY_DELAY).await;
                self.controller
                    .execute(&endpoint, DeviceCommand::State)
                    .await
            }
        };
        match result {
            Ok(crate::CommandResult::State { state }) => {
                let mut snapshot = device.snapshot.write().await;
                snapshot.apply_success(state);
                DeviceOperationResult::succeeded(snapshot.clone())
            }
            Ok(_) => unreachable!("state command always returns state"),
            Err(error) => {
                let mut snapshot = device.snapshot.write().await;
                snapshot.apply_refresh_failure(&error);
                DeviceOperationResult::failed(snapshot.clone(), &error)
            }
        }
    }

    async fn run_mutation(
        &self,
        device: Arc<ManagedDevice>,
        command: DeviceCommand,
    ) -> DeviceOperationResult {
        let _operation = device.operation.lock().await;
        let endpoint = device.persisted.read().await.endpoint.clone();
        match self.controller.execute(&endpoint, command).await {
            Ok(crate::CommandResult::State { state }) => {
                let mut snapshot = device.snapshot.write().await;
                snapshot.apply_success(state);
                DeviceOperationResult::succeeded(snapshot.clone())
            }
            Ok(_) => unreachable!("state mutation always returns state"),
            Err(error) => {
                let mut snapshot = device.snapshot.write().await;
                snapshot.last_error = error.to_string();
                DeviceOperationResult::failed(snapshot.clone(), &error)
            }
        }
    }

    async fn for_all<F, Fut>(&self, operation: F) -> Vec<DeviceOperationResult>
    where
        F: Fn(Arc<ManagedDevice>) -> Fut,
        Fut: Future<Output = DeviceOperationResult>,
    {
        let devices: Vec<_> = self.devices.read().await.values().cloned().collect();
        let mut results = stream::iter(devices)
            .map(operation)
            .buffer_unordered(MAX_CONCURRENT_OPERATIONS)
            .collect::<Vec<_>>()
            .await;
        results.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        results
    }

    async fn device(&self, id: &str) -> Result<Arc<ManagedDevice>, ManagerError> {
        self.devices
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| ManagerError::NotFound(id.to_string()))
    }

    fn persist_locked(
        &self,
        devices: &BTreeMap<String, Arc<ManagedDevice>>,
    ) -> Result<(), ManagerError> {
        // Configuration changes hold the configuration mutex and devices write lock, so these
        // non-blocking reads cannot race with another persistence operation.
        let persisted = devices
            .values()
            .map(|device| {
                device
                    .persisted
                    .try_read()
                    .expect("configuration owns persisted record")
                    .clone()
            })
            .collect();
        self.store
            .save(&DeviceStorageDocument::new(persisted))
            .map_err(ManagerError::Storage)
    }
}
