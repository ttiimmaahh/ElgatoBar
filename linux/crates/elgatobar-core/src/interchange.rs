use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    DeviceEndpoint, DeviceIdentity, LightState, domain::deserialize_canonical_uuid,
    domain::deserialize_integral_u32,
};

pub const INTERCHANGE_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterchangeDocument {
    version: u32,
    pub devices: Vec<PersistedDevice>,
    pub scenes: Vec<Scene>,
}

impl InterchangeDocument {
    #[must_use]
    pub fn new(devices: Vec<PersistedDevice>, scenes: Vec<Scene>) -> Self {
        Self {
            version: INTERCHANGE_VERSION,
            devices,
            scenes,
        }
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawInterchangeDocument {
    #[serde(deserialize_with = "deserialize_version")]
    version: u32,
    devices: Vec<PersistedDevice>,
    scenes: Vec<Scene>,
}

impl<'de> Deserialize<'de> for InterchangeDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawInterchangeDocument::deserialize(deserializer)?;
        if raw.version != INTERCHANGE_VERSION {
            return Err(de::Error::custom(format!(
                "unsupported interchange version {}; expected {}",
                raw.version, INTERCHANGE_VERSION
            )));
        }
        Ok(Self::new(raw.devices, raw.scenes))
    }
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_integral_u32(deserializer, "interchange version")
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("name must contain a non-whitespace character")]
pub struct DocumentNameError;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DocumentName(String);

impl DocumentName {
    pub fn new(value: impl Into<String>) -> Result<Self, DocumentNameError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DocumentNameError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for DocumentName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

impl TryFrom<String> for DocumentName {
    type Error = DocumentNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for DocumentName {
    type Error = DocumentNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for DocumentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedDevice {
    pub identity: DeviceIdentity,
    pub name: DocumentName,
    pub endpoint: DeviceEndpoint,
}

impl PersistedDevice {
    #[must_use]
    pub fn new(identity: DeviceIdentity, name: DocumentName, endpoint: DeviceEndpoint) -> Self {
        Self {
            identity,
            name,
            endpoint,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    pub id: Uuid,
    pub name: DocumentName,
    pub lights: Vec<SceneLight>,
}

impl Scene {
    #[must_use]
    pub fn new(id: Uuid, name: DocumentName, lights: Vec<SceneLight>) -> Self {
        Self { id, name, lights }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneLight {
    pub device_identity: DeviceIdentity,
    pub state: LightState,
}

impl SceneLight {
    #[must_use]
    pub fn new(device_identity: DeviceIdentity, state: LightState) -> Self {
        Self {
            device_identity,
            state,
        }
    }
}
