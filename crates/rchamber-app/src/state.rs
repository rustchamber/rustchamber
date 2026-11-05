use std::collections::HashMap;

use rchamber_api::models::ServerInfo;
use rchamber_api::models::{
    Agent, AssistantMessage, Command, FileDiff, McpServer, Message, MessageTime, Model, ModelRef,
    Part, PermissionRequest, Project, Provider, ReasoningPart, Session, StructuredError, TextPart,
    ToolPart, ToolState, VcsInfo,
};

use crate::action::{BackendEvent, Connection, ExecutionState, LiveEvent, ToastKind};
use crate::attachments::AttachmentItem;
use crate::snippets::Snippet;

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
    pub last_activity: f64,
    pub scroll_to_bottom: bool,
    pub model: Option<ModelRef>,
    pub agent: Option<String>,
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
            last_activity: now_ms(),
            scroll_to_bottom: true,
            model,
            agent,
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

pub struct AppModel {
    pub connection: Connection,
    pub connection_detail: Option<String>,
    pub server: Option<ServerInfo>,
    pub sessions: Vec<Session>,
    pub projects: Vec<Project>,
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
    pub searching: bool,
    pub sessions_loading: bool,
    pub toasts: Vec<Toast>,
    pub toasts_paused: bool,
    pub startup: Startup,
    pub unread: HashMap<String, usize>,
    toast_seq: u64,
    pub composer_focus: bool,
    pub show_settings: bool,
    pub show_sidebar: bool,
    pub palette_open: bool,
}

impl Default for AppModel {
    fn default() -> Self {
        Self {
            connection: Connection::Disconnected,
            connection_detail: None,
            server: None,
            sessions: Vec::new(),
            projects: Vec::new(),
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
            searching: false,
            sessions_loading: false,
            toasts: Vec::new(),
            toasts_paused: false,
            startup: Startup::Idle,
            unread: HashMap::new(),
            toast_seq: 0,
            composer_focus: true,
            show_settings: false,
            show_sidebar: true,
            palette_open: false,
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
                self.sessions_loading = false;
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
                self.unread.remove(&id);
                self.active = Some(chat);
                upsert(&mut self.sessions, session);
                self.composer_focus = true;
            }
            BackendEvent::SessionRemoved(id) => {
                self.sessions.retain(|s| s.id != id);
                if self
                    .active
                    .as_ref()
                    .is_some_and(|chat| chat.session.id == id)
                {
                    self.active = None;
                }
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
                    chat.loading = false;
                    chat.error = None;
                    chat.scroll_to_bottom = true;
                    chat.last_activity = now_ms();
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
            BackendEvent::Projects(projects) => self.projects = projects,
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
                self.workspace = Some(directory);
                self.active = None;
                self.sessions_loading = true;
                self.files = FilesState::default();
            }
            BackendEvent::PromptQueued { session_id } => {
                if let Some(chat) = &mut self.active
                    && chat.session.id == session_id
                {
                    chat.busy = true;
                }
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
            LiveEvent::SessionsDirty => {
                self.sessions_loading = true;
            }
            LiveEvent::SessionDeleted { session_id } => {
                self.sessions.retain(|s| s.id != session_id);
                if self
                    .active
                    .as_ref()
                    .is_some_and(|c| c.session.id == session_id)
                {
                    self.active = None;
                }
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
                    if state != ExecutionState::Started {
                        self.sessions_loading = true;
                    }
                    return;
                };
                match state {
                    ExecutionState::Started => {
                        chat.busy = true;
                        chat.error = None;
                    }
                    ExecutionState::Succeeded => {
                        chat.busy = false;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Succeeded);
                        chat.session.time.idle = Some(now_ms());
                        chat.scroll_to_bottom = true;
                        self.sessions_loading = true;
                    }
                    ExecutionState::Failed => {
                        chat.busy = false;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Failed);
                        chat.session.time.idle = Some(now_ms());
                        if let Some(error) = error {
                            chat.error = Some(error.message.clone());
                            self.toast(ToastKind::Error, error.message);
                        }
                        self.sessions_loading = true;
                    }
                    ExecutionState::Interrupted => {
                        chat.busy = false;
                        chat.session.outcome = Some(rchamber_api::models::Outcome::Interrupted);
                        chat.session.time.idle = Some(now_ms());
                        self.sessions_loading = true;
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
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                if self.search.trim().is_empty() {
                    true
                } else {
                    let needle = self.search.to_lowercase();
                    session.title().to_lowercase().contains(&needle)
                        || session
                            .directory()
                            .map(|d| d.to_lowercase().contains(&needle))
                            .unwrap_or(false)
                }
            })
            .collect();
        sessions.sort_by(|a, b| {
            b.time
                .updated
                .partial_cmp(&a.time.updated)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sessions
    }

    pub fn sessions_for_project(&self, canonical: &str) -> Vec<&Session> {
        self.sorted_sessions()
            .into_iter()
            .filter(|s| s.directory() == Some(canonical))
            .collect()
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

    pub fn workspace_name(&self) -> String {
        let path = self
            .active
            .as_ref()
            .and_then(|chat| chat.session.directory())
            .or(self.workspace.as_deref());
        path.map(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string())
        })
        .unwrap_or_else(|| "RustChamber".into())
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
