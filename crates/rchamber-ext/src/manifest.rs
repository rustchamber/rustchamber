use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::widget::Widget;

pub const API_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "rchamber.json";

pub const ACTIONS_MAX: usize = 8;
pub const COMMANDS_MAX: usize = 8;
pub const TOOLS_MAX: usize = 16;
pub const BLOCKS_MAX: usize = 4;
pub const FILE_EDITORS_MAX: usize = 8;
pub const INSTRUCTIONS_MAX: usize = 4096;
pub const MENTIONS_LABEL_MAX: usize = 40;
pub const STATUS_INTERVAL_MIN: u64 = 1_000;
pub const STATUS_INTERVAL_DEFAULT: u64 = 5_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub contributes: Contributes,
}

fn default_api_version() -> u32 {
    API_VERSION
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    #[default]
    Lua,
    #[serde(rename = "process")]
    Process,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Runtime {
    #[serde(default)]
    pub kind: RuntimeKind,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub command: Vec<String>,
}

fn default_entry() -> String {
    "main.lua".to_string()
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            kind: RuntimeKind::Lua,
            entry: default_entry(),
            command: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contributes {
    #[serde(default)]
    pub panel: Option<PanelContribution>,
    #[serde(default)]
    pub status_section: Option<StatusSection>,
    #[serde(default)]
    pub tools: Vec<ToolRenderer>,
    #[serde(default)]
    pub mentions: Option<Mentions>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub actions: Vec<ActionContribution>,
    #[serde(default)]
    pub blocks: Vec<BlockContribution>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub file_editors: Vec<FileEditorContribution>,
    #[serde(default)]
    pub integration: Option<Integration>,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PanelContribution {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusSection {
    pub title: String,
    #[serde(default = "default_status_method")]
    pub method: String,
    #[serde(default = "default_status_interval")]
    pub interval: u64,
    #[serde(default)]
    pub requires_project: bool,
}

fn default_status_method() -> String {
    "status".to_string()
}

fn default_status_interval() -> u64 {
    STATUS_INTERVAL_DEFAULT
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolRenderer {
    #[serde(rename = "match")]
    pub matcher: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mentions {
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandContribution {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionContribution {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(rename = "where")]
    pub target: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockContribution {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub answers: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileEditorContribution {
    pub id: String,
    pub title: String,
    #[serde(rename = "match")]
    pub matcher: String,
    #[serde(default)]
    pub method: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub api_origin: Option<String>,
    #[serde(default)]
    pub token_label: Option<String>,
    #[serde(default)]
    pub settings: Vec<IntegrationSetting>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSetting {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Invalid(String),
}

impl Manifest {
    pub fn load(root: &Path) -> Result<Self, ManifestError> {
        let path = root.join(MANIFEST_FILE);
        let bytes = std::fs::read(&path).map_err(|source| ManifestError::Io {
            path: path.clone(),
            source,
        })?;
        let manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|source| ManifestError::Json {
                path: path.clone(),
                source,
            })?;
        manifest.validate(root)?;
        Ok(manifest)
    }

    pub fn validate(&self, root: &Path) -> Result<(), ManifestError> {
        if self.api_version != API_VERSION {
            return Err(ManifestError::Invalid(format!(
                "apiVersion must be {API_VERSION}, got {}",
                self.api_version
            )));
        }
        if !is_kebab(&self.id) || self.id.len() > 64 {
            return Err(ManifestError::Invalid(
                "id must be kebab-case, start with a letter and be at most 64 characters".into(),
            ));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err(ManifestError::Invalid(
                "name must be 1 to 80 characters".into(),
            ));
        }
        semver::Version::parse(&self.version)
            .map_err(|error| ManifestError::Invalid(format!("version must be semver: {error}")))?;
        if self.runtime.command.is_empty() {
            let entry = safe_relative(&self.runtime.entry).ok_or_else(|| {
                ManifestError::Invalid("runtime.entry must stay inside the package".into())
            })?;
            if !root.join(entry).is_file() {
                return Err(ManifestError::Invalid(format!(
                    "runtime entry {} does not exist",
                    self.runtime.entry
                )));
            }
        } else if self.runtime.kind != RuntimeKind::Process {
            return Err(ManifestError::Invalid(
                "runtime.command is only allowed with runtime.kind = \"process\"".into(),
            ));
        }
        self.validate_capabilities()?;
        self.contributes.validate()?;
        if let Some(integration) = &self.contributes.integration {
            let origin = integration.api_origin.as_deref().ok_or_else(|| {
                ManifestError::Invalid("integration.apiOrigin is required".into())
            })?;
            let parsed = url::Url::parse(origin).map_err(|error| {
                ManifestError::Invalid(format!("integration.apiOrigin is invalid: {error}"))
            })?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(ManifestError::Invalid(
                    "integration.apiOrigin must be an https origin without credentials".into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_capabilities(&self) -> Result<(), ManifestError> {
        for capability in &self.capabilities {
            let name = capability.split(':').next().unwrap_or_default();
            if !matches!(
                name,
                "storage"
                    | "prompt"
                    | "sessions"
                    | "files"
                    | "network"
                    | "model"
                    | "clipboard"
                    | "notifications"
                    | "exec"
                    | "instructions"
                    | "events"
            ) {
                return Err(ManifestError::Invalid(format!(
                    "unknown capability \"{capability}\""
                )));
            }
            if name == "network" {
                let Some(origin) = capability.strip_prefix("network:") else {
                    return Err(ManifestError::Invalid(
                        "network capability needs an https origin, such as network:https://api.example.com"
                            .into(),
                    ));
                };
                if !valid_origin(origin) {
                    return Err(ManifestError::Invalid(format!(
                        "network capability needs an https origin: \"{capability}\""
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn requested_capabilities(&self) -> Vec<String> {
        let mut caps: BTreeSet<String> = self.capabilities.iter().cloned().collect();
        if let Some(integration) = &self.contributes.integration
            && let Some(origin) = &integration.api_origin
        {
            caps.insert(format!("network:{}", origin.trim_end_matches('/')));
        }
        if self.contributes.instructions.is_some() {
            caps.insert("instructions".into());
        }
        if !self.contributes.events.is_empty() {
            caps.insert("events".into());
        }
        caps.into_iter().collect()
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == name || capability.starts_with(&format!("{name}:")))
    }

    pub fn network_origins(&self) -> Vec<String> {
        let mut origins = BTreeSet::new();
        for capability in &self.capabilities {
            if let Some(origin) = capability.strip_prefix("network:") {
                origins.insert(origin.trim_end_matches('/').to_string());
            }
        }
        if let Some(integration) = &self.contributes.integration
            && let Some(origin) = &integration.api_origin
        {
            origins.insert(origin.trim_end_matches('/').to_string());
        }
        origins.into_iter().collect()
    }

    pub fn capability_summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for capability in self.requested_capabilities() {
            let (name, origin) = capability
                .split_once(':')
                .map(|(name, value)| (name, Some(value)))
                .unwrap_or((capability.as_str(), None));
            let line = match name {
                "storage" => "Store its own data".to_string(),
                "prompt" => "Send messages in your sessions".to_string(),
                "sessions" => "List, create and open sessions".to_string(),
                "files" => "Read and write files in the open project".to_string(),
                "network" => match origin {
                    Some(origin) => format!("Talk to {origin}"),
                    None => "Talk to external services".to_string(),
                },
                "model" => "Generate text with your model (not available yet)".to_string(),
                "clipboard" => "Copy text to the clipboard".to_string(),
                "notifications" => "Show notifications".to_string(),
                "exec" => "Run commands on this computer".to_string(),
                "instructions" => "Add instructions to what agents read".to_string(),
                "events" => "React when a turn finishes or asks for permission".to_string(),
                _ => capability.clone(),
            };
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        lines
    }

    pub fn review_lines(&self) -> Vec<String> {
        let mut lines = self.capability_summary();
        if let Some(instructions) = &self.contributes.instructions {
            let preview: String = instructions.chars().take(240).collect();
            lines.push(format!("Agents read: {preview}"));
        }
        for tool in &self.contributes.tools {
            lines.push(format!("Draws tool calls matching {}", tool.matcher));
        }
        for editor in &self.contributes.file_editors {
            lines.push(format!(
                "Opens files matching {} in its own viewer",
                editor.matcher
            ));
        }
        for command in &self.contributes.commands {
            lines.push(format!("Adds /{} to the composer", command.name));
        }
        for action in &self.contributes.actions {
            lines.push(format!(
                "Adds the {} action \"{}\"",
                action.target, action.label
            ));
        }
        for block in &self.contributes.blocks {
            lines.push(format!("Draws the {} chat block", block.name));
        }
        if let Some(mentions) = &self.contributes.mentions {
            lines.push(format!("Adds {} rows to the @ menu", mentions.label));
        }
        if !self.contributes.events.is_empty() {
            lines.push(format!("Hears {}", self.contributes.events.join(", ")));
        }
        lines
    }

    pub fn panel_title(&self) -> &str {
        self.contributes
            .panel
            .as_ref()
            .and_then(|panel| panel.title.as_deref())
            .unwrap_or(&self.name)
    }
}

impl Contributes {
    fn validate(&self) -> Result<(), ManifestError> {
        if self.actions.len() > ACTIONS_MAX {
            return Err(ManifestError::Invalid(format!(
                "at most {ACTIONS_MAX} actions are allowed"
            )));
        }
        for action in &self.actions {
            if !matches!(action.target.as_str(), "message" | "session") {
                return Err(ManifestError::Invalid(format!(
                    "action {} uses where \"{}\"; this build supports \"message\" and \"session\"",
                    action.id, action.target
                )));
            }
            if action.label.chars().count() > 40 {
                return Err(ManifestError::Invalid(format!(
                    "action {} label is longer than 40 characters",
                    action.id
                )));
            }
        }
        if self.commands.len() > COMMANDS_MAX {
            return Err(ManifestError::Invalid(format!(
                "at most {COMMANDS_MAX} commands are allowed"
            )));
        }
        for command in &self.commands {
            if !is_command_name(&command.name) {
                return Err(ManifestError::Invalid(format!(
                    "command \"{}\" must be lowercase letters, digits and dashes",
                    command.name
                )));
            }
        }
        if self.tools.len() > TOOLS_MAX {
            return Err(ManifestError::Invalid(format!(
                "at most {TOOLS_MAX} tools are allowed"
            )));
        }
        for tool in &self.tools {
            if tool.matcher.trim().is_empty() || tool.matcher.chars().count() > 128 {
                return Err(ManifestError::Invalid(
                    "tool match must be 1 to 128 characters".into(),
                ));
            }
            if let Some(output) = &tool.output
                && !matches!(
                    output.as_str(),
                    "auto" | "text" | "json" | "markdown" | "code" | "table"
                )
            {
                return Err(ManifestError::Invalid(format!(
                    "tool {} has unknown output \"{output}\"",
                    tool.matcher
                )));
            }
        }
        if self.blocks.len() > BLOCKS_MAX {
            return Err(ManifestError::Invalid(format!(
                "at most {BLOCKS_MAX} blocks are allowed"
            )));
        }
        for block in &self.blocks {
            if !is_block_name(&block.name) {
                return Err(ManifestError::Invalid(format!(
                    "block \"{}\" must be PascalCase",
                    block.name
                )));
            }
            if block.args.len() > 8 {
                return Err(ManifestError::Invalid(format!(
                    "block {} has more than 8 arguments",
                    block.name
                )));
            }
        }
        if self.file_editors.len() > FILE_EDITORS_MAX {
            return Err(ManifestError::Invalid(format!(
                "at most {FILE_EDITORS_MAX} file editors are allowed"
            )));
        }
        if let Some(mentions) = &self.mentions
            && (mentions.label.trim().is_empty()
                || mentions.label.chars().count() > MENTIONS_LABEL_MAX)
        {
            return Err(ManifestError::Invalid(
                "mentions.label must be 1 to 40 characters".into(),
            ));
        }
        if let Some(instructions) = &self.instructions
            && instructions.chars().count() > INSTRUCTIONS_MAX
        {
            return Err(ManifestError::Invalid(format!(
                "instructions are longer than {INSTRUCTIONS_MAX} characters"
            )));
        }
        if let Some(status) = &self.status_section {
            if status.interval < STATUS_INTERVAL_MIN {
                return Err(ManifestError::Invalid(format!(
                    "status interval must be at least {STATUS_INTERVAL_MIN} ms"
                )));
            }
            if status.method.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "status method cannot be empty".into(),
                ));
            }
        }
        Ok(())
    }
}

pub fn safe_relative(value: &str) -> Option<PathBuf> {
    if value.trim().is_empty() || value.contains('\0') || value.contains('\\') {
        return None;
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn is_kebab(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !value.ends_with('-')
        && !value.contains("--")
}

fn is_command_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && value.len() <= 24
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_block_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && value.chars().count() >= 2
        && chars.all(|ch| ch.is_ascii_alphanumeric())
}

fn valid_origin(value: &str) -> bool {
    let value = value.trim_end_matches('/');
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty() && !rest.contains(' ') && !rest.contains('/') && rest.contains('.')
}

pub fn patterns_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return path
            .rsplit('/')
            .next()
            .map(|name| name.ends_with(&format!(".{suffix}")))
            .unwrap_or(false);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    pattern.eq_ignore_ascii_case(&path)
}

pub fn root_widget(value: &serde_json::Value) -> Option<Vec<Widget>> {
    if value.is_array() {
        serde_json::from_value::<Vec<Widget>>(value.clone()).ok()
    } else {
        serde_json::from_value::<Widget>(value.clone())
            .ok()
            .map(|widget| vec![widget])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(value: serde_json::Value) -> Manifest {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn rejects_traversal_in_runtime_entry() {
        let value = serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "runtime": { "entry": "../evil.lua" }
        });
        let manifest = manifest(value);
        let root = tempfile::tempdir().unwrap();
        assert!(manifest.validate(root.path()).is_err());
    }

    #[test]
    fn infers_capabilities_from_contributions() {
        let value = serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "contributes": {
                "instructions": "Deploys run on Fly.",
                "integration": { "name": "Acme", "apiOrigin": "https://api.acme.dev" }
            }
        });
        let manifest = manifest(value);
        let caps = manifest.requested_capabilities();
        assert!(caps.contains(&"instructions".to_string()));
        assert!(caps.contains(&"network:https://api.acme.dev".to_string()));
        assert_eq!(
            manifest.network_origins(),
            vec!["https://api.acme.dev".to_string()]
        );
    }

    #[test]
    fn rejects_bare_network_capability() {
        let value = serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "capabilities": ["network"]
        });
        let manifest = manifest(value);
        let root = tempfile::tempdir().unwrap();
        assert!(manifest.validate(root.path()).is_err());
    }

    #[test]
    fn wildcard_matches_extension() {
        assert!(patterns_match("*.parquet", "a/b/file.parquet"));
        assert!(!patterns_match("*.parquet", "a/b/file.csv"));
        assert!(patterns_match("mcp.deploys.*", "mcp.deploys.status"));
    }
}
