//! Portable Elgato control core for the standalone Linux milestone.

mod controller;
mod domain;
mod http;
mod interchange;
mod manager;
mod transport;

pub use controller::{ApplicationController, CommandResult, DeviceCommand, SetLightState};
pub use domain::{
    AccessoryInfo, Brightness, BrightnessError, DEFAULT_DEVICE_PORT, DeviceEndpoint,
    DeviceIdentity, DeviceIdentityKind, ElgatoTemperature, EndpointError, IdentityError,
    LightState, TemperatureError, WifiInfo,
};
pub use http::ReqwestLightTransport;
pub use interchange::{
    DocumentName, DocumentNameError, INTERCHANGE_VERSION, InterchangeDocument, PersistedDevice,
    Scene, SceneLight,
};
pub use manager::{
    DEFAULT_REFRESH_INTERVAL_SECONDS, DEVICE_STORAGE_VERSION, DeviceOperationResult,
    DeviceSnapshot, DeviceStorageDocument, DeviceStore, MAX_CONCURRENT_OPERATIONS, ManagerError,
    MultiDeviceController, OperationStatus, REFRESH_RETRY_DELAY, RetryClock,
    SETTINGS_STORAGE_VERSION, SUPPORTED_REFRESH_INTERVAL_SECONDS, SettingsDocument,
    TokioRetryClock,
};
pub use transport::{LightTransport, TransportError};
