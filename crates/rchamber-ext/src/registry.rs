use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::{Manifest, ManifestError};
use crate::storage::{ExtensionStorage, StorageError};

const REGISTRY_FILE: &str = "extensions.json";
const PACKAGES_DIR: &str = "extensions";
const STORAGE_DIR: &str = "extension-storage";
const ZIP_MAX_FILES: usize = 500;
const ZIP_MAX_BYTES: u64 = 40 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InstallSource {
    Path { path: PathBuf },
    Zip,
    Builtin,
}

impl InstallSource {
    pub fn label(&self) -> &'static str {
        match self {
            InstallSource::Path { .. } => "folder",
            InstallSource::Zip => "archive",
            InstallSource::Builtin => "built-in",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub manifest: Manifest,
    pub root: PathBuf,
    pub source: InstallSource,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub grants: Vec<String>,
    #[serde(default)]
    pub installed_at: u64,
    #[serde(default)]
    pub settings: serde_json::Value,
}

impl InstalledExtension {
    pub fn is_active(&self) -> bool {
        self.enabled && self.granted_all()
    }

    pub fn granted_all(&self) -> bool {
        let requested_caps = self.manifest.requested_capabilities();
        let requested: BTreeSet<&String> = requested_caps.iter().collect();
        let granted: BTreeSet<&String> = self.grants.iter().collect();
        requested.is_subset(&granted)
    }

    pub fn needs_approval(&self) -> bool {
        !self.granted_all()
    }

    pub fn capability(&self, name: &str) -> bool {
        self.is_active()
            && self
                .grants
                .iter()
                .any(|capability| capability == name || capability.starts_with(&format!("{name}:")))
    }

    pub fn storage_path(&self, registry_root: &Path) -> PathBuf {
        registry_root
            .join(STORAGE_DIR)
            .join(format!("{}.json", self.manifest.id))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{0}")]
    Manifest(#[from] ManifestError),
    #[error("extension {0} is already installed")]
    AlreadyInstalled(String),
    #[error("extension {0} is not installed")]
    NotFound(String),
    #[error("the package is too large")]
    TooLarge,
    #[error("the package has too many files")]
    TooManyFiles,
    #[error("the package contains an unsafe path")]
    UnsafePath,
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read the archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("extension storage: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SavedRegistry {
    #[serde(default)]
    extensions: Vec<InstalledExtension>,
}

pub struct Registry {
    root: PathBuf,
    extensions: Vec<InstalledExtension>,
}

impl Registry {
    pub fn load(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let path = root.join(REGISTRY_FILE);
        let mut extensions = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<SavedRegistry>(&bytes)
                .map(|saved| saved.extensions)
                .unwrap_or_else(|error| {
                    tracing::warn!("ignoring corrupt {}: {error}", path.display());
                    Vec::new()
                }),
            Err(_) => Vec::new(),
        };
        for extension in &mut extensions {
            if let InstallSource::Path { path } = &extension.source
                && let Ok(manifest) = Manifest::load(path)
            {
                extension.manifest = manifest;
            }
        }
        let packages = root.join(PACKAGES_DIR);
        extensions.retain(|extension| {
            let valid = match &extension.source {
                InstallSource::Path { .. } => true,
                InstallSource::Zip => {
                    extension.manifest.validate(&extension.root).is_ok()
                        && extension.root.starts_with(&packages)
                }
                InstallSource::Builtin => extension.manifest.validate(&extension.root).is_ok(),
            };
            if !valid {
                tracing::warn!(
                    "ignoring extension {} at {}",
                    extension.manifest.id,
                    extension.root.display()
                );
            }
            valid
        });
        Self { root, extensions }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> &[InstalledExtension] {
        &self.extensions
    }

    pub fn get(&self, id: &str) -> Option<&InstalledExtension> {
        self.extensions
            .iter()
            .find(|extension| extension.manifest.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut InstalledExtension> {
        self.extensions
            .iter_mut()
            .find(|extension| extension.manifest.id == id)
    }

    pub fn save(&self) -> Result<(), RegistryError> {
        std::fs::create_dir_all(&self.root).map_err(|source| RegistryError::Io {
            path: self.root.clone(),
            source,
        })?;
        let path = self.root.join(REGISTRY_FILE);
        let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        let bytes = serde_json::to_vec_pretty(&SavedRegistry {
            extensions: self.extensions.clone(),
        })
        .map_err(|error| RegistryError::Invalid(error.to_string()))?;
        std::fs::write(&tmp, bytes).map_err(|source| RegistryError::Io {
            path: tmp.clone(),
            source,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &path).map_err(|source| RegistryError::Io { path, source })?;
        Ok(())
    }

    pub fn install_path(&mut self, path: &Path) -> Result<InstalledExtension, RegistryError> {
        let path = path.canonicalize().map_err(|source| RegistryError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !path.is_dir() {
            return Err(RegistryError::Invalid(
                "the path must point to an extension folder".into(),
            ));
        }
        let manifest = Manifest::load(&path)?;
        crate::check::check_syntax(&path, &manifest).map_err(RegistryError::Invalid)?;
        if self.get(&manifest.id).is_some() {
            return Err(RegistryError::AlreadyInstalled(manifest.id));
        }
        let extension = InstalledExtension {
            manifest,
            root: path.clone(),
            source: InstallSource::Path { path },
            enabled: false,
            grants: Vec::new(),
            installed_at: now_millis(),
            settings: serde_json::Value::Null,
        };
        self.extensions.push(extension.clone());
        self.save()?;
        Ok(extension)
    }

    pub fn install_zip_path(
        &mut self,
        archive: &Path,
    ) -> Result<InstalledExtension, RegistryError> {
        let file = std::fs::File::open(archive).map_err(|source| RegistryError::Io {
            path: archive.to_path_buf(),
            source,
        })?;
        let mut zip = zip::ZipArchive::new(file)?;
        if zip.len() > ZIP_MAX_FILES {
            return Err(RegistryError::TooManyFiles);
        }
        let staging = self
            .root
            .join(PACKAGES_DIR)
            .join(format!(".staging-{}", std::process::id()));
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        std::fs::create_dir_all(&staging).map_err(|source| RegistryError::Io {
            path: staging.clone(),
            source,
        })?;
        let mut total = 0u64;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let Some(relative) = entry.enclosed_name() else {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(RegistryError::UnsafePath);
            };
            if entry.is_dir() {
                continue;
            }
            let target = staging.join(&relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|source| RegistryError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut output =
                std::fs::File::create(&target).map_err(|source| RegistryError::Io {
                    path: target.clone(),
                    source,
                })?;
            let remaining = ZIP_MAX_BYTES.saturating_sub(total);
            let copied = std::io::copy(
                &mut std::io::Read::take(&mut entry, remaining.saturating_add(1)),
                &mut output,
            )
            .map_err(|source| RegistryError::Io {
                path: target,
                source,
            })?;
            if copied > remaining {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(RegistryError::TooLarge);
            }
            total = total.saturating_add(copied);
        }
        let manifest = match Manifest::load(&staging) {
            Ok(manifest) => manifest,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error.into());
            }
        };
        if let Err(error) = crate::check::check_syntax(&staging, &manifest) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(RegistryError::Invalid(error));
        }
        if self.get(&manifest.id).is_some() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(RegistryError::AlreadyInstalled(manifest.id));
        }
        let target = self.root.join(PACKAGES_DIR).join(&manifest.id);
        if target.exists() {
            let _ = std::fs::remove_dir_all(&target);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| RegistryError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::rename(&staging, &target).map_err(|source| RegistryError::Io {
            path: target.clone(),
            source,
        })?;
        let extension = InstalledExtension {
            manifest,
            root: target,
            source: InstallSource::Zip,
            enabled: false,
            grants: Vec::new(),
            installed_at: now_millis(),
            settings: serde_json::Value::Null,
        };
        self.extensions.push(extension.clone());
        self.save()?;
        Ok(extension)
    }

    pub fn approve(&mut self, id: &str, grants: Vec<String>) -> Result<(), RegistryError> {
        let requested: BTreeSet<String> = {
            let Some(extension) = self.get(id) else {
                return Err(RegistryError::NotFound(id.into()));
            };
            extension
                .manifest
                .requested_capabilities()
                .into_iter()
                .collect()
        };
        let grants: Vec<String> = grants
            .into_iter()
            .filter(|capability| requested.contains(capability))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !grants.is_empty() && grants.len() != requested.len() {
            return Err(RegistryError::Invalid(
                "approve the whole permission list or none of it".into(),
            ));
        }
        let Some(extension) = self.get_mut(id) else {
            return Err(RegistryError::NotFound(id.into()));
        };
        extension.grants = grants;
        extension.enabled = extension.granted_all();
        self.save()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), RegistryError> {
        let Some(extension) = self.get_mut(id) else {
            return Err(RegistryError::NotFound(id.into()));
        };
        if enabled && !extension.granted_all() {
            return Err(RegistryError::Invalid(
                "the extension needs its permissions approved first".into(),
            ));
        }
        extension.enabled = enabled;
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> Result<(), RegistryError> {
        let position = self
            .extensions
            .iter()
            .position(|extension| extension.manifest.id == id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        let extension = self.extensions.remove(position);
        if matches!(extension.source, InstallSource::Zip)
            && extension.root.starts_with(self.root.join(PACKAGES_DIR))
        {
            let _ = std::fs::remove_dir_all(&extension.root);
        }
        let storage = extension.storage_path(&self.root);
        let _ = std::fs::remove_file(storage);
        self.save()
    }

    pub fn reload(&mut self, id: &str) -> Result<(), RegistryError> {
        let Some(position) = self
            .extensions
            .iter()
            .position(|extension| extension.manifest.id == id)
        else {
            return Err(RegistryError::NotFound(id.into()));
        };
        let root = self.extensions[position].root.clone();
        let manifest = Manifest::load(&root)?;
        if manifest.id != id {
            return Err(RegistryError::Invalid(format!(
                "the package now declares a different id: {}",
                manifest.id
            )));
        }
        self.extensions[position].manifest = manifest;
        self.save()
    }

    pub fn storage(&self, id: &str) -> Result<ExtensionStorage, RegistryError> {
        let Some(extension) = self.get(id) else {
            return Err(RegistryError::NotFound(id.into()));
        };
        Ok(ExtensionStorage::load(&extension.storage_path(&self.root))?)
    }

    pub fn save_storage(&self, id: &str, storage: &ExtensionStorage) -> Result<(), RegistryError> {
        let Some(extension) = self.get(id) else {
            return Err(RegistryError::NotFound(id.into()));
        };
        storage.save(&extension.storage_path(&self.root))?;
        Ok(())
    }

    pub fn active(&self) -> impl Iterator<Item = &InstalledExtension> {
        self.extensions
            .iter()
            .filter(|extension| extension.is_active())
    }
}

pub fn default_root() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.config_dir().join("rustchamber"))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("extensions")
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(dir: &Path, id: &str, capabilities: &[&str]) -> PathBuf {
        let root = dir.join(id);
        std::fs::create_dir_all(&root).unwrap();
        let manifest = serde_json::json!({
            "id": id,
            "name": id,
            "version": "1.0.0",
            "capabilities": capabilities,
            "runtime": { "entry": "main.lua" },
            "contributes": { "statusSection": { "title": "Demo", "method": "status" } }
        });
        std::fs::write(root.join("rchamber.json"), manifest.to_string()).unwrap();
        std::fs::write(root.join("main.lua"), "function status() return {} end").unwrap();
        root
    }

    #[test]
    fn installs_and_requires_approval() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("registry");
        let source = package(dir.path(), "demo", &["storage"]);
        let mut registry = Registry::load(&root);
        let extension = registry.install_path(&source).unwrap();
        assert!(extension.needs_approval());
        assert!(!extension.is_active());
        registry.approve("demo", vec!["storage".into()]).unwrap();
        assert!(registry.get("demo").unwrap().is_active());
        assert!(registry.get("demo").unwrap().capability("storage"));
    }

    #[test]
    fn refuses_partial_approval() {
        let dir = tempfile::tempdir().unwrap();
        let source = package(dir.path(), "demo", &["storage", "prompt"]);
        let mut registry = Registry::load(dir.path().join("registry"));
        registry.install_path(&source).unwrap();
        assert!(registry.approve("demo", vec!["storage".into()]).is_err());
        registry
            .approve("demo", vec!["storage".into(), "prompt".into()])
            .unwrap();
    }

    #[test]
    fn persists_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let source = package(dir.path(), "demo", &[]);
        let root = dir.path().join("registry");
        let mut registry = Registry::load(&root);
        registry.install_path(&source).unwrap();
        registry.approve("demo", Vec::new()).unwrap();
        let reloaded = Registry::load(&root);
        assert!(reloaded.get("demo").unwrap().enabled);
    }

    #[test]
    fn a_missing_folder_stays_listed_for_removal() {
        let dir = tempfile::tempdir().unwrap();
        let source = package(dir.path(), "demo", &[]);
        let root = dir.path().join("registry");
        let mut registry = Registry::load(&root);
        registry.install_path(&source).unwrap();
        std::fs::remove_dir_all(&source).unwrap();
        let reloaded = Registry::load(&root);
        assert!(reloaded.get("demo").is_some());
    }

    #[test]
    fn installs_a_zip_and_removes_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = package(dir.path(), "zipped", &[]);
        let archive = dir.path().join("zipped.zip");
        let file = std::fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("rchamber.json", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            std::fs::read(source.join("rchamber.json"))
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        zip.start_file("main.lua", options).unwrap();
        std::io::Write::write_all(&mut zip, b"function status() return {} end").unwrap();
        zip.finish().unwrap();
        let root = dir.path().join("registry");
        let mut registry = Registry::load(&root);
        let installed = registry.install_zip_path(&archive).unwrap();
        assert!(installed.root.starts_with(root.join(PACKAGES_DIR)));
        registry.approve("zipped", Vec::new()).unwrap();
        registry.remove("zipped").unwrap();
        assert!(registry.get("zipped").is_none());
        assert!(!installed.root.exists());
    }
}
