use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub const KEY_MAX: usize = 128;
pub const VALUE_MAX: usize = 64 * 1024;
pub const TOTAL_MAX: usize = 2 * 1024 * 1024;
pub const KEYS_MAX: usize = 2_000;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("invalid storage key")]
    Key,
    #[error("a value can be at most 64 KiB")]
    ValueTooLarge,
    #[error("the extension storage is full")]
    Full,
    #[error("could not write extension storage: {0}")]
    Io(#[from] std::io::Error),
    #[error("extension storage is corrupt: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtensionStorage {
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

impl ExtensionStorage {
    pub fn load(path: &Path) -> Result<Self, StorageError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(StorageError::Io(error)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, &bytes)?;
        restrict_permissions(&tmp)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }

    pub fn set(&mut self, key: &str, value: Value) -> Result<(), StorageError> {
        validate_key(key)?;
        let encoded = serde_json::to_vec(&value)?;
        if encoded.len() > VALUE_MAX {
            return Err(StorageError::ValueTooLarge);
        }
        if !self.values.contains_key(key) && self.values.len() >= KEYS_MAX {
            return Err(StorageError::Full);
        }
        let previous = self.values.insert(key.to_string(), value.clone());
        let total: usize = self
            .values
            .iter()
            .map(|(key, value)| key.len() + serde_json::to_vec(value).map_or(0, |v| v.len()))
            .sum();
        if total > TOTAL_MAX {
            match previous {
                Some(previous) => {
                    self.values.insert(key.to_string(), previous);
                }
                None => {
                    self.values.remove(key);
                }
            }
            return Err(StorageError::Full);
        }
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.values.remove(key).is_some()
    }
}

fn validate_key(key: &str) -> Result<(), StorageError> {
    if key.is_empty() || key.len() > KEY_MAX || key.chars().any(|ch| ch.is_control()) {
        return Err(StorageError::Key);
    }
    Ok(())
}

pub struct StorageHandle {
    path: PathBuf,
    state: ExtensionStorage,
}

impl StorageHandle {
    pub fn open(path: PathBuf) -> Self {
        let state = ExtensionStorage::load(&path).unwrap_or_default();
        Self { path, state }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.state.get(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.state.keys()
    }

    pub fn set(&mut self, key: &str, value: Value) -> Result<(), StorageError> {
        self.state.set(key, value)?;
        self.state.save(&self.path)
    }

    pub fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        let removed = self.state.delete(key);
        if removed {
            self.state.save(&self.path)?;
        }
        Ok(removed)
    }
}

fn restrict_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_values_and_keys() {
        let mut storage = ExtensionStorage::default();
        assert!(storage.set("", Value::Null).is_err());
        assert!(storage.set(&"k".repeat(KEY_MAX + 1), Value::Null).is_err());
        assert!(
            storage
                .set("big", Value::String("x".repeat(VALUE_MAX)))
                .is_err()
        );
        storage.set("ok", Value::String("value".into())).unwrap();
        assert_eq!(storage.keys(), vec!["ok".to_string()]);
        assert!(storage.delete("ok"));
        assert!(!storage.delete("ok"));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("storage.json");
        let mut handle = StorageHandle::open(path.clone());
        handle.set("a", Value::from(1)).unwrap();
        let reopened = StorageHandle::open(path);
        assert_eq!(reopened.get("a"), Some(&Value::from(1)));
    }
}
