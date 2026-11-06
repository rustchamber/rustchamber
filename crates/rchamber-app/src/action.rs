use rchamber_api::LaunchOptions;
use rchamber_api::PromptFile;
use rchamber_api::models::{
    FileDiff, FileEntry, McpServer, Model, ModelRef, PermissionRequest, Project, ServerInfo,
    Session, VcsInfo,
};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct TrackedProject {
    pub directory: String,
    pub since: f64,
}

#[derive(Clone, Debug)]
pub enum Action {
    SetTrackedProjects(Vec<TrackedProject>),
    SelectSession(String),
    CreateSession {
        directory: String,
        prompt: Option<String>,
        files: Vec<PromptFile>,
        model: Option<ModelRef>,
        agent: Option<String>,
        allow_all: bool,
    },
    Send {
        session_id: String,
        text: String,
        files: Vec<PromptFile>,
        delivery: String,
    },
    LoadInbox(String),
    CancelInbox {
        session_id: String,
        inbox_id: String,
    },
    SetInboxDelivery {
        session_id: String,
        inbox_id: String,
        delivery: String,
    },
    ReorderInbox {
        session_id: String,
        inbox_ids: Vec<String>,
    },
    Interrupt(String),
    RenameSession {
        id: String,
        title: String,
    },
    DeleteSession(String),
    ArchiveSession {
        id: String,
        archived: bool,
    },
    SetSessionPinned {
        id: String,
        pinned: bool,
    },
    SetSessionWork {
        id: String,
        work: bool,
    },
    SetSessionPermissions {
        id: String,
        allow_all: bool,
    },
    MarkSessionViewed(String),
    MarkAllSessionsViewed(Vec<String>),
    MoveSession {
        id: String,
        directory: String,
    },
    ArchiveSessions {
        ids: Vec<String>,
        archived: bool,
    },
    DeleteSessions(Vec<String>),
    SwitchModel {
        session_id: String,
        model: ModelRef,
    },
    SwitchAgent {
        session_id: String,
        agent: String,
    },
    ReplyPermission {
        session_id: String,
        request_id: String,
        decision: String,
    },
    SetWorkspace(String),
    LoadOlder(String),
    LoadSessionPreview(String),
    LoadDiff(String),
    LoadDirectory(String),
    LoadFile(String),
    SearchFiles(String),
    Configure {
        endpoint: Option<String>,
        password: Option<String>,
        workspace: Option<String>,
    },
    Reconnect,
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum BackendEvent {
    Connection(Connection),
    Server(Box<ServerInfo>),
    Sessions(Vec<Session>),
    SessionUpsert(Box<Session>),
    SessionActivated(Box<Session>),
    SessionRemoved(String),
    Messages {
        session_id: String,
        messages: Vec<rchamber_api::models::Message>,
    },
    OlderMessages {
        session_id: String,
        messages: Vec<rchamber_api::models::Message>,
        next: Option<String>,
    },
    Live(LiveEvent),
    Models(Vec<Model>),
    Providers(Vec<rchamber_api::models::Provider>),
    Agents(Vec<rchamber_api::models::Agent>),
    Commands(Vec<rchamber_api::models::Command>),
    Projects(Vec<Project>),
    SessionMetadataUpdated(Box<Session>),
    SessionViewed {
        id: String,
        idle: f64,
    },
    SessionPreview {
        id: String,
        text: String,
    },
    Permissions {
        session_id: String,
        requests: Vec<PermissionRequest>,
    },
    Diff {
        session_id: String,
        files: Vec<FileDiff>,
    },
    Vcs(Option<VcsInfo>),
    Files {
        path: String,
        entries: Vec<FileEntry>,
    },
    FileContents {
        path: String,
        bytes: Vec<u8>,
    },
    FileSearch {
        query: String,
        entries: Vec<FileEntry>,
    },
    Skills(Vec<rchamber_api::models::Skill>),
    Mcp(Vec<McpServer>),
    Snippets(Vec<crate::snippets::Snippet>),
    Toast {
        kind: ToastKind,
        message: String,
    },
    PromptQueued {
        session_id: String,
    },
    Inbox {
        session_id: String,
        items: Vec<serde_json::Value>,
    },
    WorkspaceChanged(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Ready,
    Reconnecting(String),
    Failed(String),
    Disconnected,
}

#[derive(Clone, Debug)]
pub enum LiveEvent {
    TransportConnected,
    TransportDisconnected(String),
    SessionsDirty,
    SessionRenamed {
        session_id: String,
        title: String,
    },
    SessionDeleted {
        session_id: String,
    },
    SessionViewed {
        session_id: String,
        idle: f64,
    },
    SessionPermissions {
        session_id: String,
        permissions: Value,
    },
    SessionModel {
        session_id: String,
        model: ModelRef,
    },
    SessionAgent {
        session_id: String,
        agent: String,
    },
    Execution {
        session_id: String,
        state: ExecutionState,
        error: Option<rchamber_api::models::StructuredError>,
    },
    StepStarted {
        session_id: String,
        message_id: String,
        agent: Option<String>,
        model: Option<ModelRef>,
    },
    StepEnded {
        session_id: String,
        message_id: String,
        cost: f64,
        tokens: Option<rchamber_api::models::TokenUsage>,
        finish: Option<String>,
    },
    StepFailed {
        session_id: String,
        message_id: String,
        error: Option<rchamber_api::models::StructuredError>,
    },
    TextDelta {
        session_id: String,
        message_id: String,
        ordinal: usize,
        delta: String,
    },
    TextEnded {
        session_id: String,
        message_id: String,
        ordinal: usize,
        text: String,
    },
    ReasoningDelta {
        session_id: String,
        message_id: String,
        ordinal: usize,
        delta: String,
    },
    ReasoningEnded {
        session_id: String,
        message_id: String,
        ordinal: usize,
        text: String,
    },
    ToolInputStarted {
        session_id: String,
        message_id: String,
        tool_id: String,
        name: String,
    },
    ToolInputDelta {
        session_id: String,
        message_id: String,
        tool_id: String,
        delta: String,
    },
    ToolInputEnded {
        session_id: String,
        message_id: String,
        tool_id: String,
        text: String,
    },
    ToolCalled {
        session_id: String,
        message_id: String,
        tool_id: String,
        name: String,
        input: serde_json::Value,
        executed: bool,
    },
    ToolProgress {
        session_id: String,
        message_id: String,
        tool_id: String,
        metadata: serde_json::Value,
    },
    ToolSuccess {
        session_id: String,
        message_id: String,
        tool_id: String,
        content: Vec<rchamber_api::models::ToolContent>,
        executed: bool,
    },
    ToolFailed {
        session_id: String,
        message_id: String,
        tool_id: String,
        error: Option<rchamber_api::models::StructuredError>,
        content: Vec<rchamber_api::models::ToolContent>,
    },
    PermissionAsked(PermissionRequest),
    PermissionReplied {
        session_id: String,
        request_id: String,
    },
    MessagesDirty {
        session_id: String,
    },
    InboxDelivered {
        session_id: String,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionState {
    Started,
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Default)]
pub struct LaunchConfig {
    pub workspace: Option<String>,
    pub endpoint: Option<String>,
    pub password: Option<String>,
}

impl LaunchConfig {
    pub fn into_options(&self) -> LaunchOptions {
        LaunchOptions {
            directory: self.workspace.as_ref().map(std::path::PathBuf::from),
            binary: None,
            port: None,
            password: self.password.clone(),
            external_url: self.endpoint.clone(),
        }
    }
}
