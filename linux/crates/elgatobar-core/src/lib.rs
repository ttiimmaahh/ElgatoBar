//! Portable Elgato control core for the standalone Linux milestone.

mod controller;
mod domain;
mod http;
mod interchange;
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
pub use transport::{LightTransport, TransportError};
