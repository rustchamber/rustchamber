use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const CONNECTIONS_VERSION: u32 = 2;

/// Key used for projects that belong to the managed local server rather than a
/// saved connection.
pub const LOCAL_SERVER_KEY: &str = "local";

pub fn server_key(connection: Option<&str>) -> String {
    connection.unwrap_or(LOCAL_SERVER_KEY).to_string()
}

fn default_version() -> u32 {
    0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    #[default]
    Endpoint,
    Ssh,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub kind: ConnectionKind,
    pub endpoint: String,
    pub ssh: String,
    pub remote_directory: String,
    pub password: Option<String>,
    pub created_at: f64,
    pub last_used: f64,
}

impl fmt::Debug for SavedConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedConnection")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("ssh", &self.ssh)
            .field("remote_directory", &self.remote_directory)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("created_at", &self.created_at)
            .field("last_used", &self.last_used)
            .finish()
    }
}

impl SavedConnection {
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.summary()
        } else {
            self.name.clone()
        }
    }

    pub fn summary(&self) -> String {
        match self.kind {
            ConnectionKind::Endpoint => self.endpoint.clone(),
            ConnectionKind::Ssh => {
                if self.ssh.trim().is_empty() {
                    "ssh server".into()
                } else {
                    rchamber_api::parse_target(&self.ssh)
                        .map(|target| target.summary())
                        .unwrap_or_else(|_| self.ssh.clone())
                }
            }
        }
    }

    pub fn is_ssh(&self) -> bool {
        self.kind == ConnectionKind::Ssh
    }
}

/// A dialog value before the backend assigns or updates the stored entry.
#[derive(Clone)]
pub struct ConnectionDraft {
    pub id: Option<String>,
    pub name: String,
    pub kind: ConnectionKind,
    pub value: String,
    pub remote_directory: String,
    pub password: Option<String>,
}

impl fmt::Debug for ConnectionDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionDraft")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("value", &self.value)
            .field("remote_directory", &self.remote_directory)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnectionsStore {
    #[serde(default = "default_version")]
    pub version: u32,
    pub active: Option<String>,
    pub connections: Vec<SavedConnection>,
    #[serde(skip)]
    pub read_only: bool,
}

impl Default for ConnectionsStore {
    fn default() -> Self {
        Self {
            version: CONNECTIONS_VERSION,
            active: None,
            connections: Vec::new(),
            read_only: false,
        }
    }
}

impl ConnectionsStore {
    pub fn path() -> Option<PathBuf> {
        let base = directories::BaseDirs::new()?;
        Some(
            base.config_dir()
                .join("rustchamber")
                .join("connections.json"),
        )
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        Self::load_path(&path)
    }

    fn load_path(path: &Path) -> Self {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read connections");
                return Self {
                    read_only: true,
                    ..Self::default()
                };
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                tracing::warn!(path = %path.display(), "connections file is not valid UTF-8");
                let corrupt =
                    path.with_file_name(format!("connections.corrupt-{}.json", now_ms() as u64));
                let _ = std::fs::rename(path, corrupt);
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(store) => match accept_version(store) {
                Some(store) => store,
                None => {
                    tracing::warn!(path = %path.display(), "connections file is from a newer version");
                    let backup =
                        path.with_file_name(format!("connections.future-{}.json", now_ms() as u64));
                    let _ = std::fs::copy(path, backup);
                    Self {
                        read_only: true,
                        ..Self::default()
                    }
                }
            },
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "connections file is corrupt");
                let corrupt =
                    path.with_file_name(format!("connections.corrupt-{}.json", now_ms() as u64));
                let _ = std::fs::rename(path, corrupt);
                Self::default()
            }
        }
    }

    /// Seed `connections.json` from the legacy settings endpoint once.
    ///
    /// The settings fields are not consulted again after the file exists.
    pub fn migrate_legacy(&mut self, endpoint: Option<&str>, password: Option<&str>) -> bool {
        let Some(path) = Self::path() else {
            return false;
        };
        self.migrate_legacy_path(&path, endpoint, password)
    }

    fn migrate_legacy_path(
        &mut self,
        path: &Path,
        endpoint: Option<&str>,
        password: Option<&str>,
    ) -> bool {
        if path.exists() {
            return false;
        }
        let mut migrated = false;
        if let Some(endpoint) = endpoint.map(str::trim).filter(|value| !value.is_empty()) {
            let id = self.fresh_id(now_ms());
            let entry = SavedConnection {
                id: id.clone(),
                name: fallback_name(ConnectionKind::Endpoint, endpoint),
                kind: ConnectionKind::Endpoint,
                endpoint: endpoint.to_string(),
                ssh: String::new(),
                remote_directory: String::new(),
                password: password
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                created_at: now_ms(),
                last_used: now_ms(),
            };
            self.connections.push(entry);
            self.active = Some(id);
            migrated = true;
        }
        if migrated {
            self.save_path(path)
        } else {
            false
        }
    }

    pub fn save(&self) -> bool {
        let Some(path) = Self::path() else {
            return false;
        };
        self.save_path(&path)
    }

    fn save_path(&self, path: &Path) -> bool {
        if self.read_only {
            tracing::warn!(path = %path.display(), "refusing to overwrite an unreadable connections file");
            return false;
        }
        static SAVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = SAVE.lock().unwrap_or_else(|error| error.into_inner());
        let Some(parent) = path.parent() else {
            return false;
        };
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(path = %path.display(), %error, "could not create connections directory");
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
        let text = match serde_json::to_string_pretty(self) {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(%error, "could not serialize connections");
                return false;
            }
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let tmp = path.with_extension(format!("json.tmp-{}-{stamp}", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let write = options.open(&tmp).and_then(|mut file| {
            use std::io::Write;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        if let Err(error) = write {
            tracing::warn!(path = %path.display(), %error, "could not write connections");
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        if let Err(error) = std::fs::rename(&tmp, path) {
            tracing::warn!(path = %path.display(), %error, "could not replace connections");
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        true
    }

    pub fn connections(&self) -> &[SavedConnection] {
        &self.connections
    }

    pub fn catalog(&self) -> Vec<SavedConnection> {
        self.connections.clone()
    }

    pub fn find(&self, id: &str) -> Option<&SavedConnection> {
        self.connections
            .iter()
            .find(|connection| connection.id == id)
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn active_connection(&self) -> Option<&SavedConnection> {
        self.active.as_deref().and_then(|id| self.find(id))
    }

    pub fn set_active(&mut self, id: Option<String>, now: f64) {
        let id = id.filter(|id| self.find(id).is_some());
        if let Some(id) = &id
            && let Some(connection) = self.connections.iter_mut().find(|entry| entry.id == *id)
        {
            connection.last_used = now;
        }
        self.active = id;
    }

    pub fn upsert(&mut self, draft: ConnectionDraft, now: f64) -> String {
        let value = draft.value.trim().to_string();
        let name = draft.name.trim().to_string();
        let name = if name.is_empty() {
            fallback_name(draft.kind, &value)
        } else {
            name
        };
        let password = match draft.kind {
            ConnectionKind::Endpoint => draft
                .password
                .map(|password| password.trim().to_string())
                .filter(|password| !password.is_empty()),
            ConnectionKind::Ssh => None,
        };
        let remote_directory = if draft.kind == ConnectionKind::Ssh {
            draft.remote_directory.trim().to_string()
        } else {
            String::new()
        };
        if let Some(id) = draft.id.as_deref()
            && let Some(connection) = self.connections.iter_mut().find(|entry| entry.id == id)
        {
            connection.name = name;
            connection.kind = draft.kind;
            connection.endpoint = if draft.kind == ConnectionKind::Endpoint {
                value.clone()
            } else {
                String::new()
            };
            connection.ssh = if draft.kind == ConnectionKind::Ssh {
                value
            } else {
                String::new()
            };
            connection.remote_directory = remote_directory;
            connection.password = password;
            return connection.id.clone();
        }
        let id = self.fresh_id(now);
        self.connections.push(SavedConnection {
            id: id.clone(),
            name,
            kind: draft.kind,
            endpoint: if draft.kind == ConnectionKind::Endpoint {
                value.clone()
            } else {
                String::new()
            },
            ssh: if draft.kind == ConnectionKind::Ssh {
                value
            } else {
                String::new()
            },
            remote_directory,
            password,
            created_at: now,
            last_used: 0.0,
        });
        id
    }

    pub fn remove(&mut self, id: &str) {
        self.connections.retain(|connection| connection.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
    }

    pub fn replace_entries(&mut self, entries: Vec<SavedConnection>, active: Option<String>) {
        self.connections = entries;
        self.active = active.filter(|id| self.connections.iter().any(|entry| entry.id == *id));
    }

    fn fresh_id(&self, now: f64) -> String {
        let stamp = now as u64;
        let mut sequence = 0u64;
        loop {
            let candidate = format!("conn:{stamp}:{sequence}");
            if !self
                .connections
                .iter()
                .any(|connection| connection.id == candidate)
            {
                return candidate;
            }
            sequence += 1;
        }
    }
}

fn accept_version(mut store: ConnectionsStore) -> Option<ConnectionsStore> {
    if store.version > CONNECTIONS_VERSION {
        return None;
    }
    store.version = CONNECTIONS_VERSION;
    if let Some(active) = store.active.clone()
        && !store.connections.iter().any(|entry| entry.id == active)
    {
        store.active = None;
    }
    Some(store)
}

fn fallback_name(kind: ConnectionKind, value: &str) -> String {
    match kind {
        ConnectionKind::Endpoint => {
            let without_scheme = value
                .split_once("://")
                .map(|(_, rest)| rest)
                .unwrap_or(value);
            let host = without_scheme
                .split('/')
                .next()
                .unwrap_or(without_scheme)
                .trim();
            if host.is_empty() {
                "Server".into()
            } else {
                host.to_string()
            }
        }
        ConnectionKind::Ssh => rchamber_api::parse_target(value)
            .map(|target| target.summary())
            .unwrap_or_else(|_| "SSH server".into()),
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(value: &str) -> ConnectionDraft {
        ConnectionDraft {
            id: None,
            name: String::new(),
            kind: ConnectionKind::Endpoint,
            value: value.into(),
            remote_directory: String::new(),
            password: Some("secret".into()),
        }
    }

    #[test]
    fn roundtrip_preserves_entries_and_active_connection() {
        let mut store = ConnectionsStore::default();
        let id = store.upsert(draft("http://127.0.0.1:4100"), 10.0);
        store.set_active(Some(id.clone()), 11.0);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        store.save_path(&path);
        let restored = ConnectionsStore::load_path(&path);

        assert_eq!(restored.version, CONNECTIONS_VERSION);
        assert_eq!(restored.connections.len(), 1);
        assert_eq!(restored.connections[0].id, id);
        assert_eq!(restored.connections[0].endpoint, "http://127.0.0.1:4100");
        assert_eq!(restored.connections[0].password.as_deref(), Some("secret"));
        assert_eq!(restored.active_id(), Some(id.as_str()));
        assert_eq!(
            restored.active_connection().map(|entry| entry.last_used),
            Some(11.0)
        );
    }

    #[test]
    fn old_files_load_with_defaults() {
        let text = r#"{
            "version": 1,
            "connections": [
                {"id":"conn:1:0","name":"Old","kind":"endpoint","endpoint":"http://127.0.0.1:1"}
            ]
        }"#;
        let store: ConnectionsStore = serde_json::from_str(text).unwrap();
        assert!(store.active.is_none());
        assert_eq!(store.connections[0].endpoint, "http://127.0.0.1:1");
        assert!(store.connections[0].ssh.is_empty());
        assert!(store.connections[0].remote_directory.is_empty());
        assert_eq!(store.connections[0].last_used, 0.0);
    }

    #[test]
    fn ssh_drafts_store_no_password_and_keep_the_raw_command() {
        let mut store = ConnectionsStore::default();
        let id = store.upsert(
            ConnectionDraft {
                id: None,
                name: "Remote".into(),
                kind: ConnectionKind::Ssh,
                value: "ssh -p 2222 me@example.com".into(),
                remote_directory: "/srv/project".into(),
                password: Some("ignored".into()),
            },
            5.0,
        );
        let entry = store.find(&id).unwrap();
        assert!(entry.is_ssh());
        assert_eq!(entry.ssh, "ssh -p 2222 me@example.com");
        assert!(entry.endpoint.is_empty());
        assert_eq!(entry.remote_directory, "/srv/project");
        assert!(entry.password.is_none());
        assert_eq!(entry.name, "Remote");
    }

    #[test]
    fn upsert_updates_existing_and_remove_clears_active() {
        let mut store = ConnectionsStore::default();
        let id = store.upsert(draft("http://127.0.0.1:4100"), 1.0);
        store.set_active(Some(id.clone()), 2.0);
        store.upsert(
            ConnectionDraft {
                id: Some(id.clone()),
                name: "Renamed".into(),
                kind: ConnectionKind::Endpoint,
                value: "http://127.0.0.1:4200".into(),
                remote_directory: String::new(),
                password: None,
            },
            3.0,
        );
        assert_eq!(store.connections.len(), 1);
        assert_eq!(store.connections[0].name, "Renamed");
        assert_eq!(store.connections[0].endpoint, "http://127.0.0.1:4200");
        assert!(store.connections[0].password.is_none());
        store.remove(&id);
        assert!(store.connections.is_empty());
        assert!(store.active.is_none());
    }

    #[test]
    fn corrupt_and_future_files_move_aside() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        std::fs::write(&path, "not json").unwrap();
        let store = ConnectionsStore::load_path(&path);
        assert!(store.connections.is_empty());
        assert!(!path.exists());
        let moved: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("connections.corrupt-"))
            .collect();
        assert_eq!(moved.len(), 1);

        let future = ConnectionsStore {
            version: CONNECTIONS_VERSION + 1,
            ..ConnectionsStore::default()
        };
        std::fs::write(&path, serde_json::to_string(&future).unwrap()).unwrap();
        let store = ConnectionsStore::load_path(&path);
        assert!(store.connections.is_empty());
        let backups: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("connections.future-"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn legacy_settings_endpoint_becomes_the_only_initial_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        let mut store = ConnectionsStore::default();
        assert!(store.migrate_legacy_path(&path, Some("http://127.0.0.1:5100"), Some("legacy")));

        let restored = ConnectionsStore::load_path(&path);
        assert_eq!(restored.connections.len(), 1);
        assert_eq!(restored.connections[0].name, "127.0.0.1:5100");
        assert_eq!(restored.connections[0].password.as_deref(), Some("legacy"));
        assert_eq!(
            restored.active_id(),
            Some(restored.connections[0].id.as_str())
        );

        // Once the file exists the legacy settings values never seed again.
        assert!(!store.migrate_legacy_path(&path, Some("http://127.0.0.1:5200"), Some("other")));
        let restored = ConnectionsStore::load_path(&path);
        assert_eq!(restored.connections.len(), 1);
    }

    #[test]
    fn replacing_entries_adopts_and_cleans_the_active_id() {
        let mut store = ConnectionsStore::default();
        let id = store.upsert(draft("http://127.0.0.1:4100"), 1.0);
        store.set_active(Some(id.clone()), 2.0);
        store.replace_entries(store.connections().to_vec(), Some(id.clone()));
        assert_eq!(store.active_id(), Some(id.as_str()));
        store.replace_entries(Vec::new(), Some(id));
        assert!(store.active.is_none());
    }

    #[test]
    fn a_failed_atomic_write_leaves_the_existing_file_alone() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("blocked");
        std::fs::write(&blocked, "occupied").unwrap();
        let store = ConnectionsStore::default();
        assert!(!store.save_path(&blocked.join("connections.json")));
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "occupied");
    }

    #[test]
    fn unversioned_files_load_and_sanitize_a_dangling_active_id() {
        let text = r#"{
            "connections": [
                {"id":"conn:1:0","name":"Old","kind":"endpoint","endpoint":"http://127.0.0.1:1"}
            ],
            "active": "conn:missing"
        }"#;
        let parsed: ConnectionsStore = serde_json::from_str(text).unwrap();
        assert_eq!(parsed.version, 0);
        let store = accept_version(parsed).unwrap();
        assert_eq!(store.version, CONNECTIONS_VERSION);
        assert!(store.active.is_none());
        assert_eq!(store.connections.len(), 1);
    }

    #[test]
    fn migration_reports_failure_when_the_file_cannot_be_written() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("blocked");
        std::fs::write(&blocked, "occupied").unwrap();
        let mut store = ConnectionsStore::default();
        assert!(!store.migrate_legacy_path(
            &blocked.join("connections.json"),
            Some("http://127.0.0.1:5100"),
            None
        ));
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "occupied");
    }

    #[test]
    fn invalid_utf8_moves_aside_instead_of_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let store = ConnectionsStore::load_path(&path);
        assert!(store.connections.is_empty());
        assert!(!path.exists());
        let moved: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("connections.corrupt-"))
            .collect();
        assert_eq!(moved.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn saved_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        let store = ConnectionsStore::default();
        assert!(store.save_path(&path));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(directory.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
