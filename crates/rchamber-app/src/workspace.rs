use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const WORKSPACE_VERSION: u32 = 3;

fn default_version() -> u32 {
    WORKSPACE_VERSION
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectSort {
    #[default]
    Manual,
    Recent,
    Name,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionGrouping {
    #[default]
    Project,
    Time,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayPrefs {
    pub grouping: SessionGrouping,
    pub project_sort: ProjectSort,
    pub detailed_rows: bool,
    pub branch_names: bool,
    pub show_projects: bool,
    pub show_chats: bool,
    pub show_in_progress: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
            grouping: SessionGrouping::Project,
            project_sort: ProjectSort::Manual,
            detailed_rows: false,
            branch_names: true,
            show_projects: true,
            show_chats: true,
            show_in_progress: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub canonical: String,
    pub name: String,
    pub name_override: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub hidden: bool,
    pub collapsed: bool,
    pub added_at: f64,
}

impl ProjectEntry {
    pub fn display_name(&self) -> &str {
        self.name_override.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionFolder {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub collapsed: bool,
    pub created_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceStore {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub folders: Vec<SessionFolder>,
    #[serde(default)]
    pub folder_sessions: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub active_project_id: Option<String>,
    #[serde(default)]
    pub last_session_id: Option<String>,
    #[serde(default)]
    pub last_session_by_project: HashMap<String, String>,
    #[serde(default)]
    pub new_session_drafts: HashMap<String, String>,
    #[serde(default)]
    pub allow_all_new_sessions: bool,
    #[serde(default)]
    pub collapsed_projects: HashSet<String>,
    #[serde(default)]
    pub closed_projects: HashSet<String>,
    #[serde(default)]
    pub collapsed_sections: HashSet<String>,
    #[serde(default)]
    pub hidden_sections: Vec<String>,
    #[serde(default)]
    pub display: DisplayPrefs,
}

impl Default for WorkspaceStore {
    fn default() -> Self {
        Self {
            version: WORKSPACE_VERSION,
            projects: Vec::new(),
            folders: Vec::new(),
            folder_sessions: HashMap::new(),
            order: Vec::new(),
            active_project_id: None,
            last_session_id: None,
            last_session_by_project: HashMap::new(),
            new_session_drafts: HashMap::new(),
            allow_all_new_sessions: false,
            collapsed_projects: HashSet::new(),
            closed_projects: HashSet::new(),
            collapsed_sections: HashSet::new(),
            hidden_sections: Vec::new(),
            display: DisplayPrefs::default(),
        }
    }
}

impl WorkspaceStore {
    pub fn path() -> Option<PathBuf> {
        let base = directories::BaseDirs::new()?;
        Some(base.config_dir().join("rustchamber").join("workspace.json"))
    }

    pub fn chats_directory() -> Option<PathBuf> {
        let base = directories::BaseDirs::new()?;
        let path = base.config_dir().join("rustchamber").join("chats");
        std::fs::create_dir_all(&path).ok()?;
        Some(path)
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        Self::load_path(&path)
    }

    fn load_path(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(_) => return Self::default(),
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(store) => accept_version(store).unwrap_or_default(),
            Err(_) => {
                let corrupt = corrupt_path(path);
                let _ = std::fs::rename(path, corrupt);
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        self.save_path(&path);
    }

    fn save_path(&self, path: &Path) {
        static SAVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = SAVE.lock().unwrap_or_else(|error| error.into_inner());
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return;
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let tmp = path.with_extension(format!("json.tmp-{}-{stamp}", std::process::id()));
        let write = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .and_then(|mut file| {
                use std::io::Write;
                file.write_all(text.as_bytes())?;
                file.sync_all()
            });
        if write.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(tmp, path);
    }

    pub fn add_local_project(&mut self, directory: &str, name: Option<&str>) -> String {
        let canonical = normalize_path(directory);
        self.closed_projects.remove(&canonical);
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| directory_name(&canonical));
        if let Some(entry) = self
            .projects
            .iter_mut()
            .find(|entry| paths_match(&entry.canonical, &canonical))
        {
            if name != directory_name(&entry.canonical) {
                entry.name_override = Some(name);
            }
            return entry.id.clone();
        }
        let id = format!("dir:{canonical}");
        self.projects.push(ProjectEntry {
            id: id.clone(),
            canonical,
            name,
            name_override: None,
            color: None,
            icon: None,
            hidden: false,
            collapsed: false,
            added_at: now_ms(),
        });
        self.order.push(id.clone());
        id
    }

    pub fn remove_project(&mut self, id: &str) {
        if let Some(project) = self.projects.iter().find(|project| project.id == id) {
            self.closed_projects.insert(project.canonical.clone());
        }
        let folder_ids: Vec<String> = self
            .folders
            .iter()
            .filter(|folder| folder.project_id == id)
            .map(|folder| folder.id.clone())
            .collect();
        for folder_id in folder_ids {
            self.delete_folder(&folder_id);
        }
        self.projects.retain(|project| project.id != id);
        self.order.retain(|project| project != id);
        self.collapsed_projects.remove(id);
        self.last_session_by_project.remove(id);
        if self.active_project_id.as_deref() == Some(id) {
            self.active_project_id = None;
        }
    }

    pub fn create_folder(&mut self, project_id: &str, name: &str) -> String {
        let stamp = now_ms() as u64;
        let mut seq = 0u64;
        let id = loop {
            let candidate = format!("folder:{stamp}:{seq}");
            if !self.folders.iter().any(|folder| folder.id == candidate)
                && !self.folder_sessions.contains_key(&candidate)
            {
                break candidate;
            }
            seq += 1;
        };
        let name = name.trim();
        self.folders.push(SessionFolder {
            id: id.clone(),
            project_id: project_id.to_string(),
            name: if name.is_empty() {
                "New folder".into()
            } else {
                name.to_string()
            },
            collapsed: false,
            created_at: now_ms(),
        });
        id
    }

    pub fn rename_folder(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(folder) = self.folders.iter_mut().find(|folder| folder.id == id) {
            folder.name = name.to_string();
        }
    }

    pub fn delete_folder(&mut self, id: &str) {
        self.folders.retain(|folder| folder.id != id);
        self.folder_sessions.remove(id);
    }

    pub fn set_folder_collapsed(&mut self, id: &str, collapsed: bool) {
        if let Some(folder) = self.folders.iter_mut().find(|folder| folder.id == id) {
            folder.collapsed = collapsed;
        }
    }

    pub fn assign_session(&mut self, folder_id: &str, session_id: &str) {
        if !self.folders.iter().any(|folder| folder.id == folder_id) {
            return;
        }
        for (id, sessions) in &mut self.folder_sessions {
            if id != folder_id {
                sessions.retain(|session| session != session_id);
            }
        }
        self.folder_sessions
            .retain(|id, sessions| !sessions.is_empty() || id == folder_id);
        let sessions = self
            .folder_sessions
            .entry(folder_id.to_string())
            .or_default();
        sessions.retain(|session| session != session_id);
        sessions.push(session_id.to_string());
    }

    pub fn unassign_session(&mut self, session_id: &str) {
        for sessions in self.folder_sessions.values_mut() {
            sessions.retain(|session| session != session_id);
        }
        self.folder_sessions
            .retain(|_, sessions| !sessions.is_empty());
    }

    pub fn folder_for_session(&self, session_id: &str) -> Option<&SessionFolder> {
        self.folders.iter().find(|folder| {
            self.folder_sessions
                .get(&folder.id)
                .is_some_and(|sessions| sessions.iter().any(|session| session == session_id))
        })
    }

    pub fn folders_for_project(&self, project_id: &str) -> Vec<&SessionFolder> {
        self.folders
            .iter()
            .filter(|folder| folder.project_id == project_id)
            .collect()
    }

    pub fn sessions_in_folder(&self, folder_id: &str) -> &[String] {
        match self.folder_sessions.get(folder_id) {
            Some(sessions) => sessions,
            None => &[],
        }
    }

    pub fn ordered_projects(&self) -> Vec<&ProjectEntry> {
        let mut projects: Vec<&ProjectEntry> = self
            .projects
            .iter()
            .filter(|project| !project.hidden)
            .collect();
        match self.display.project_sort {
            ProjectSort::Manual => {
                let rank: HashMap<&str, usize> = self
                    .order
                    .iter()
                    .enumerate()
                    .map(|(index, id)| (id.as_str(), index))
                    .collect();
                projects.sort_by(|a, b| {
                    match (
                        rank.get(a.id.as_str()).copied(),
                        rank.get(b.id.as_str()).copied(),
                    ) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            ProjectSort::Recent => projects.sort_by(|a, b| {
                b.added_at
                    .partial_cmp(&a.added_at)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            ProjectSort::Name => projects.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            }),
        }
        projects
    }

    pub fn mark_active_project(&mut self, id: &str) {
        self.active_project_id = Some(id.to_string());
    }

    pub fn project_for_directory(&self, directory: &str) -> Option<&ProjectEntry> {
        self.projects
            .iter()
            .find(|project| paths_match(&project.canonical, directory))
    }

    pub fn project_for_session(
        &self,
        session: &rchamber_api::models::Session,
    ) -> Option<&ProjectEntry> {
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == session.project_id)
        {
            return Some(project);
        }
        let directory = session.directory()?;
        self.project_for_directory(directory)
    }

    pub fn note_session(&mut self, session: &rchamber_api::models::Session) {
        self.last_session_id = Some(session.id.clone());
        if let Some(project) = self.project_for_session(session) {
            let id = project.id.clone();
            self.last_session_by_project.insert(id, session.id.clone());
        }
    }
}

pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    let mut normalized = trimmed;
    while normalized.len() > 1
        && (normalized.ends_with('/') || normalized.ends_with('\\'))
        && !normalized.ends_with(":\\")
    {
        normalized = &normalized[..normalized.len() - 1];
    }
    if normalized.is_empty() {
        return String::new();
    }
    match Path::new(normalized).canonicalize() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => normalized.to_string(),
    }
}

pub fn paths_match(left: &str, right: &str) -> bool {
    if normalize_path(left) == normalize_path(right) {
        return true;
    }
    match (
        Path::new(left).canonicalize(),
        Path::new(right).canonicalize(),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn directory_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn corrupt_path(path: &Path) -> PathBuf {
    path.with_file_name(format!("workspace.corrupt-{}.json", now_ms() as u64))
}

fn accept_version(mut store: WorkspaceStore) -> Option<WorkspaceStore> {
    if store.version > WORKSPACE_VERSION {
        return None;
    }
    store.version = WORKSPACE_VERSION;
    Some(store)
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

    fn entry(id: &str, canonical: &str, name: &str, added_at: f64) -> ProjectEntry {
        ProjectEntry {
            id: id.into(),
            canonical: canonical.into(),
            name: name.into(),
            name_override: None,
            color: None,
            icon: None,
            hidden: false,
            collapsed: false,
            added_at,
        }
    }

    #[test]
    fn roundtrip_preserves_store_fields() {
        let mut store = WorkspaceStore::default();
        store
            .projects
            .push(entry("p1", "/tmp/alpha", "Alpha", 10.0));
        store.order.push("p1".into());
        store.active_project_id = Some("p1".into());
        store.last_session_id = Some("ses_1".into());
        store
            .last_session_by_project
            .insert("p1".into(), "ses_1".into());
        store.new_session_drafts.insert("p1".into(), "draft".into());
        store.collapsed_projects.insert("p1".into());
        store.collapsed_sections.insert("pinned".into());
        store.hidden_sections.push("chats".into());
        store.display.grouping = SessionGrouping::Time;
        store.display.project_sort = ProjectSort::Name;
        store.display.detailed_rows = true;

        let text = serde_json::to_string(&store).unwrap();
        let restored: WorkspaceStore = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.version, WORKSPACE_VERSION);
        assert_eq!(restored.projects.len(), 1);
        assert_eq!(restored.order, vec!["p1"]);
        assert_eq!(restored.active_project_id.as_deref(), Some("p1"));
        assert_eq!(restored.last_session_id.as_deref(), Some("ses_1"));
        assert_eq!(
            restored
                .last_session_by_project
                .get("p1")
                .map(String::as_str),
            Some("ses_1")
        );
        assert_eq!(
            restored.new_session_drafts.get("p1").map(String::as_str),
            Some("draft")
        );
        assert!(restored.collapsed_projects.contains("p1"));
        assert!(restored.collapsed_sections.contains("pinned"));
        assert_eq!(restored.hidden_sections, vec!["chats"]);
        assert_eq!(restored.display.grouping, SessionGrouping::Time);
        assert_eq!(restored.display.project_sort, ProjectSort::Name);
        assert!(restored.display.detailed_rows);
    }

    #[test]
    fn manual_order_uses_order_then_appends_unknown() {
        let mut store = WorkspaceStore::default();
        store.projects.push(entry("a", "/tmp/a", "A", 1.0));
        store.projects.push(entry("b", "/tmp/b", "B", 2.0));
        store.projects.push(entry("c", "/tmp/c", "C", 3.0));
        store.order = vec!["c".into(), "a".into()];

        let ordered: Vec<&str> = store
            .ordered_projects()
            .into_iter()
            .map(|project| project.id.as_str())
            .collect();
        assert_eq!(ordered, vec!["c", "a", "b"]);
    }

    #[test]
    fn path_normalization_matches_trailing_slashes_and_projects() {
        assert_eq!(
            normalize_path("/tmp/example/"),
            normalize_path("/tmp/example")
        );
        assert!(paths_match("/tmp/example/", "/tmp/example"));

        let mut store = WorkspaceStore::default();
        store
            .projects
            .push(entry("p1", "/tmp/example/", "Example", 1.0));
        assert_eq!(
            store
                .project_for_directory("/tmp/example")
                .map(|project| project.id.as_str()),
            Some("p1")
        );
    }

    #[test]
    fn folders_create_rename_and_delete() {
        let mut store = WorkspaceStore::default();
        store.projects.push(entry("p1", "/tmp/a", "A", 1.0));
        store.projects.push(entry("p2", "/tmp/b", "B", 2.0));
        let first = store.create_folder("p1", "  Alpha  ");
        let second = store.create_folder("p1", "Beta");
        let chat = store.create_folder("chats", "Quick chats");

        assert!(first.starts_with("folder:"));
        assert_ne!(first, second);
        assert_eq!(store.folders[0].name, "Alpha");
        assert_eq!(store.folders[0].project_id, "p1");
        assert!(store.folders[0].created_at > 0.0);
        let ids: Vec<&str> = store
            .folders_for_project("p1")
            .into_iter()
            .map(|folder| folder.id.as_str())
            .collect();
        assert_eq!(ids, vec![first.as_str(), second.as_str()]);
        assert_eq!(store.folders_for_project("chats")[0].id, chat);

        store.rename_folder(&first, "Renamed");
        assert_eq!(store.folders[0].name, "Renamed");
        store.rename_folder(&first, "   ");
        assert_eq!(store.folders[0].name, "Renamed");

        store.assign_session(&first, "ses_1");
        store.assign_session(&second, "ses_2");
        store.delete_folder(&first);
        assert!(
            store
                .folders_for_project("p1")
                .iter()
                .all(|f| f.id != first)
        );
        assert!(!store.folder_sessions.contains_key(&first));
        assert_eq!(store.sessions_in_folder(&second), ["ses_2".to_string()]);
    }

    #[test]
    fn assigning_a_session_moves_it_between_folders() {
        let mut store = WorkspaceStore::default();
        let first = store.create_folder("p1", "First");
        let second = store.create_folder("p1", "Second");

        store.assign_session(&first, "ses_1");
        assert_eq!(
            store
                .folder_for_session("ses_1")
                .map(|folder| folder.id.as_str()),
            Some(first.as_str())
        );

        store.assign_session(&second, "ses_1");
        assert!(store.sessions_in_folder(&first).is_empty());
        assert_eq!(store.sessions_in_folder(&second), ["ses_1".to_string()]);
        assert_eq!(
            store
                .folder_for_session("ses_1")
                .map(|folder| folder.id.as_str()),
            Some(second.as_str())
        );

        store.assign_session(&second, "ses_1");
        assert_eq!(store.sessions_in_folder(&second).len(), 1);

        store.unassign_session("ses_1");
        assert!(store.folder_for_session("ses_1").is_none());
        assert!(store.folder_sessions.is_empty());
    }

    #[test]
    fn deleting_a_project_cleans_up_its_folders() {
        let mut store = WorkspaceStore::default();
        store.projects.push(entry("p1", "/tmp/a", "A", 1.0));
        store.projects.push(entry("p2", "/tmp/b", "B", 2.0));
        let first = store.create_folder("p1", "First");
        let second = store.create_folder("p2", "Second");
        store.assign_session(&first, "ses_1");
        store.assign_session(&second, "ses_2");

        store.remove_project("p1");

        assert!(store.folders_for_project("p1").is_empty());
        assert!(!store.folder_sessions.contains_key(&first));
        assert_eq!(store.sessions_in_folder(&second), ["ses_2".to_string()]);
    }

    #[test]
    fn version_one_loads_with_empty_folders() {
        let text = r#"{
            "version": 1,
            "projects": [],
            "order": ["p1"],
            "collapsed_projects": ["p1"],
            "display": {}
        }"#;
        let parsed: WorkspaceStore = serde_json::from_str(text).unwrap();
        let store = accept_version(parsed).expect("v1 is accepted");

        assert_eq!(store.version, WORKSPACE_VERSION);
        assert_eq!(store.order, vec!["p1"]);
        assert!(store.collapsed_projects.contains("p1"));
        assert!(store.folders.is_empty());
        assert!(store.folder_sessions.is_empty());
    }

    #[test]
    fn newer_versions_are_rejected() {
        let store = WorkspaceStore {
            version: WORKSPACE_VERSION + 1,
            ..WorkspaceStore::default()
        };
        assert!(accept_version(store).is_none());
    }

    #[test]
    fn folder_roundtrip_preserves_assignments_and_collapse() {
        let mut store = WorkspaceStore::default();
        store.projects.push(entry("p1", "/tmp/a", "A", 1.0));
        let folder = store.create_folder("p1", "Alpha");
        store.assign_session(&folder, "ses_1");
        store.set_folder_collapsed(&folder, true);

        let directory = std::env::temp_dir().join(format!(
            "rchamber-workspace-{}-{}",
            std::process::id(),
            now_ms() as u64
        ));
        let path = directory.join("workspace.json");
        store.save_path(&path);
        let restored = WorkspaceStore::load_path(&path);
        let _ = std::fs::remove_dir_all(&directory);

        assert_eq!(restored.version, WORKSPACE_VERSION);
        assert_eq!(restored.folders.len(), 1);
        assert_eq!(restored.folders[0].name, "Alpha");
        assert!(restored.folders[0].collapsed);
        assert_eq!(
            restored
                .folder_for_session("ses_1")
                .map(|entry| entry.id.as_str()),
            Some(folder.as_str())
        );
        assert_eq!(restored.sessions_in_folder(&folder), ["ses_1".to_string()]);
    }
}
