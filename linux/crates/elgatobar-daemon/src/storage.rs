use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use elgatobar_core::{DeviceStorageDocument, DeviceStore, SettingsDocument};
use serde::Serialize;
use uuid::Uuid;

const APPLICATION_DIRECTORY: &str = "elgatobar";
const DEVICES_FILE: &str = "devices-v1.json";
const SETTINGS_FILE: &str = "settings-v1.json";

#[derive(Clone, Debug)]
pub struct StoragePaths {
    pub data_directory: PathBuf,
    pub config_directory: PathBuf,
}

impl StoragePaths {
    pub fn discover() -> Result<Self, String> {
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set; set XDG_DATA_HOME and XDG_CONFIG_HOME".to_string())?;
        let data = env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let config = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Ok(Self {
            data_directory: data.join(APPLICATION_DIRECTORY),
            config_directory: config.join(APPLICATION_DIRECTORY),
        })
    }

    #[must_use]
    pub fn with_roots(data_root: PathBuf, config_root: PathBuf) -> Self {
        Self {
            data_directory: data_root.join(APPLICATION_DIRECTORY),
            config_directory: config_root.join(APPLICATION_DIRECTORY),
        }
    }

    #[must_use]
    pub fn device_file(&self) -> PathBuf {
        self.data_directory.join(DEVICES_FILE)
    }

    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config_directory.join(SETTINGS_FILE)
    }
}

#[derive(Clone, Debug)]
pub struct FileDeviceStore {
    path: PathBuf,
}

impl FileDeviceStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl DeviceStore for FileDeviceStore {
    fn load(&self) -> Result<DeviceStorageDocument, String> {
        if !self.path.exists() {
            return Ok(DeviceStorageDocument::new(Vec::new()));
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| format!("could not read {}: {error}", self.path.display()))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("could not decode {}: {error}", self.path.display()))
    }

    fn save(&self, document: &DeviceStorageDocument) -> Result<(), String> {
        atomic_write_json(&self.path, document, |_| Ok(()))
            .map_err(|error| format!("could not replace {}: {error}", self.path.display()))
    }
}

pub fn load_settings(path: &Path) -> Result<SettingsDocument, String> {
    if !path.exists() {
        return Ok(SettingsDocument::default());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not decode {}: {error}", path.display()))
}

pub fn save_settings(path: &Path, settings: &SettingsDocument) -> Result<(), String> {
    atomic_write_json(path, settings, |_| Ok(()))
        .map_err(|error| format!("could not replace {}: {error}", path.display()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicWriteStage {
    Encoded,
    TemporaryCreated,
    TemporarySynced,
    Replaced,
    DirectorySynced,
}

pub fn atomic_write_json<T, F>(path: &Path, value: &T, mut checkpoint: F) -> io::Result<()>
where
    T: Serialize,
    F: FnMut(AtomicWriteStage) -> io::Result<()>,
{
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    checkpoint(AtomicWriteStage::Encoded)?;

    let directory = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "storage path has no parent"))?;
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document"),
        Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        checkpoint(AtomicWriteStage::TemporaryCreated)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        checkpoint(AtomicWriteStage::TemporarySynced)?;
        fs::rename(&temporary, path)?;
        checkpoint(AtomicWriteStage::Replaced)?;
        File::open(directory)?.sync_all()?;
        checkpoint(AtomicWriteStage::DirectorySynced)
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_replaces_document_only_after_temp_sync() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("devices.json");
        fs::write(&path, b"previous\n").unwrap();
        let document = DeviceStorageDocument::new(Vec::new());

        atomic_write_json(&path, &document, |_| Ok(())).unwrap();
        let decoded: DeviceStorageDocument =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(decoded.version(), 1);
    }

    #[test]
    fn failures_before_replace_preserve_previous_document() {
        for failed_stage in [
            AtomicWriteStage::Encoded,
            AtomicWriteStage::TemporaryCreated,
            AtomicWriteStage::TemporarySynced,
        ] {
            let directory = TempDir::new().unwrap();
            let path = directory.path().join("devices.json");
            fs::write(&path, b"previous\n").unwrap();
            let result =
                atomic_write_json(&path, &DeviceStorageDocument::new(Vec::new()), |stage| {
                    if stage == failed_stage {
                        Err(io::Error::other("injected failure"))
                    } else {
                        Ok(())
                    }
                });
            assert!(result.is_err());
            assert_eq!(fs::read(&path).unwrap(), b"previous\n");
        }
    }

    #[test]
    fn future_versions_are_rejected_without_rewriting_them() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("devices.json");
        let future = br#"{"version":99,"devices":[]}"#;
        fs::write(&path, future).unwrap();
        let store = FileDeviceStore::new(path.clone());
        let error = store.load().unwrap_err();
        assert!(error.contains("unsupported device storage version 99"));
        assert_eq!(fs::read(path).unwrap(), future);
    }

    #[test]
    fn settings_round_trip_and_future_version_is_preserved() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("settings.json");
        let settings = SettingsDocument::new(10).unwrap();
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);

        let future = br#"{"version":2,"refreshIntervalSeconds":5}"#;
        fs::write(&path, future).unwrap();
        assert!(
            load_settings(&path)
                .unwrap_err()
                .contains("unsupported settings version 2")
        );
        assert_eq!(fs::read(path).unwrap(), future);
    }
}
