use std::collections::{HashMap, HashSet};

use rchamber_api::models::ServerInfo;
use rchamber_api::models::{
    Agent, AssistantMessage, Command, FileDiff, McpServer, Message, MessageTime, Model, ModelRef,
    Part, PermissionRequest, Project, Provider, ReasoningPart, Session, StructuredError, TextPart,
    ToolPart, ToolState, VcsInfo,
};
use rchamber_ext::Widget;

use crate::action::{
    BackendEvent, Connection, ExecutionState, LiveEvent, ToastKind, TrackedProject,
};
use crate::attachments::AttachmentItem;
use crate::snippets::Snippet;
use crate::workspace::{ProjectEntry, WorkspaceStore, paths_match};

pub const TOAST_LIFETIME_MS: f64 = 4_000.0;
pub const TOAST_EXIT_MS: f64 = 110.0;

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
    pub created: f64,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub session: Session,
    pub messages: Vec<Message>,
    pub loading: bool,
    pub busy: bool,
    pub awaiting_permission: bool,
    pub draft: String,
    pub pending: Vec<PermissionRequest>,
    pub diff: Vec<FileDiff>,
    pub inbox: Vec<serde_json::Value>,
    pub error: Option<String>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub scroll_to_bottom: bool,
    pub model: Option<ModelRef>,
    pub agent: Option<String>,
    pub turn_started: Option<f64>,
}

impl Chat {
    pub fn new(session: Session) -> Self {
        let model = session.model.clone();
        let agent = session.agent.clone();
        Self {
            session,
            messages: Vec::new(),
            loading: true,
            busy: false,
            awaiting_permission: false,
            draft: String::new(),
            pending: Vec::new(),
            diff: Vec::new(),
            inbox: Vec::new(),
            error: None,
            has_more: false,
            next_cursor: None,
            scroll_to_bottom: true,
            model,
            agent,
            turn_started: None,
        }
    }

    pub fn title(&self) -> &str {
        self.session.title()
    }

    fn assistant_mut(&mut self, id: &str) -> &mut AssistantMessage {
        let position = self.messages.iter().position(|m| m.id() == Some(id));
        let index = match position {
            Some(index) if matches!(self.messages[index], Message::Assistant(_)) => index,
            Some(index) => {
                self.messages[index] = Message::Assistant(Box::new(assistant_message(id)));
                index
            }
            None => {
                self.messages
                    .push(Message::Assistant(Box::new(assistant_message(id))));
                self.messages.len() - 1
            }
        };
        match &mut self.messages[index] {
            Message::Assistant(message) => message,
            _ => unreachable!(),
        }
    }

    fn tool_mut(&mut self, message_id: &str, tool_id: &str) -> &mut ToolPart {
        let assistant = self.assistant_mut(message_id);
        let index = assistant
            .content
            .iter()
            .position(|part| matches!(part, Part::Tool(tool) if tool.id == tool_id));
        let index = match index {
            Some(index) => index,
            None => {
                assistant.content.push(Part::Tool(Box::new(ToolPart {
                    id: tool_id.to_string(),
                    name: String::new(),
                    executed: false,
                    state: ToolState::Streaming(Default::default()),
                    time: Default::default(),
                    native: None,
                })));
                assistant.content.len() - 1
            }
        };
        match &mut assistant.content[index] {
            Part::Tool(tool) => tool,
            _ => unreachable!(),
        }
    }

    fn set_text(
        &mut self,
        message_id: &str,
        ordinal: usize,
        text: Option<&str>,
        delta: Option<&str>,
    ) {
        let assistant = self.assistant_mut(message_id);
        let mut seen = 0usize;
        let mut target = None;
        for (index, part) in assistant.content.iter().enumerate() {
            if matches!(part, Part::Text(_)) {
                if seen == ordinal {
                    target = Some(index);
                    break;
                }
                seen += 1;
            }
        }
        let index = match target {
            Some(index) => index,
            None => {
                assistant.content.push(Part::Text(TextPart {
                    text: String::new(),
                    native: None,
                }));
                assistant.content.len() - 1
            }
        };
        if let Part::Text(part) = &mut assistant.content[index] {
            match (text, delta) {
                (Some(full), _) => part.text = full.to_string(),
                (None, Some(fragment)) => part.text.push_str(fragment),
                _ => {}
            }
        }
    }

    fn set_reasoning(
        &mut self,
        message_id: &str,
        ordinal: usize,
        text: Option<&str>,
        delta: Option<&str>,
    ) {
        let assistant = self.assistant_mut(message_id);
        let mut seen = 0usize;
        let mut target = None;
        for (index, part) in assistant.content.iter().enumerate() {
            if matches!(part, Part::Reasoning(_)) {
                if seen == ordinal {
                    target = Some(index);
                    break;
                }
                seen += 1;
            }
        }
        let index = match target {
            Some(index) => index,
            None => {
                assistant.content.push(Part::Reasoning(ReasoningPart {
                    text: String::new(),
                    time: None,
                    native: None,
                }));
                assistant.content.len() - 1
            }
        };
        if let Part::Reasoning(part) = &mut assistant.content[index] {
            match (text, delta) {
                (Some(full), _) => part.text = full.to_string(),
                (None, Some(fragment)) => part.text.push_str(fragment),
                _ => {}
            }
        }
    }
}

fn assistant_message(id: &str) -> AssistantMessage {
    AssistantMessage {
        id: id.to_string(),
        time: MessageTime {
            created: now_ms(),
            streamed: None,
            completed: None,
        },
        agent: None,
        model: None,
        content: Vec::new(),
        snapshot: None,
        finish: None,
        raw_finish: None,
        cost: 0.0,
        tokens: None,
        error: None,
        retry: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    Idle,
    Connecting,
    Ready,
    Failed,
}

#[derive(Default)]
pub struct FilesState {
    pub tree: HashMap<String, Vec<rchamber_api::models::FileEntry>>,
    pub expanded: std::collections::HashSet<String>,
    pub preview: Option<(String, String)>,
    pub search: Vec<rchamber_api::models::FileEntry>,
    pub search_query: String,
    pub open_tabs: Vec<String>,
    pub active_tab: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExtensionFailure {
    pub method: String,
    pub message: String,
}

#[derive(Default)]
pub struct ExtensionUiState {
    pub installed: Vec<rchamber_ext::InstalledExtension>,
    pub status: HashMap<String, Vec<Widget>>,
    pub panels: HashMap<String, Vec<Widget>>,
    pub blocks: HashMap<String, Vec<Widget>>,
    pub errors: HashMap<String, ExtensionFailure>,
    pub badges: HashMap<String, i64>,
    pub mentions: Vec<crate::extensions::MentionRow>,
}

fn clear_failure(errors: &mut HashMap<String, ExtensionFailure>, extension_id: &str, method: &str) {
    if errors
        .get(extension_id)
        .is_some_and(|failure| failure.method == method)
    {
        errors.remove(extension_id);
    }
}

pub struct AppModel {
    pub connection: Connection,
    pub connection_detail: Option<String>,
    pub server: Option<ServerInfo>,
    pub sessions: Vec<Session>,
    pub session_previews: HashMap<String, String>,
    pub preview_pending: HashSet<String>,
    pub projects: Vec<Project>,
    pub workspace_store: WorkspaceStore,
    pub models: Vec<Model>,
    pub providers: Vec<Provider>,
    pub agents: Vec<Agent>,
    pub commands: Vec<Command>,
    pub skills: Vec<rchamber_api::models::Skill>,
    pub mcp: Vec<McpServer>,
    pub snippets: Vec<Snippet>,
    pub files: FilesState,
    pub attachments: HashMap<String, Vec<AttachmentItem>>,
    pub vcs: Option<VcsInfo>,
    pub workspace: Option<String>,
    pub active: Option<Chat>,
    pub drafts: HashMap<String, String>,
    pub drafts_changed_at: f64,
    pub search: String,
    pub toasts: Vec<Toast>,
    pub toasts_paused: bool,
    pub startup: Startup,
    toast_seq: u64,
    pub composer_focus: bool,
    pub show_settings: bool,
    pub show_sidebar: bool,
    pub palette_open: bool,
    pub extensions: ExtensionUiState,
    pub pending_open_session: Option<String>,
    pub pending_open_url: Option<String>,
    pub pending_clipboard: Option<String>,
    pub pending_compose: Option<String>,
    pub pending_extension_panel: Option<String>,
    pub pending_extension_approval: Option<String>,
}

impl Default for AppModel {
    fn default() -> Self {
        Self {
            connection: Connection::Disconnected,
            connection_detail: None,
            server: None,
            sessions: Vec::new(),
            session_previews: HashMap::new(),
            preview_pending: HashSet::new(),
            projects: Vec::new(),
            workspace_store: WorkspaceStore::load(),
            models: Vec::new(),
            providers: Vec::new(),
            agents: Vec::new(),
            commands: Vec::new(),
            skills: Vec::new(),
            mcp: Vec::new(),
            snippets: Vec::new(),
            files: FilesState::default(),
            attachments: HashMap::new(),
            vcs: None,
            workspace: None,
            active: None,
            drafts: HashMap::new(),
            drafts_changed_at: 0.0,
            search: String::new(),
            toasts: Vec::new(),
            toasts_paused: false,
            startup: Startup::Idle,
            toast_seq: 0,
            composer_focus: true,
            show_settings: false,
            show_sidebar: true,
            palette_open: false,
            extensions: ExtensionUiState::default(),
            pending_open_session: None,
            pending_open_url: None,
            pending_clipboard: None,
            pending_compose: None,
            pending_extension_panel: None,
            pending_extension_approval: None,
        }
    }
}

impl AppModel {
    pub fn apply(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::Connection(connection) => {
                self.connection = connection.clone();
                self.startup = match connection {
                    Connection::Connecting => Startup::Connecting,
                    Connection::Ready => Startup::Ready,
                    Connection::Failed(_) | Connection::Disconnected => Startup::Failed,
                    Connection::Reconnecting(_) => self.startup,
                };
            }
            BackendEvent::Server(info) => {
                self.connection_detail =
                    Some(format!("OpenCode {} · pid {}", info.version, info.pid));
                self.server = Some(*info);
            }
            BackendEvent::Sessions(sessions) => {
                self.sessions = sessions;
                if let Some(chat) = &mut self.active
                    && let Some(session) = self.sessions.iter().find(|s| s.id == chat.session.id)
                {
                    chat.session = session.clone();
                }
            }
            BackendEvent::SessionUpsert(session) => {
                let session = *session;
                if let Some(chat) = &mut self.active
                    && chat.session.id == session.id
                {
                    chat.session = session.clone();
                }
                upsert(&mut self.sessions, session);
            }
            BackendEvent::SessionActivated(session) => {
                let session = *session;
                let id = session.id.clone();
                let draft = self.drafts.get(&id).cloned().unwrap_or_default();
                let mut chat = Chat::new(session.clone());
                chat.draft = draft;
                chat.model = session.model.clone();
                chat.agent = session.agent.clone();
                if let Some(project) = self.workspace_store.project_for_session(&session) {
                    let project_id = project.id.clone();
                    self.workspace_store.mark_active_project(&project_id);
                }
                self.workspace_store.save();
                self.active = Some(chat);
                upsert(&mut self.sessions, session);
                self.composer_focus = true;
            }
            BackendEvent::SessionRemoved(id) => {
                self.remove_session_local(&id);
            }
            BackendEvent::Messages {
                session_id,
                messages,
            } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    let busy = infer_busy(&messages);
                    chat.messages = merge_messages(&chat.messages, messages, chat.busy || busy);
                    chat.busy = busy;
                    if busy && chat.turn_started.is_none() {
                        chat.turn_started = Some(now_ms());
                    }
                    chat.loading = false;
                    chat.error = None;
                    chat.scroll_to_bottom = true;
                }
            }
            BackendEvent::OlderMessages {
                session_id,
                messages,
                next,
            } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    let mut merged = messages;
                    merged.extend(std::mem::take(&mut chat.messages));
                    chat.messages = merged;
                    chat.next_cursor = next;
                    chat.has_more = chat.next_cursor.is_some();
                }
            }
            BackendEvent::Live(event) => self.apply_live(event),
            BackendEvent::Models(models) => self.models = models,
            BackendEvent::Providers(providers) => self.providers = providers,
            BackendEvent::Agents(agents) => self.agents = agents,
            BackendEvent::Commands(commands) => self.commands = commands,
            BackendEvent::Projects(projects) => {
                self.projects = projects;
            }
            BackendEvent::SessionMetadataUpdated(session) => {
                let session = *session;
                if let Some(chat) = &mut self.active
                    && chat.session.id == session.id
                {
                    chat.session = session.clone();
                }
                upsert(&mut self.sessions, session);
            }
            BackendEvent::SessionViewed { id, idle } => {
                self.mark_viewed_local(&id, idle);
                if let Some(chat) = &mut self.active
                    && chat.session.id == id
                {
                    chat.session.time.viewed = Some(
                        chat.session
                            .time
                            .viewed
                            .map_or(idle, |viewed| viewed.max(idle)),
                    );
                }
            }
            BackendEvent::SessionPreview { id, text } => {
                self.preview_pending.remove(&id);
                self.session_previews.insert(id, text);
            }
            BackendEvent::Permissions {
                session_id,
                requests,
            } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.awaiting_permission = !requests.is_empty();
                    chat.pending = requests;
                }
            }
            BackendEvent::Diff { session_id, files } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.diff = files;
                }
            }
            BackendEvent::Vcs(vcs) => self.vcs = vcs,
            BackendEvent::Files { path, entries } => {
                self.files.tree.insert(path, entries);
            }
            BackendEvent::FileContents { path, bytes } => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                if !self.files.open_tabs.iter().any(|tab| tab == &path) {
                    self.files.open_tabs.push(path.clone());
                }
                self.files.active_tab = Some(path.clone());
                self.files.preview = Some((path, text));
            }
            BackendEvent::FileSearch { entries, .. } => {
                self.files.search = entries;
            }
            BackendEvent::Skills(skills) => self.skills = skills,
            BackendEvent::Mcp(servers) => self.mcp = servers,
            BackendEvent::Snippets(snippets) => self.snippets = snippets,
            BackendEvent::Inbox { session_id, items } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.inbox = items;
                }
            }
            BackendEvent::Toast { kind, message } => self.toast(kind, message),
            BackendEvent::WorkspaceChanged(directory) => {
                self.save_draft();
                self.workspace = Some(directory.clone());
                self.workspace_store.active_project_id = self
                    .workspace_store
                    .project_for_directory(&directory)
                    .map(|project| project.id.clone());
                self.workspace_store.save();
                self.files = FilesState::default();
            }
            BackendEvent::PromptQueued { session_id } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.busy = true;
                    if chat.turn_started.is_none() {
                        chat.turn_started = Some(now_ms());
                    }
                }
            }
            BackendEvent::Extensions(extensions) => {
                self.extensions.installed = extensions;
                let active: HashSet<String> = self
                    .extensions
                    .installed
                    .iter()
                    .filter(|extension| extension.is_active())
                    .map(|extension| extension.manifest.id.clone())
                    .collect();
                self.extensions.status.retain(|id, _| active.contains(id));
                self.extensions.panels.retain(|id, _| active.contains(id));
                self.extensions.badges.retain(|id, _| active.contains(id));
                self.extensions.errors.retain(|id, _| active.contains(id));
                self.extensions
                    .mentions
                    .retain(|row| active.contains(&row.extension_id));
            }
            BackendEvent::ExtensionInstalled { extension_id } => {
                let has_panel = self.extensions.installed.iter().any(|extension| {
                    extension.manifest.id == extension_id
                        && extension.manifest.contributes.panel.is_some()
                });
                if has_panel {
                    self.pending_extension_approval = Some(extension_id);
                }
            }
            BackendEvent::ExtensionResult {
                call_id,
                extension_id,
                method,
                value,
            } => {
                let widgets = rchamber_ext::manifest::root_widget(&value);
                if method == "status" || call_id == "status" {
                    if let Some(widgets) = widgets {
                        self.extensions.status.insert(extension_id.clone(), widgets);
                        clear_failure(&mut self.extensions.errors, &extension_id, "status");
                    } else {
                        self.extensions.errors.insert(
                            extension_id,
                            ExtensionFailure {
                                method: "status".into(),
                                message: "The extension did not return a valid status tree.".into(),
                            },
                        );
                    }
                } else if method == "panel" || call_id == "panel" {
                    if let Some(widgets) = widgets {
                        self.extensions.panels.insert(extension_id.clone(), widgets);
                        clear_failure(&mut self.extensions.errors, &extension_id, "panel");
                    } else {
                        self.extensions.errors.insert(
                            extension_id,
                            ExtensionFailure {
                                method: "panel".into(),
                                message: "The extension did not return a valid panel tree.".into(),
                            },
                        );
                    }
                } else if call_id == "panel-action" {
                    if let Some(widgets) = widgets {
                        self.extensions.panels.insert(extension_id.clone(), widgets);
                        clear_failure(&mut self.extensions.errors, &extension_id, "panel");
                    }
                } else if let Some(widgets) = widgets
                    && !widgets.is_empty()
                {
                    self.extensions.panels.insert(extension_id.clone(), widgets);
                    self.pending_extension_panel = Some(extension_id);
                }
            }
            BackendEvent::ExtensionBlock { key, widgets } => {
                self.extensions.blocks.insert(key, widgets);
            }
            BackendEvent::ExtensionMentions { extension_id, rows } => {
                self.extensions
                    .mentions
                    .retain(|row| row.extension_id != extension_id);
                self.extensions.mentions.extend(rows);
            }
            BackendEvent::ExtensionError {
                extension_id,
                method,
                message,
                ..
            } => {
                self.extensions.errors.insert(
                    extension_id.clone(),
                    ExtensionFailure {
                        method: method.clone(),
                        message: message.clone(),
                    },
                );
                self.toast(
                    ToastKind::Error,
                    format!("Extension {extension_id} failed in {method}: {message}"),
                );
            }
            BackendEvent::ExtensionCompose { text, .. } => {
                self.pending_compose = Some(text);
            }
            BackendEvent::ExtensionAttach { title, url, .. } => {
                self.pending_compose = Some(match url {
                    Some(url) => format!("[{title}]({url})"),
                    None => title,
                });
            }
            BackendEvent::ExtensionBadge { id, count } => match count {
                Some(count) => {
                    self.extensions.badges.insert(id, count);
                }
                None => {
                    self.extensions.badges.remove(&id);
                }
            },
            BackendEvent::ExtensionOpenSession(id) => {
                self.pending_open_session = Some(id);
            }
            BackendEvent::ExtensionClipboard(text) => {
                self.pending_clipboard = Some(text);
            }
            BackendEvent::ExtensionOpenUrl(url) => {
                self.pending_open_url = Some(url);
            }
        }
    }

    fn apply_live(&mut self, event: LiveEvent) {
        match event {
            LiveEvent::TransportConnected => {
                self.connection = Connection::Ready;
                self.startup = Startup::Ready;
            }
            LiveEvent::TransportDisconnected(reason) => {
                self.connection = Connection::Reconnecting(reason);
            }
            LiveEvent::SessionsDirty => {}
            LiveEvent::SessionDeleted { session_id } => {
                self.remove_session_local(&session_id);
            }
            LiveEvent::SessionRenamed { session_id, title } => {
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
                    session.title = Some(title.clone());
                }
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.session.title = Some(title);
                }
            }
            LiveEvent::SessionPermissions {
                session_id,
                permissions,
            } => {
                if let Some(session) = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.permissions = if permissions.is_null() {
                        None
                    } else {
                        Some(permissions.clone())
                    };
                }
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.session.permissions = if permissions.is_null() {
                        None
                    } else {
                        Some(permissions)
                    };
                }
            }
            LiveEvent::SessionViewed { session_id, idle } => {
                self.mark_viewed_local(&session_id, idle);
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.session.time.viewed = Some(
                        chat.session
                            .time
                            .viewed
                            .map_or(idle, |viewed| viewed.max(idle)),
                    );
                }
            }
            LiveEvent::SessionModel { session_id, model } => {
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
                    session.model = Some(model.clone());
                }
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.model = Some(model.clone());
                    chat.session.model = Some(model);
                }
            }
            LiveEvent::SessionAgent { session_id, agent } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.agent = Some(agent.clone());
                    chat.session.agent = Some(agent);
                }
            }
            LiveEvent::Execution {
                session_id,
                state,
                error,
            } => {
                let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) else {
                    return;
                };
                match state {
                    ExecutionState::Started => {
                        chat.busy = true;
                        chat.turn_started = Some(now_ms());
                        chat.error = None;
                    }
                    ExecutionState::Succeeded => {
                        chat.busy = false;
                        chat.turn_started = None;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Succeeded);
                        chat.session.time.idle = Some(now_ms());
                        chat.scroll_to_bottom = true;
                    }
                    ExecutionState::Failed => {
                        chat.busy = false;
                        chat.turn_started = None;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Failed);
                        chat.session.time.idle = Some(now_ms());
                        if let Some(error) = error {
                            chat.error = Some(error.message.clone());
                            self.toast(ToastKind::Error, error.message);
                        }
                    }
                    ExecutionState::Interrupted => {
                        chat.busy = false;
                        chat.turn_started = None;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Interrupted);
                        chat.session.time.idle = Some(now_ms());
                    }
                }
            }
            LiveEvent::StepStarted {
                session_id,
                message_id,
                agent,
                model,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.busy = true;
                    let assistant = chat.assistant_mut(&message_id);
                    if assistant.agent.is_none() {
                        assistant.agent = agent;
                    }
                    if assistant.model.is_none() {
                        assistant.model = model;
                    }
                }
            }
            LiveEvent::StepEnded {
                session_id,
                message_id,
                cost,
                tokens,
                finish,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let assistant = chat.assistant_mut(&message_id);
                    assistant.cost = cost;
                    if tokens.is_some() {
                        assistant.tokens = tokens;
                    }
                    assistant.finish = finish;
                    if assistant.time.completed.is_none() {
                        assistant.time.completed = Some(now_ms());
                    }
                    chat.scroll_to_bottom = true;
                }
            }
            LiveEvent::StepFailed {
                session_id,
                message_id,
                error,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let assistant = chat.assistant_mut(&message_id);
                    assistant.error = error.clone();
                    assistant.time.completed = Some(now_ms());
                    if let Some(error) = error {
                        chat.error = Some(error.message);
                    }
                }
            }
            LiveEvent::TextDelta {
                session_id,
                message_id,
                ordinal,
                delta,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.busy = true;
                    chat.set_text(&message_id, ordinal, None, Some(&delta));
                    chat.scroll_to_bottom = true;
                }
            }
            LiveEvent::TextEnded {
                session_id,
                message_id,
                ordinal,
                text,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.set_text(&message_id, ordinal, Some(&text), None);
                }
            }
            LiveEvent::ReasoningDelta {
                session_id,
                message_id,
                ordinal,
                delta,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.set_reasoning(&message_id, ordinal, None, Some(&delta));
                }
            }
            LiveEvent::ReasoningEnded {
                session_id,
                message_id,
                ordinal,
                text,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.set_reasoning(&message_id, ordinal, Some(&text), None);
                }
            }
            LiveEvent::ToolInputStarted {
                session_id,
                message_id,
                tool_id,
                name,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    if !name.is_empty() {
                        tool.name = name;
                    }
                    tool.state = ToolState::Streaming(Default::default());
                }
            }
            LiveEvent::ToolInputDelta {
                session_id,
                message_id,
                tool_id,
                delta,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    if let ToolState::Streaming(state) = &mut tool.state {
                        state.input.push_str(&delta);
                    }
                }
            }
            LiveEvent::ToolInputEnded {
                session_id,
                message_id,
                tool_id,
                text,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    tool.state =
                        ToolState::Streaming(rchamber_api::models::StreamingTool { input: text });
                }
            }
            LiveEvent::ToolCalled {
                session_id,
                message_id,
                tool_id,
                name,
                input,
                executed,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    tool.name = name;
                    tool.executed = executed;
                    tool.state = ToolState::Running(rchamber_api::models::RunningTool {
                        input,
                        metadata: None,
                    });
                    tool.time.ran = Some(now_ms());
                    chat.scroll_to_bottom = true;
                }
            }
            LiveEvent::ToolProgress {
                session_id,
                message_id,
                tool_id,
                metadata,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    if let ToolState::Running(running) = &mut tool.state {
                        running.metadata = Some(metadata);
                    }
                }
            }
            LiveEvent::ToolSuccess {
                session_id,
                message_id,
                tool_id,
                content,
                executed,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    let input = tool
                        .state
                        .input()
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let metadata = match &tool.state {
                        ToolState::Running(r) => r.metadata.clone(),
                        _ => None,
                    };
                    tool.executed = executed;
                    tool.state = ToolState::Completed(rchamber_api::models::CompletedTool {
                        input,
                        content,
                        metadata,
                    });
                    tool.time.completed = Some(now_ms());
                }
            }
            LiveEvent::ToolFailed {
                session_id,
                message_id,
                tool_id,
                error,
                content,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    let tool = chat.tool_mut(&message_id, &tool_id);
                    let input = tool
                        .state
                        .input()
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    tool.state = ToolState::Error(rchamber_api::models::ErrorTool {
                        input,
                        error,
                        content,
                        metadata: None,
                    });
                    tool.time.completed = Some(now_ms());
                }
            }
            LiveEvent::PermissionAsked(request) => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|chat| chat.session.id == request.session_id)
                {
                    if let Some(chat) = &mut self.active {
                        if !chat.pending.iter().any(|p| p.id == request.id) {
                            chat.pending.push(request);
                        }
                        chat.awaiting_permission = true;
                    }
                    self.toast(ToastKind::Warning, "Permission requested");
                }
            }
            LiveEvent::PermissionReplied {
                session_id,
                request_id,
            } => {
                if let Some(chat) = self.active.as_mut().filter(|c| c.session.id == session_id) {
                    chat.pending.retain(|p| p.id != request_id);
                    chat.awaiting_permission = !chat.pending.is_empty();
                }
            }
            LiveEvent::MessagesDirty { session_id } => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|c| c.session.id == session_id)
                    && let Some(chat) = &mut self.active
                {
                    chat.loading = chat.messages.is_empty();
                }
            }
            LiveEvent::InboxDelivered { session_id } => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|c| c.session.id == session_id)
                    && let Some(chat) = &mut self.active
                {
                    chat.busy = true;
                }
            }
            LiveEvent::Unknown => {}
        }
    }

    pub fn toast(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.toast_seq += 1;
        self.toasts.push(Toast {
            id: self.toast_seq,
            kind,
            message: message.into(),
            created: now_ms(),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn expire_toasts(&mut self) {
        if self.toasts_paused {
            return;
        }
        let now = now_ms();
        self.toasts
            .retain(|toast| now - toast.created < TOAST_LIFETIME_MS + TOAST_EXIT_MS);
    }

    pub fn tick(&mut self) {
        self.expire_toasts();
    }

    pub fn sorted_sessions(&self) -> Vec<&Session> {
        let needle = self.search.trim().to_lowercase();
        let needle = if needle.is_empty() {
            None
        } else {
            Some(needle)
        };
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| match &needle {
                None => true,
                Some(needle) => {
                    session.title().to_lowercase().contains(needle)
                        || session.id.to_lowercase().contains(needle)
                        || session
                            .directory()
                            .map(|directory| directory.to_lowercase().contains(needle))
                            .unwrap_or(false)
                }
            })
            .collect();
        sort_sessions_by_recency(&mut sessions);
        sessions
    }

    pub fn archived_sessions(&self) -> Vec<&Session> {
        self.sessions
            .iter()
            .filter(|session| session.time.archived.is_some())
            .collect()
    }

    pub fn sessions_for_project(&self, project: &ProjectEntry) -> Vec<&Session> {
        self.sessions
            .iter()
            .filter(|session| {
                session.project_id == project.id
                    || session
                        .directory()
                        .is_some_and(|directory| paths_match(directory, &project.canonical))
            })
            .collect()
    }

    pub fn request_session_preview(&mut self, id: &str) -> bool {
        if self.session_previews.contains_key(id) || !self.preview_pending.insert(id.to_string()) {
            return false;
        }
        true
    }

    pub fn is_unread(&self, session: &Session) -> bool {
        if self.active_id() == Some(session.id.as_str())
            || session.parent_id.is_some()
            || session.time.archived.is_some()
        {
            return false;
        }
        match session.time.idle {
            Some(idle) => session.time.viewed.is_none_or(|viewed| viewed < idle),
            None => false,
        }
    }

    pub fn is_pinned(&self, session: &Session) -> bool {
        metadata_flag(session, "pinned")
    }

    pub fn is_work(&self, session: &Session) -> bool {
        metadata_flag(session, "work")
    }

    pub fn set_pinned_local(&mut self, id: &str, pinned: bool) {
        self.update_session_local(id, |session| set_metadata_flag(session, "pinned", pinned));
    }

    pub fn set_work_local(&mut self, id: &str, work: bool) {
        self.update_session_local(id, |session| set_metadata_flag(session, "work", work));
    }

    pub fn mark_viewed_local(&mut self, id: &str, idle: f64) {
        self.update_session_local(id, |session| {
            session.time.viewed = Some(session.time.viewed.map_or(idle, |viewed| viewed.max(idle)));
        });
    }

    pub fn tracked_projects(&self) -> Vec<TrackedProject> {
        let mut projects: Vec<TrackedProject> = self
            .workspace_store
            .projects
            .iter()
            .filter(|project| !project.hidden)
            .map(|project| TrackedProject {
                directory: project.canonical.clone(),
                since: project.added_at - 1_000.0,
            })
            .collect();
        if let Some(chats) = WorkspaceStore::chats_directory()
            && !projects
                .iter()
                .any(|project| paths_match(&project.directory, &chats.to_string_lossy()))
        {
            projects.push(TrackedProject {
                directory: chats.to_string_lossy().to_string(),
                since: 0.0,
            });
        }
        projects
    }

    pub fn workspace_projects(&self) -> Vec<&ProjectEntry> {
        self.workspace_store.ordered_projects()
    }

    pub fn project_by_id(&self, id: &str) -> Option<&ProjectEntry> {
        self.workspace_store
            .projects
            .iter()
            .find(|project| project.id == id)
    }

    pub fn project_for_session(&self, session: &Session) -> Option<&ProjectEntry> {
        self.workspace_store.project_for_session(session)
    }

    pub fn add_project(&mut self, directory: &str, name: Option<&str>) -> String {
        let id = self.workspace_store.add_local_project(directory, name);
        self.workspace_store.save();
        id
    }

    pub fn remove_project(&mut self, id: &str) {
        self.workspace_store.remove_project(id);
        self.workspace_store.save();
    }

    pub fn create_folder(&mut self, project_id: &str, name: &str) -> String {
        let id = self.workspace_store.create_folder(project_id, name);
        self.workspace_store.save();
        id
    }

    pub fn rename_folder(&mut self, id: &str, name: &str) {
        self.workspace_store.rename_folder(id, name);
        self.workspace_store.save();
    }

    pub fn delete_folder(&mut self, id: &str) {
        self.workspace_store.delete_folder(id);
        self.workspace_store.save();
    }

    pub fn set_folder_collapsed(&mut self, id: &str, collapsed: bool) {
        self.workspace_store.set_folder_collapsed(id, collapsed);
        self.workspace_store.save();
    }

    pub fn assign_session_to_folder(&mut self, folder_id: &str, session_id: &str) {
        self.workspace_store.assign_session(folder_id, session_id);
        self.workspace_store.save();
    }

    pub fn unassign_session_from_folder(&mut self, session_id: &str) {
        self.workspace_store.unassign_session(session_id);
        self.workspace_store.save();
    }

    pub fn forget_sessions_from_folders(&mut self, ids: &[String]) {
        for id in ids {
            self.workspace_store.unassign_session(id);
        }
        self.workspace_store.save();
    }

    pub fn rename_project(&mut self, id: &str, name: &str) {
        if let Some(project) = self
            .workspace_store
            .projects
            .iter_mut()
            .find(|project| project.id == id)
        {
            project.name_override = Some(name.to_string());
            self.workspace_store.save();
        }
    }

    fn remove_session_local(&mut self, id: &str) {
        self.sessions.retain(|session| session.id != id);
        self.session_previews.remove(id);
        self.preview_pending.remove(id);
        if self
            .active
            .as_ref()
            .is_some_and(|chat| chat.session.id == id)
        {
            self.active = None;
        }
        if self.workspace_store.folder_for_session(id).is_some() {
            self.workspace_store.unassign_session(id);
            self.workspace_store.save();
        }
    }

    fn update_session_local(&mut self, id: &str, mut update: impl FnMut(&mut Session)) {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            update(session);
        }
        if let Some(chat) = self.active.as_mut().filter(|chat| chat.session.id == id) {
            update(&mut chat.session);
        }
    }

    pub fn model_for(&self, reference: &ModelRef) -> Option<&Model> {
        self.models
            .iter()
            .find(|model| model.id == reference.id && model.provider_id == reference.provider_id)
    }

    pub fn enabled_models(&self) -> Vec<&Model> {
        self.models.iter().filter(|m| m.enabled).collect()
    }

    pub fn default_model(&self) -> Option<ModelRef> {
        let model = self
            .enabled_models()
            .into_iter()
            .find(|m| m.id == "deepseek-v4.1-flash" && m.provider_id == "opencode-go")
            .or_else(|| self.enabled_models().into_iter().next())?;
        Some(ModelRef {
            id: model.id.clone(),
            provider_id: model.provider_id.clone(),
            variant: model.variants.last().map(|v| v.id.clone()),
        })
    }

    pub fn current_model(&self) -> Option<&Model> {
        let reference = self.active.as_ref().and_then(|chat| chat.model.as_ref())?;
        self.model_for(reference)
    }

    pub fn primary_agents(&self) -> Vec<&Agent> {
        self.agents
            .iter()
            .filter(|agent| !agent.hidden && (agent.mode == "primary" || agent.mode == "all"))
            .collect()
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active.as_ref().map(|chat| chat.session.id.as_str())
    }

    pub fn has_connection(&self) -> bool {
        matches!(
            self.connection,
            Connection::Ready | Connection::Connecting | Connection::Reconnecting(_)
        )
    }

    pub fn attachments_for(&self, key: &str) -> Vec<rchamber_api::PromptFile> {
        self.attachments
            .get(key)
            .map(|items| items.iter().map(|item| item.file.clone()).collect())
            .unwrap_or_default()
    }

    pub fn attachment_items_for(&self, key: &str) -> &[AttachmentItem] {
        self.attachments
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn has_attachments(&self, key: &str) -> bool {
        self.attachments
            .get(key)
            .is_some_and(|items| !items.is_empty())
    }

    pub fn add_attachment(&mut self, key: &str, item: AttachmentItem) {
        self.attachments
            .entry(key.to_string())
            .or_default()
            .push(item);
    }

    pub fn restore_attachments(&mut self, key: &str, items: Vec<AttachmentItem>) {
        self.attachments
            .entry(key.to_string())
            .or_default()
            .extend(items);
    }

    pub fn remove_attachment(&mut self, key: &str, index: usize) {
        let Some(items) = self.attachments.get_mut(key) else {
            return;
        };
        if index >= items.len() {
            return;
        }
        let source = items[index].source_document_id.clone();
        if let Some(source) = source {
            items.retain(|item| item.source_document_id.as_deref() != Some(source.as_str()));
        } else {
            items.remove(index);
        }
    }

    pub fn clear_attachments(&mut self, key: &str) {
        self.attachments.remove(key);
    }

    pub fn save_draft(&mut self) {
        if let Some(chat) = &self.active {
            let current = self
                .drafts
                .get(&chat.session.id)
                .cloned()
                .unwrap_or_default();
            if current != chat.draft {
                self.drafts
                    .insert(chat.session.id.clone(), chat.draft.clone());
                self.drafts_changed_at = now_ms();
            }
        }
        if self.drafts.len() > 50 {
            let excess = self.drafts.len() - 50;
            let keys: Vec<String> = self.drafts.keys().take(excess).cloned().collect();
            for key in keys {
                self.drafts.remove(&key);
            }
        }
    }

    pub fn clear_active(&mut self) {
        self.save_draft();
        self.active = None;
    }
}

fn upsert(sessions: &mut Vec<Session>, session: Session) {
    if let Some(existing) = sessions.iter_mut().find(|s| s.id == session.id) {
        *existing = session;
    } else {
        sessions.push(session);
    }
}

fn sort_sessions_by_recency(sessions: &mut [&Session]) {
    sessions.sort_by(|a, b| {
        b.time
            .updated
            .partial_cmp(&a.time.updated)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn metadata_flag(session: &Session, key: &str) -> bool {
    let Some(value) = session
        .metadata
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .and_then(|metadata| metadata.get("openchamber"))
        .and_then(serde_json::Value::as_object)
        .and_then(|openchamber| openchamber.get(key))
    else {
        return false;
    };
    match key {
        "pinned" => {
            value.as_bool() == Some(true)
                || value
                    .as_object()
                    .is_some_and(|pinned| pinned.contains_key("at"))
        }
        "work" => {
            value.as_bool() == Some(true)
                || value
                    .as_object()
                    .and_then(|work| work.get("state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("open")
        }
        _ => value.as_bool() == Some(true),
    }
}

fn set_metadata_flag(session: &mut Session, key: &str, value: bool) {
    let mut root = match session.metadata.take() {
        Some(serde_json::Value::Object(metadata)) => metadata,
        _ => serde_json::Map::new(),
    };
    let openchamber = root
        .entry("openchamber")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !openchamber.is_object() {
        *openchamber = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(openchamber) = openchamber.as_object_mut() {
        match key {
            "pinned" if value => {
                openchamber.insert("pinned".into(), serde_json::json!({ "at": now_ms() }));
            }
            "pinned" => {
                openchamber.remove("pinned");
            }
            "work" if value => {
                openchamber.insert(
                    "work".into(),
                    serde_json::json!({
                        "state": "open",
                        "openedAt": now_ms(),
                        "openedBy": "user",
                    }),
                );
            }
            "work" => {
                openchamber.insert(
                    "work".into(),
                    serde_json::json!({ "state": "done", "doneAt": now_ms() }),
                );
            }
            _ => {
                openchamber.insert(key.to_string(), serde_json::Value::Bool(value));
            }
        }
    }
    session.metadata = Some(serde_json::Value::Object(root));
}

pub fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

pub fn describe_error(error: &Option<StructuredError>) -> Option<String> {
    error.as_ref().map(|e| e.message.clone())
}

fn merge_messages(local: &[Message], incoming: Vec<Message>, streaming: bool) -> Vec<Message> {
    if !streaming {
        return incoming;
    }
    let mut merged = incoming;
    for message in local {
        let Message::Assistant(local_assistant) = message else {
            continue;
        };
        if local_assistant.time.completed.is_some() {
            continue;
        }
        match merged
            .iter()
            .position(|candidate| candidate.id() == Some(local_assistant.id.as_str()))
        {
            Some(index) => {
                let keep_local = match &merged[index] {
                    Message::Assistant(incoming_assistant) => {
                        incoming_assistant.time.completed.is_none()
                    }
                    _ => true,
                };
                if keep_local {
                    merged[index] = message.clone();
                }
            }
            None => {
                if local_assistant.content.iter().any(|part| match part {
                    Part::Text(text) => !text.text.trim().is_empty(),
                    Part::Reasoning(reasoning) => !reasoning.text.trim().is_empty(),
                    Part::Tool(_) => true,
                    Part::Unknown => false,
                }) {
                    merged.push(message.clone());
                }
            }
        }
    }
    merged
}

fn infer_busy(messages: &[Message]) -> bool {
    for message in messages.iter().rev() {
        match message {
            Message::Assistant(assistant) => return assistant.time.completed.is_none(),
            Message::Idle(_) => return false,
            Message::User(_) | Message::Synthetic(_) | Message::System(_) => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchamber_api::models::{LocationRef, SessionTime};

    fn session(
        id: &str,
        parent: Option<&str>,
        project: &str,
        directory: &str,
        archived: bool,
        idle: f64,
        viewed: Option<f64>,
    ) -> Session {
        Session {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            project_id: project.into(),
            slug: None,
            agent: None,
            model: None,
            cost: 0.0,
            tokens: Default::default(),
            outcome: None,
            permissions: None,
            time: SessionTime {
                created: 0.0,
                updated: idle,
                busy: None,
                idle: Some(idle),
                viewed,
                archived: archived.then_some(idle),
            },
            title: Some(id.into()),
            subpath: None,
            metadata: None,
            revert: None,
            location: Some(LocationRef {
                directory: directory.into(),
            }),
            fork: None,
        }
    }

    fn model(sessions: Vec<Session>) -> AppModel {
        AppModel {
            workspace_store: WorkspaceStore::default(),
            sessions,
            ..AppModel::default()
        }
    }

    #[test]
    fn session_snapshot_separates_archived_sessions() {
        let model = model(vec![
            session("r1", None, "p1", "/tmp/alpha", false, 10.0, None),
            session("c1", Some("r1"), "p1", "/tmp/alpha", false, 11.0, None),
            session("r4", None, "p3", "/tmp/gamma", true, 15.0, None),
        ]);

        assert_eq!(model.sessions.len(), 3);
        assert_eq!(model.archived_sessions().len(), 1);
    }

    #[test]
    fn project_matching_and_unread_use_session_fields() {
        let mut model = model(vec![
            session("r1", None, "p1", "/tmp/alpha", false, 20.0, Some(10.0)),
            session(
                "c1",
                Some("r1"),
                "p1",
                "/tmp/alpha",
                false,
                21.0,
                Some(21.0),
            ),
        ]);
        let project_id = model
            .workspace_store
            .add_local_project("/tmp/alpha/", Some("Alpha"));

        let owned: Vec<&str> = model
            .sessions_for_project(model.project_by_id(&project_id).unwrap())
            .into_iter()
            .map(|session| session.id.as_str())
            .collect();
        assert_eq!(owned, vec!["r1", "c1"]);
        assert!(model.is_unread(&model.sessions[0]));
        assert!(!model.is_unread(&model.sessions[1]));

        model.mark_viewed_local("r1", 20.0);
        assert!(!model.is_unread(&model.sessions[0]));
        assert_eq!(model.sessions[0].time.viewed, Some(20.0));
    }

    #[test]
    fn metadata_flags_merge_without_dropping_other_keys() {
        let mut first = session("r1", None, "p1", "/tmp/alpha", false, 20.0, None);
        first.metadata = Some(serde_json::json!({"other": 1}));
        let mut model = model(vec![first]);

        model.set_pinned_local("r1", true);
        model.set_work_local("r1", true);
        assert!(model.is_pinned(&model.sessions[0]));
        assert!(model.is_work(&model.sessions[0]));
        assert_eq!(
            model.sessions[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("other"))
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );

        model.set_pinned_local("r1", false);
        assert!(!model.is_pinned(&model.sessions[0]));
        assert!(model.is_work(&model.sessions[0]));
    }

    #[test]
    fn activating_a_session_opens_and_selects_its_tab() {
        let mut model = model(vec![session(
            "r1",
            None,
            "p1",
            "/tmp/alpha",
            false,
            20.0,
            None,
        )]);

        model.apply(BackendEvent::SessionActivated(Box::new(session(
            "r1",
            None,
            "p1",
            "/tmp/alpha",
            false,
            20.0,
            None,
        ))));

        assert_eq!(model.active_id(), Some("r1"));
    }

    fn extension(id: &str, panel: bool) -> rchamber_ext::InstalledExtension {
        let mut manifest: rchamber_ext::manifest::Manifest =
            serde_json::from_value(serde_json::json!({
                "id": id,
                "name": id,
                "version": "1.0.0"
            }))
            .unwrap();
        if panel {
            manifest.contributes.panel = Some(rchamber_ext::manifest::PanelContribution {
                title: Some(id.into()),
                icon: None,
            });
        }
        rchamber_ext::InstalledExtension {
            manifest,
            root: std::path::PathBuf::from("/tmp/extension"),
            source: rchamber_ext::InstallSource::Builtin,
            enabled: true,
            grants: Vec::new(),
            installed_at: 0,
            settings: serde_json::Value::Null,
        }
    }

    #[test]
    fn installing_a_panel_extension_requests_approval() {
        let mut model = AppModel::default();
        model.apply(BackendEvent::Extensions(vec![extension("demo", true)]));
        model.apply(BackendEvent::ExtensionInstalled {
            extension_id: "demo".into(),
        });
        assert_eq!(model.pending_extension_approval.as_deref(), Some("demo"));
    }

    #[test]
    fn a_panel_action_with_no_tree_is_not_an_error() {
        let mut model = AppModel::default();
        model.apply(BackendEvent::ExtensionResult {
            call_id: "panel-action".into(),
            extension_id: "demo".into(),
            method: "command".into(),
            value: serde_json::Value::Null,
        });
        assert!(model.extensions.errors.is_empty());
    }

    #[test]
    fn an_invalid_panel_tree_reports_and_a_valid_tree_clears_it() {
        let mut model = AppModel::default();
        model.apply(BackendEvent::ExtensionResult {
            call_id: "panel".into(),
            extension_id: "demo".into(),
            method: "panel".into(),
            value: serde_json::json!({ "not": "a tree" }),
        });
        assert!(model.extensions.errors.contains_key("demo"));

        model.apply(BackendEvent::ExtensionResult {
            call_id: "panel".into(),
            extension_id: "demo".into(),
            method: "panel".into(),
            value: serde_json::json!([{ "kind": "heading", "text": "Hello" }]),
        });
        assert!(!model.extensions.errors.contains_key("demo"));
    }
}
