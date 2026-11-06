use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Timestamp = f64;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub reasoning: f64,
    #[serde(default)]
    pub cache: TokenCache,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCache {
    #[serde(default)]
    pub read: f64,
    #[serde(default)]
    pub write: f64,
}

impl TokenUsage {
    pub fn total(&self) -> f64 {
        self.input + self.output + self.reasoning
    }

    pub fn context_used(&self) -> f64 {
        self.input + self.cache.read + self.cache.write
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionTime {
    #[serde(default)]
    pub created: Timestamp,
    #[serde(default)]
    pub updated: Timestamp,
    pub busy: Option<Timestamp>,
    pub idle: Option<Timestamp>,
    pub viewed: Option<Timestamp>,
    pub archived: Option<Timestamp>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationRef {
    pub directory: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionCursor {
    pub previous: Option<String>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionPage {
    #[serde(default)]
    pub data: Vec<Session>,
    #[serde(default)]
    pub cursor: SessionCursor,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    #[serde(default, rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    pub slug: Option<String>,
    pub agent: Option<String>,
    pub model: Option<ModelRef>,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: TokenUsage,
    pub outcome: Option<Outcome>,
    pub time: SessionTime,
    pub title: Option<String>,
    pub subpath: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default)]
    pub permissions: Option<Value>,
    pub revert: Option<Value>,
    pub location: Option<LocationRef>,
    pub fork: Option<Value>,
}

impl Session {
    pub fn title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("Untitled session")
    }

    pub fn directory(&self) -> Option<&str> {
        self.location.as_ref().map(|l| l.directory.as_str())
    }

    pub fn allows_all(&self) -> bool {
        self.permissions
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|rules| {
                rules.iter().any(|rule| {
                    rule.get("action").and_then(Value::as_str) == Some("*")
                        && rule.get("resource").and_then(Value::as_str) == Some("*")
                        && rule.get("effect").and_then(Value::as_str) == Some("allow")
                })
            })
    }

    pub fn set_allows_all(&mut self, allow_all: bool) {
        self.permissions = allow_all.then(|| {
            serde_json::json!([
                { "action": "*", "resource": "*", "effect": "allow" }
            ])
        });
    }

    pub fn is_busy(&self) -> bool {
        match (self.time.busy, self.time.idle) {
            (Some(busy), Some(idle)) => busy > idle,
            (Some(_), None) => true,
            _ => self.time.idle.is_none_or(|idle| idle < self.time.updated),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl ModelRef {
    pub fn key(&self) -> String {
        format!("{}/{}", self.provider_id, self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    #[serde(default, rename = "modelID")]
    pub model_id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(default)]
    pub name: String,
    pub family: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub variants: Vec<ModelVariant>,
    #[serde(default)]
    pub cost: Vec<ModelCost>,
    pub status: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub limit: Option<ModelLimit>,
    pub time: Option<ModelTime>,
    #[serde(default)]
    pub capabilities: Option<Value>,
}

impl Model {
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.model_id
        } else {
            &self.name
        }
    }

    pub fn context_limit(&self) -> Option<u64> {
        self.limit.as_ref().map(|l| l.context)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariant {
    pub id: String,
    #[serde(default)]
    pub settings: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    pub cache: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLimit {
    #[serde(default)]
    pub context: u64,
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTime {
    #[serde(default)]
    pub released: Timestamp,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub activation: Option<String>,
    pub package: Option<String>,
    #[serde(rename = "integrationID")]
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub hidden: bool,
    pub color: Option<Value>,
    pub model: Option<ModelRef>,
    pub steps: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub name: String,
    pub description: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub template: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub content: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: McpServerStatus,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    #[serde(default)]
    pub status: String,
}

impl McpServer {
    pub fn is_connected(&self) -> bool {
        self.status.status == "connected"
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub canonical: String,
    pub name: Option<String>,
    pub vcs: Option<String>,
    pub icon: Option<Value>,
    pub time: ProjectTime,
    #[serde(default)]
    pub sandboxes: Vec<String>,
}

impl Project {
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            std::path::Path::new(&self.canonical)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.canonical.clone())
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProjectTime {
    #[serde(default)]
    pub created: Timestamp,
    #[serde(default)]
    pub updated: Timestamp,
    #[serde(default)]
    pub active: Timestamp,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub version: String,
    #[serde(default)]
    pub pid: u64,
    pub hostname: Option<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    pub paths: ServerPaths,
    pub capabilities: Option<ServerCapabilities>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ServerPaths {
    #[serde(default)]
    pub tmp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub persistent_pty: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub action: String,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub save: Vec<String>,
    pub metadata: Option<Value>,
    pub source: Option<Value>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VcsInfo {
    pub provider: Option<String>,
    pub branch: Option<VcsBranch>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VcsBranch {
    pub current: Option<String>,
    pub default: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsFileStatus {
    pub file: String,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VcsStatus {
    #[serde(default)]
    pub data: Vec<VcsFileStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub file: String,
    #[serde(default)]
    pub patch: String,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    pub status: Option<String>,
    pub more: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageList {
    #[serde(default)]
    pub data: Vec<Message>,
    pub cursor: Option<MessageCursor>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MessageCursor {
    pub previous: Option<String>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Message {
    User(Box<UserMessage>),
    Assistant(Box<AssistantMessage>),
    Synthetic(SyntheticMessage),
    System(SystemMessage),
    Shell(ShellMessage),
    Skill(SkillMessage),
    Compaction(CompactionMessage),
    Idle(IdleMessage),
    AgentSwitched(AgentSwitchedMessage),
    ModelSwitched(ModelSwitchedMessage),
    LocationSwitched(LocationSwitchedMessage),
    #[serde(other)]
    Unknown,
}

impl Message {
    pub fn id(&self) -> Option<&str> {
        match self {
            Message::User(m) => Some(&m.id),
            Message::Assistant(m) => Some(&m.id),
            Message::Synthetic(m) => Some(&m.id),
            Message::System(m) => Some(&m.id),
            Message::Shell(m) => Some(&m.id),
            Message::Skill(m) => Some(&m.id),
            Message::Compaction(m) => Some(&m.id),
            Message::Idle(m) => Some(&m.id),
            Message::AgentSwitched(m) => Some(&m.id),
            Message::ModelSwitched(m) => Some(&m.id),
            Message::LocationSwitched(m) => Some(&m.id),
            Message::Unknown => None,
        }
    }

    pub fn created(&self) -> Option<Timestamp> {
        match self {
            Message::User(m) => Some(m.time.created),
            Message::Assistant(m) => Some(m.time.created),
            Message::Synthetic(m) => Some(m.time.created),
            Message::System(m) => Some(m.time.created),
            Message::Shell(m) => Some(m.time.created),
            Message::Skill(m) => Some(m.time.created),
            Message::Compaction(m) => Some(m.time.created),
            Message::Idle(m) => Some(m.time.created),
            Message::AgentSwitched(m) => Some(m.time.created),
            Message::ModelSwitched(m) => Some(m.time.created),
            Message::LocationSwitched(m) => Some(m.time.created),
            Message::Unknown => None,
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Message::User(_))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MessageTime {
    #[serde(default)]
    pub created: Timestamp,
    pub streamed: Option<Timestamp>,
    pub completed: Option<Timestamp>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub files: Vec<Attachment>,
    #[serde(default)]
    pub agents: Vec<Value>,
    #[serde(default)]
    pub skills: Vec<Value>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub mime: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub agent: Option<String>,
    pub model: Option<ModelRef>,
    #[serde(default)]
    pub content: Vec<Part>,
    pub snapshot: Option<Value>,
    pub finish: Option<String>,
    #[serde(rename = "rawFinish")]
    pub raw_finish: Option<String>,
    #[serde(default)]
    pub cost: f64,
    pub tokens: Option<TokenUsage>,
    pub error: Option<StructuredError>,
    pub retry: Option<RetryInfo>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryInfo {
    pub attempt: u32,
    pub at: Timestamp,
    pub error: Option<StructuredError>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredError {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub message: String,
    pub status: Option<u16>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntheticMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    #[serde(default)]
    pub text: String,
    pub description: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SystemMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    #[serde(default)]
    pub text: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    #[serde(rename = "shellID")]
    pub shell_id: Option<String>,
    #[serde(default)]
    pub command: String,
    pub status: Option<String>,
    pub exit: Option<Value>,
    pub output: Option<ShellOutput>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutput {
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub cursor: u64,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub skill: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub summary: Option<String>,
    pub text: Option<String>,
    pub error: Option<StructuredError>,
    pub model: Option<ModelRef>,
    #[serde(default)]
    pub cost: f64,
    pub tokens: Option<TokenUsage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub outcome: Option<Outcome>,
    pub error: Option<StructuredError>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentSwitchedMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub agent: String,
    pub previous: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelSwitchedMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub model: ModelRef,
    pub previous: Option<ModelRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocationSwitchedMessage {
    pub id: String,
    #[serde(default)]
    pub time: MessageTime,
    pub location: Option<LocationRef>,
    pub previous: Option<LocationRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(Box<ToolPart>),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextPart {
    #[serde(default)]
    pub text: String,
    pub native: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReasoningPart {
    #[serde(default)]
    pub text: String,
    pub time: Option<PartTime>,
    pub native: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolPart {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub executed: bool,
    pub state: ToolState,
    #[serde(default)]
    pub time: PartTime,
    pub native: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PartTime {
    #[serde(default)]
    pub created: Timestamp,
    pub ran: Option<Timestamp>,
    pub completed: Option<Timestamp>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolState {
    Streaming(StreamingTool),
    Running(RunningTool),
    Completed(CompletedTool),
    Error(ErrorTool),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct StreamingTool {
    #[serde(default)]
    pub input: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunningTool {
    #[serde(default)]
    pub input: Value,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CompletedTool {
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub content: Vec<ToolContent>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ErrorTool {
    #[serde(default)]
    pub input: Value,
    pub error: Option<StructuredError>,
    #[serde(default)]
    pub content: Vec<ToolContent>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    Text {
        #[serde(default)]
        text: String,
    },
    File {
        #[serde(default)]
        uri: String,
        #[serde(default)]
        name: Option<String>,
        mime: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl ToolContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ToolContent::Text { text } => Some(text),
            _ => None,
        }
    }
}

impl ToolState {
    pub fn input(&self) -> Option<&Value> {
        match self {
            ToolState::Running(t) => Some(&t.input),
            ToolState::Completed(t) => Some(&t.input),
            ToolState::Error(t) => Some(&t.input),
            _ => None,
        }
    }

    pub fn output_text(&self) -> String {
        let content = match self {
            ToolState::Completed(t) => &t.content,
            ToolState::Error(t) => &t.content,
            _ => return String::new(),
        };
        content
            .iter()
            .filter_map(|c| match c {
                ToolContent::Text { text } => Some(text.as_str()),
                ToolContent::File { uri, .. } => Some(uri.as_str()),
                ToolContent::Unknown => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_running(&self) -> bool {
        matches!(self, ToolState::Streaming(_) | ToolState::Running(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, ToolState::Error(_))
    }
}

pub fn text_from_parts(parts: &[Part]) -> String {
    let mut out = String::new();
    for part in parts {
        if let Part::Text(t) = part {
            out.push_str(&t.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_decodes_connected_status() {
        let server: McpServer =
            serde_json::from_str(r#"{"name":"rea","status":{"status":"connected"}}"#).unwrap();
        assert_eq!(server.name, "rea");
        assert!(server.is_connected());
    }

    #[test]
    fn mcp_server_treats_other_statuses_as_disconnected() {
        let server: McpServer =
            serde_json::from_str(r#"{"name":"rea","status":{"status":"failed"}}"#).unwrap();
        assert!(!server.is_connected());
    }

    #[test]
    fn mcp_server_defaults_when_status_is_missing() {
        let server: McpServer = serde_json::from_str(r#"{"name":"rea"}"#).unwrap();
        assert!(!server.is_connected());
        assert!(server.status.status.is_empty());
    }

    #[test]
    fn model_variant_default_omits_the_field() {
        let reference = ModelRef {
            id: "deepseek-v4.1-flash".into(),
            provider_id: "opencode-go".into(),
            variant: None,
        };
        let value = serde_json::to_value(&reference).unwrap();
        assert!(value.get("variant").is_none());
        assert_eq!(value["id"], "deepseek-v4.1-flash");
    }

    #[test]
    fn session_permission_mode_reads_and_writes_allow_all() {
        let mut session: Session =
            serde_json::from_str(r#"{"id":"ses_1","projectID":"p1","time":{}}"#).unwrap();
        assert!(!session.allows_all());
        session.set_allows_all(true);
        assert!(session.allows_all());
        session.set_allows_all(false);
        assert!(!session.allows_all());
    }
}
