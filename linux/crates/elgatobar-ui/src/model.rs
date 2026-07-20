use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use elgatobar_dbus::{DeviceSnapshot, OperationResult};

pub const MIN_BRIGHTNESS: u8 = 3;
pub const MAX_BRIGHTNESS: u8 = 100;
pub const MIN_NATIVE_TEMPERATURE: u16 = 143;
pub const MAX_NATIVE_TEMPERATURE: u16 = 344;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectBackoff(Duration);

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self(Duration::from_secs(1))
    }
}
impl ReconnectBackoff {
    pub fn current(self) -> Duration {
        self.0
    }
    pub fn advance(&mut self) {
        self.0 = (self.0 * 2).min(Duration::from_secs(30));
    }
    pub fn reset(&mut self) {
        self.0 = Duration::from_secs(1);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Loading,
    Available,
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InventoryState {
    Loading,
    Unavailable,
    Unconfigured,
    AllOnlineOn,
    AllOnlineOff,
    Mixed,
    PartialOffline,
    AllOffline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateDisplay {
    pub power: String,
    pub brightness: String,
    pub temperature: String,
    pub native_temperature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub status: String,
    pub detail: String,
    pub state: Option<StateDisplay>,
    pub mutations_enabled: bool,
    pub pending: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowModel {
    pub state: InventoryState,
    pub stale: bool,
    pub summary: String,
    pub rows: Vec<DeviceRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeedbackKind {
    Success,
    PartialFailure,
    CompleteFailure,
    InvalidInput,
    Connectivity,
    Protocol,
    Storage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Feedback {
    pub kind: FeedbackKind,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Intent {
    Retry,
    RefreshAll,
    ToggleAll,
    Toggle(String),
    Brightness(String, u8),
    Temperature(String, u16),
    Identify(String),
    Add(String),
    Remove(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Retry,
    RefreshAll {
        generation: u64,
    },
    ToggleAll {
        generation: u64,
    },
    Toggle {
        id: String,
        generation: u64,
    },
    SetBrightness {
        id: String,
        value: u8,
        generation: u64,
    },
    SetTemperature {
        id: String,
        value: u16,
        generation: u64,
    },
    Identify {
        id: String,
        generation: u64,
    },
    Add {
        endpoint: String,
        generation: u64,
    },
    Remove {
        id: String,
        generation: u64,
    },
}

#[derive(Default)]
pub struct Controller {
    connection: Option<ConnectionState>,
    snapshots: Vec<DeviceSnapshot>,
    generation: u64,
    pending: BTreeMap<String, u64>,
    queued_brightness: BTreeMap<String, u8>,
    queued_temperature: BTreeMap<String, u16>,
    aggregate_pending: Option<u64>,
    configuration_pending: Option<u64>,
}

impl Controller {
    pub fn loading() -> Self {
        Self {
            connection: Some(ConnectionState::Loading),
            ..Self::default()
        }
    }
    pub fn connection(&mut self, state: ConnectionState) {
        self.connection = Some(state);
    }
    pub fn replace(&mut self, snapshots: Vec<DeviceSnapshot>) {
        self.generation += 1;
        self.snapshots = snapshots;
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn aggregate_pending(&self) -> bool {
        self.aggregate_pending.is_some()
    }
    pub fn configuration_pending(&self) -> bool {
        self.configuration_pending.is_some()
    }
    pub fn snapshot(&self, id: &str) -> Option<&DeviceSnapshot> {
        self.snapshots.iter().find(|s| s.device_id == id)
    }

    pub fn model(&self) -> WindowModel {
        let connection = self
            .connection
            .as_ref()
            .unwrap_or(&ConnectionState::Loading);
        let stale = matches!(connection, ConnectionState::Unavailable(_));
        let state = if matches!(connection, ConnectionState::Loading) && self.snapshots.is_empty() {
            InventoryState::Loading
        } else if stale && self.snapshots.is_empty() {
            InventoryState::Unavailable
        } else if self.snapshots.is_empty() {
            InventoryState::Unconfigured
        } else {
            classify(&self.snapshots)
        };
        let summary = match (&state, connection) {
            (InventoryState::Loading, _) => "Connecting to the ElgatoBar daemon…".into(),
            (InventoryState::Unavailable, ConnectionState::Unavailable(e)) => {
                format!("Daemon unavailable: {e}")
            }
            (InventoryState::Unconfigured, _) => "No lights are configured.".into(),
            (InventoryState::PartialOffline, _) => {
                "Some lights are offline; cached values are retained.".into()
            }
            (InventoryState::AllOffline, _) => {
                "All lights are offline; values shown are last known.".into()
            }
            (_, ConnectionState::Unavailable(e)) => {
                format!("Daemon unavailable; showing stale values. {e}")
            }
            _ => format!(
                "{} configured light{}",
                self.snapshots.len(),
                if self.snapshots.len() == 1 { "" } else { "s" }
            ),
        };
        let rows = self
            .snapshots
            .iter()
            .map(|s| DeviceRow {
                id: s.device_id.clone(),
                name: s.name.clone(),
                endpoint: s.endpoint.clone(),
                status: if stale {
                    "Stale"
                } else if s.online {
                    if s.is_on { "On" } else { "Off" }
                } else {
                    "Offline"
                }
                .into(),
                detail: if s.last_error.is_empty() {
                    s.device_id.clone()
                } else {
                    format!("{} · {}", s.device_id, s.last_error)
                },
                state: s.has_state.then(|| StateDisplay {
                    power: if s.is_on { "On" } else { "Off" }.into(),
                    brightness: format!("{}%", s.brightness),
                    temperature: format!("{} K", native_to_kelvin(s.temperature)),
                    native_temperature: format!("native {}", s.temperature),
                }),
                mutations_enabled: !stale && s.online && s.has_state,
                pending: self.pending.contains_key(&s.device_id),
            })
            .collect();
        WindowModel {
            state,
            stale,
            summary,
            rows,
        }
    }

    pub fn intent(&mut self, intent: Intent) -> Option<Command> {
        self.generation += 1;
        let generation = self.generation;
        match intent {
            Intent::Retry => Some(Command::Retry),
            Intent::RefreshAll => {
                if self.aggregate_pending.is_some() {
                    None
                } else {
                    self.aggregate_pending = Some(generation);
                    Some(Command::RefreshAll { generation })
                }
            }
            Intent::ToggleAll => {
                if self.aggregate_pending.is_some() {
                    None
                } else {
                    self.aggregate_pending = Some(generation);
                    Some(Command::ToggleAll { generation })
                }
            }
            Intent::Toggle(id) => {
                self.begin(id, generation, |id| Command::Toggle { id, generation })
            }
            Intent::Brightness(id, value) => {
                let value = value.clamp(MIN_BRIGHTNESS, MAX_BRIGHTNESS);
                if self.pending.contains_key(&id) {
                    self.queued_brightness.insert(id, value);
                    None
                } else {
                    self.begin(id, generation, |id| Command::SetBrightness {
                        id,
                        value,
                        generation,
                    })
                }
            }
            Intent::Temperature(id, value) => {
                let value = value.clamp(MIN_NATIVE_TEMPERATURE, MAX_NATIVE_TEMPERATURE);
                if self.pending.contains_key(&id) {
                    self.queued_temperature.insert(id, value);
                    None
                } else {
                    self.begin(id, generation, |id| Command::SetTemperature {
                        id,
                        value,
                        generation,
                    })
                }
            }
            Intent::Identify(id) => {
                self.begin(id, generation, |id| Command::Identify { id, generation })
            }
            Intent::Add(endpoint) => {
                if self.configuration_pending.is_some() {
                    None
                } else {
                    self.configuration_pending = Some(generation);
                    Some(Command::Add {
                        endpoint,
                        generation,
                    })
                }
            }
            Intent::Remove(id) => {
                self.pending.insert(id.clone(), generation);
                Some(Command::Remove { id, generation })
            }
        }
    }

    fn begin(
        &mut self,
        id: String,
        generation: u64,
        f: impl FnOnce(String) -> Command,
    ) -> Option<Command> {
        if self.snapshot(&id).is_some_and(|s| s.online) {
            self.pending.insert(id.clone(), generation);
            Some(f(id))
        } else {
            None
        }
    }

    pub fn complete(&mut self, id: &str, generation: u64) -> Option<Command> {
        if self.pending.get(id).copied() != Some(generation) {
            return None;
        }
        self.pending.remove(id);
        self.generation += 1;
        let next = self.generation;
        if let Some(value) = self.queued_brightness.remove(id) {
            self.pending.insert(id.into(), next);
            return Some(Command::SetBrightness {
                id: id.into(),
                value,
                generation: next,
            });
        }
        if let Some(value) = self.queued_temperature.remove(id) {
            self.pending.insert(id.into(), next);
            return Some(Command::SetTemperature {
                id: id.into(),
                value,
                generation: next,
            });
        }
        None
    }
    pub fn complete_aggregate(&mut self, generation: u64) {
        if self.aggregate_pending == Some(generation) {
            self.aggregate_pending = None;
        }
    }
    pub fn complete_configuration(&mut self, generation: u64) {
        if self.configuration_pending == Some(generation) {
            self.configuration_pending = None;
        }
    }
}

fn classify(s: &[DeviceSnapshot]) -> InventoryState {
    let online = s.iter().filter(|d| d.online).count();
    if online == 0 {
        InventoryState::AllOffline
    } else if online < s.len() {
        InventoryState::PartialOffline
    } else if s.iter().all(|d| d.has_state && d.is_on) {
        InventoryState::AllOnlineOn
    } else if s.iter().all(|d| d.has_state && !d.is_on) {
        InventoryState::AllOnlineOff
    } else {
        InventoryState::Mixed
    }
}

pub fn native_to_kelvin(native: u16) -> u32 {
    if native == 0 {
        0
    } else {
        1_000_000 / u32::from(native)
    }
}
pub fn kelvin_to_native(kelvin: u32) -> u16 {
    ((1_000_000 / kelvin.clamp(2900, 7000)) as u16)
        .clamp(MIN_NATIVE_TEMPERATURE, MAX_NATIVE_TEMPERATURE)
}

pub fn operation_feedback(results: &[OperationResult]) -> Feedback {
    let succeeded = results.iter().filter(|r| r.status == "succeeded").count();
    let failed: Vec<_> = results.iter().filter(|r| r.status != "succeeded").collect();
    let names: BTreeSet<_> = failed
        .iter()
        .map(|r| {
            if r.snapshot.name.is_empty() {
                r.device_id.as_str()
            } else {
                r.snapshot.name.as_str()
            }
        })
        .collect();
    let kind = if failed.is_empty() {
        FeedbackKind::Success
    } else if succeeded > 0 {
        FeedbackKind::PartialFailure
    } else {
        FeedbackKind::CompleteFailure
    };
    let title = if failed.is_empty() {
        "Operation completed"
    } else if succeeded > 0 {
        "Operation partially completed"
    } else {
        "Operation failed"
    }
    .into();
    Feedback {
        kind,
        title,
        detail: if failed.is_empty() {
            format!("{succeeded} device(s) updated")
        } else {
            format!(
                "Affected: {}",
                names.into_iter().collect::<Vec<_>>().join(", ")
            )
        },
    }
}

pub fn error_feedback(message: &str) -> Feedback {
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("invalidinput") || lower.contains("invalid input") {
        FeedbackKind::InvalidInput
    } else if lower.contains("protocol") {
        FeedbackKind::Protocol
    } else if lower.contains("storage") {
        FeedbackKind::Storage
    } else {
        FeedbackKind::Connectivity
    };
    Feedback {
        kind,
        title: "Operation failed".into(),
        detail: message.into(),
    }
}
