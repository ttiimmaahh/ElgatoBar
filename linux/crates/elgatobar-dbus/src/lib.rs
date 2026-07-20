//! Versioned D-Bus contract shared by the Linux daemon and its clients.

use serde::{Deserialize, Serialize};
use zbus::{proxy, zvariant::Type};

pub const SERVICE_NAME: &str = "io.github.ttiimmaahh.ElgatoBar1";
pub const OBJECT_PATH: &str = "/io/github/ttiimmaahh/ElgatoBar1";
pub const INTERFACE_NAME: &str = "io.github.ttiimmaahh.ElgatoBar1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct LightSnapshot {
    pub endpoint: String,
    pub online: bool,
    pub is_on: bool,
    pub brightness: u8,
    pub temperature: u16,
    pub last_error: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct OperationResult {
    pub device_id: String,
    /// `succeeded`, `failed`, or `skipped-offline`.
    pub status: String,
    pub snapshot: DeviceSnapshot,
    /// Empty on success; otherwise `connectivity`, `protocol`, or `offline`.
    pub error_kind: String,
    pub error: String,
}

impl LightSnapshot {
    #[must_use]
    pub fn unavailable(endpoint: String, message: impl Into<String>) -> Self {
        Self {
            endpoint,
            online: false,
            is_on: false,
            brightness: 0,
            temperature: 0,
            last_error: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct AccessorySnapshot {
    pub display_name: String,
    pub product_name: String,
    pub serial_number: String,
    pub firmware_version: String,
    pub hardware_board_type: i64,
}

#[proxy(
    interface = "io.github.ttiimmaahh.ElgatoBar1",
    default_service = "io.github.ttiimmaahh.ElgatoBar1",
    default_path = "/io/github/ttiimmaahh/ElgatoBar1"
)]
pub trait ElgatoBar {
    async fn add_device(&self, endpoint: &str) -> zbus::Result<DeviceSnapshot>;
    async fn accessory_info(&self) -> zbus::Result<AccessorySnapshot>;
    async fn device_snapshot(&self, device_id: &str) -> zbus::Result<DeviceSnapshot>;
    async fn identify_device(&self, device_id: &str) -> zbus::Result<OperationResult>;
    async fn identify(&self) -> zbus::Result<()>;
    async fn list_devices(&self) -> zbus::Result<Vec<DeviceSnapshot>>;
    async fn refresh(&self) -> zbus::Result<LightSnapshot>;
    async fn refresh_all(&self) -> zbus::Result<Vec<OperationResult>>;
    async fn refresh_device(&self, device_id: &str) -> zbus::Result<OperationResult>;
    async fn remove_device(&self, device_id: &str) -> zbus::Result<DeviceSnapshot>;
    async fn set_device_state(
        &self,
        device_id: &str,
        has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
    ) -> zbus::Result<OperationResult>;
    async fn set_state(
        &self,
        has_power: bool,
        power: bool,
        brightness: u8,
        temperature: u16,
    ) -> zbus::Result<LightSnapshot>;
    async fn snapshot(&self) -> zbus::Result<LightSnapshot>;
    async fn toggle(&self) -> zbus::Result<LightSnapshot>;
    async fn toggle_all(&self) -> zbus::Result<Vec<OperationResult>>;
    async fn toggle_device(&self, device_id: &str) -> zbus::Result<OperationResult>;

    #[zbus(signal)]
    async fn devices_changed(&self, snapshots: Vec<DeviceSnapshot>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn state_changed(&self, snapshot: LightSnapshot) -> zbus::Result<()>;
}
