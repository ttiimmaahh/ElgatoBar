use std::{fmt, net::Ipv6Addr, str::FromStr};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};
use thiserror::Error;
use url::{Host, Url};
use uuid::Uuid;

pub const DEFAULT_DEVICE_PORT: u16 = 9123;
const MIN_KELVIN: u32 = 2_900;
const MAX_KELVIN: u32 = 7_000;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EndpointError {
    #[error("endpoint must use plain http, not {0}")]
    UnsupportedScheme(String),
    #[error("endpoint must contain only a host and optional port")]
    UnexpectedComponents,
    #[error("endpoint host is missing or invalid")]
    InvalidHost,
    #[error("endpoint port is invalid")]
    InvalidPort,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct DeviceEndpoint {
    host: String,
    port: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDeviceEndpoint {
    host: String,
    #[serde(deserialize_with = "deserialize_port")]
    port: u16,
}

impl<'de> Deserialize<'de> for DeviceEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDeviceEndpoint::deserialize(deserializer)?;
        Self::new(raw.host, raw.port).map_err(de::Error::custom)
    }
}

impl DeviceEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self, EndpointError> {
        let host = host.into();
        let normalized = host.as_str();
        let ipv6 = normalized.parse::<Ipv6Addr>().ok();
        if normalized.is_empty()
            || normalized.chars().any(char::is_whitespace)
            || normalized.contains(['/', '\\', '@', '?', '#', '%', '[', ']'])
            || (normalized.contains(':') && ipv6.is_none())
        {
            return Err(EndpointError::InvalidHost);
        }
        if ipv6.is_none() {
            let candidate = Url::parse(&format!("http://{normalized}:1/"))
                .map_err(|_| EndpointError::InvalidHost)?;
            if !matches!(candidate.host(), Some(Host::Domain(_) | Host::Ipv4(_))) {
                return Err(EndpointError::InvalidHost);
            }
        }
        if port == 0 {
            return Err(EndpointError::InvalidPort);
        }
        Ok(Self {
            host: normalized.to_ascii_lowercase(),
            port,
        })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn url(&self, path: &str) -> Result<Url, EndpointError> {
        let host = if self.host.parse::<Ipv6Addr>().is_ok() {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        Url::parse(&format!("http://{host}:{}{}", self.port, path))
            .map_err(|_| EndpointError::InvalidHost)
    }
}

impl FromStr for DeviceEndpoint {
    type Err = EndpointError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(EndpointError::InvalidHost);
        }
        if let Some((scheme, _)) = value.split_once("://")
            && !scheme.eq_ignore_ascii_case("http")
        {
            return Err(EndpointError::UnsupportedScheme(scheme.to_string()));
        }
        let candidate = if value.contains("://") {
            value.to_string()
        } else {
            format!("http://{value}")
        };
        let parsed = Url::parse(&candidate).map_err(|_| EndpointError::InvalidHost)?;
        if parsed.scheme() != "http" {
            return Err(EndpointError::UnsupportedScheme(
                parsed.scheme().to_string(),
            ));
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            return Err(EndpointError::UnexpectedComponents);
        }
        let host = match parsed.host().ok_or(EndpointError::InvalidHost)? {
            Host::Domain(domain) => domain.to_string(),
            Host::Ipv4(address) => address.to_string(),
            Host::Ipv6(address) => address.to_string(),
        };
        let port = parsed.port().unwrap_or(DEFAULT_DEVICE_PORT);
        Self::new(host, port)
    }
}

impl fmt::Display for DeviceEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.parse::<Ipv6Addr>().is_ok() {
            write!(formatter, "[{}]:{}", self.host, self.port)
        } else {
            write!(formatter, "{}:{}", self.host, self.port)
        }
    }
}

struct UnsignedIntegralVisitor {
    name: &'static str,
    max: u64,
}

impl Visitor<'_> for UnsignedIntegralVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} as a non-negative integral number no greater than {}",
            self.name, self.max
        )
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value <= self.max {
            Ok(value)
        } else {
            Err(E::custom(format!(
                "{} {value} exceeds maximum {}",
                self.name, self.max
            )))
        }
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value = u64::try_from(value)
            .map_err(|_| E::custom(format!("{} must not be negative", self.name)))?;
        self.visit_u64(value)
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= self.max as f64 {
            Ok(value as u64)
        } else {
            Err(E::custom(format!(
                "{} must be a finite non-negative integer no greater than {}",
                self.name, self.max
            )))
        }
    }
}

fn deserialize_integral_u64<'de, D>(
    deserializer: D,
    name: &'static str,
    max: u64,
) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(UnsignedIntegralVisitor { name, max })
}

pub(crate) fn deserialize_integral_u16<'de, D>(
    deserializer: D,
    name: &'static str,
) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserialize_integral_u64(deserializer, name, u16::MAX.into())?;
    u16::try_from(value).map_err(de::Error::custom)
}

pub(crate) fn deserialize_integral_u32<'de, D>(
    deserializer: D,
    name: &'static str,
) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserialize_integral_u64(deserializer, name, u32::MAX.into())?;
    u32::try_from(value).map_err(de::Error::custom)
}

fn deserialize_port<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let port = deserialize_integral_u16(deserializer, "endpoint port")?;
    if port == 0 {
        return Err(de::Error::custom(EndpointError::InvalidPort));
    }
    Ok(port)
}

pub(crate) fn deserialize_integral_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    struct SignedIntegralVisitor;

    impl Visitor<'_> for SignedIntegralVisitor {
        type Value = i64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a finite integral number in the signed 64-bit range")
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            i64::try_from(value)
                .map_err(|_| E::custom(format!("integer {value} exceeds signed 64-bit range")))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
            if value.is_finite()
                && value.fract() == 0.0
                && value >= i64::MIN as f64
                && value < I64_UPPER_EXCLUSIVE
            {
                Ok(value as i64)
            } else {
                Err(E::custom(
                    "value must be a finite integer in the signed 64-bit range",
                ))
            }
        }
    }

    deserializer.deserialize_any(SignedIntegralVisitor)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("brightness {0} is outside the supported range 3..=100")]
pub struct BrightnessError(pub u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(into = "u8")]
pub struct Brightness(u8);

impl<'de> Deserialize<'de> for Brightness {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_integral_u64(deserializer, "brightness", u8::MAX.into())?;
        let value = u8::try_from(value).map_err(de::Error::custom)?;
        Self::try_from(value).map_err(de::Error::custom)
    }
}

impl Brightness {
    pub const MIN: u8 = 3;
    pub const MAX: u8 = 100;

    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Brightness {
    type Error = BrightnessError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(BrightnessError(value))
        }
    }
}

impl From<Brightness> for u8 {
    fn from(value: Brightness) -> Self {
        value.get()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("Elgato temperature {0} is outside the supported range 143..=344")]
pub struct TemperatureError(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(into = "u16")]
pub struct ElgatoTemperature(u16);

impl<'de> Deserialize<'de> for ElgatoTemperature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_integral_u16(deserializer, "Elgato temperature")?;
        Self::try_from(value).map_err(de::Error::custom)
    }
}

impl ElgatoTemperature {
    pub const MIN: u16 = 143;
    pub const MAX: u16 = 344;

    #[must_use]
    pub fn get(self) -> u16 {
        self.0
    }

    #[must_use]
    pub fn from_kelvin(kelvin: u32) -> Self {
        let kelvin = kelvin.clamp(MIN_KELVIN, MAX_KELVIN);
        let raw = (1_000_000 / kelvin) as u16;
        Self(raw.clamp(Self::MIN, Self::MAX))
    }

    #[must_use]
    pub fn to_kelvin(self) -> u32 {
        1_000_000 / u32::from(self.0)
    }
}

impl TryFrom<u16> for ElgatoTemperature {
    type Error = TemperatureError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(TemperatureError(value))
        }
    }
}

impl From<ElgatoTemperature> for u16 {
    fn from(value: ElgatoTemperature) -> Self {
        value.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LightState {
    pub is_on: bool,
    pub brightness: Brightness,
    pub temperature: ElgatoTemperature,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiInfo {
    pub ssid: Option<String>,
    #[serde(rename = "frequencyMHz")]
    pub frequency_mhz: Option<u32>,
    pub rssi: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryInfo {
    pub product_name: String,
    pub hardware_board_type: i64,
    pub firmware_build_number: i64,
    pub firmware_version: String,
    pub serial_number: String,
    pub display_name: Option<String>,
    pub features: Option<Vec<String>>,
    #[serde(rename = "wifi-info")]
    pub wifi_info: Option<WifiInfo>,
}

impl AccessoryInfo {
    #[must_use]
    pub fn best_name(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.product_name)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IdentityError {
    #[error("serial identity must contain a non-whitespace character")]
    BlankSerial,
    #[error("mDNS instance must contain a non-whitespace character")]
    BlankMdnsInstance,
    #[error("mDNS product name must contain a non-whitespace character")]
    BlankProductName,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceIdentityKind {
    Serial,
    Mdns,
    InstallationLocal,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DeviceIdentity(DeviceIdentityValue);

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
enum DeviceIdentityValue {
    Serial {
        serial: String,
    },
    Mdns {
        instance: String,
        product_name: String,
        hardware_board_type: i64,
    },
    InstallationLocal {
        id: Uuid,
        confirmed_endpoint: DeviceEndpoint,
    },
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum RawDeviceIdentity {
    Serial {
        serial: String,
    },
    Mdns {
        instance: String,
        product_name: String,
        #[serde(deserialize_with = "deserialize_integral_i64")]
        hardware_board_type: i64,
    },
    InstallationLocal {
        #[serde(deserialize_with = "deserialize_canonical_uuid")]
        id: Uuid,
        confirmed_endpoint: DeviceEndpoint,
    },
}

impl<'de> Deserialize<'de> for DeviceIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RawDeviceIdentity::deserialize(deserializer)? {
            RawDeviceIdentity::Serial { serial } => Self::serial(serial).map_err(de::Error::custom),
            RawDeviceIdentity::Mdns {
                instance,
                product_name,
                hardware_board_type,
            } => Self::mdns(instance, product_name, hardware_board_type).map_err(de::Error::custom),
            RawDeviceIdentity::InstallationLocal {
                id,
                confirmed_endpoint,
            } => Ok(Self::installation_local(id, confirmed_endpoint)),
        }
    }
}

impl DeviceIdentity {
    pub fn serial(serial: impl AsRef<str>) -> Result<Self, IdentityError> {
        let serial = normalize_component(serial.as_ref());
        if serial.is_empty() {
            return Err(IdentityError::BlankSerial);
        }
        Ok(Self(DeviceIdentityValue::Serial { serial }))
    }

    pub fn mdns(
        instance: impl AsRef<str>,
        product_name: impl AsRef<str>,
        hardware_board_type: i64,
    ) -> Result<Self, IdentityError> {
        let instance = normalize_component(instance.as_ref());
        if instance.is_empty() {
            return Err(IdentityError::BlankMdnsInstance);
        }
        let product_name = normalize_component(product_name.as_ref());
        if product_name.is_empty() {
            return Err(IdentityError::BlankProductName);
        }
        Ok(Self(DeviceIdentityValue::Mdns {
            instance,
            product_name,
            hardware_board_type,
        }))
    }

    #[must_use]
    pub fn installation_local(id: Uuid, confirmed_endpoint: DeviceEndpoint) -> Self {
        Self(DeviceIdentityValue::InstallationLocal {
            id,
            confirmed_endpoint,
        })
    }

    #[must_use]
    pub fn select(
        accessory: &AccessoryInfo,
        mdns_instance: Option<&str>,
        endpoint: &DeviceEndpoint,
        installation_id: Uuid,
    ) -> Self {
        if let Ok(identity) = Self::serial(&accessory.serial_number) {
            return identity;
        }
        if let Some(instance) = mdns_instance
            && let Ok(identity) = Self::mdns(
                instance,
                &accessory.product_name,
                accessory.hardware_board_type,
            )
        {
            return identity;
        }
        Self::installation_local(installation_id, endpoint.clone())
    }

    #[must_use]
    pub fn kind(&self) -> DeviceIdentityKind {
        match self.0 {
            DeviceIdentityValue::Serial { .. } => DeviceIdentityKind::Serial,
            DeviceIdentityValue::Mdns { .. } => DeviceIdentityKind::Mdns,
            DeviceIdentityValue::InstallationLocal { .. } => DeviceIdentityKind::InstallationLocal,
        }
    }

    #[must_use]
    pub fn serial_value(&self) -> Option<&str> {
        match &self.0 {
            DeviceIdentityValue::Serial { serial } => Some(serial),
            _ => None,
        }
    }

    #[must_use]
    pub fn mdns_value(&self) -> Option<(&str, &str, i64)> {
        match &self.0 {
            DeviceIdentityValue::Mdns {
                instance,
                product_name,
                hardware_board_type,
            } => Some((instance, product_name, *hardware_board_type)),
            _ => None,
        }
    }

    #[must_use]
    pub fn installation_local_value(&self) -> Option<(Uuid, &DeviceEndpoint)> {
        match &self.0 {
            DeviceIdentityValue::InstallationLocal {
                id,
                confirmed_endpoint,
            } => Some((*id, confirmed_endpoint)),
            _ => None,
        }
    }

    #[must_use]
    pub fn can_follow_endpoint_change(&self) -> bool {
        !matches!(self.0, DeviceIdentityValue::InstallationLocal { .. })
    }
}

pub(crate) fn deserialize_canonical_uuid<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let id = Uuid::parse_str(&value).map_err(de::Error::custom)?;
    if value != id.hyphenated().to_string() {
        return Err(de::Error::custom(
            "UUID must use lowercase hyphenated canonical text",
        ));
    }
    Ok(id)
}

fn normalize_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
