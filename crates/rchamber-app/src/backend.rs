use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rchamber_api::models::{Message, Part, Session};
use rchamber_api::process::LaunchOptions;
use rchamber_api::{ApiError, Client, RawEvent, ServerHandle};
use serde_json::{Map, Value};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::Instant;

use crate::action::{
    Action, BackendEvent, Connection, ExecutionState, LaunchConfig, LiveEvent, ToastKind,
    TrackedProject,
};
use crate::live;
use crate::snippets;

pub type Wake = Arc<dyn Fn() + Send + Sync>;

const MCP_SETTLE_ATTEMPTS: u32 = 8;
const MCP_SETTLE_DELAY: Duration = Duration::from_secs(2);
const SESSIONS_PER_PROJECT: u32 = 100;

pub struct Backend {
    pub actions: UnboundedSender<Action>,
    pub events: UnboundedReceiver<BackendEvent>,
}

impl Backend {
    pub fn spawn(config: LaunchConfig, wake: Wake) -> Self {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let config_clone = config.clone();
        std::thread::Builder::new()
            .name("rchamber-backend".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(async move {
                        let mut inner = Inner::new(config_clone, event_tx, wake);
                        inner.run(action_rx).await;
                    }),
                    Err(err) => {
                        let _ = event_tx.send(BackendEvent::Connection(Connection::Failed(
                            format!("could not start async runtime: {err}"),
                        )));
                    }
                }
            })
            .expect("spawn backend thread");
        Self {
            actions: action_tx,
            events: event_rx,
        }
    }

    pub fn send(&self, action: Action) {
        let _ = self.actions.send(action);
    }
}

struct Inner {
    tx: UnboundedSender<BackendEvent>,
    wake: Wake,
    config: LaunchConfig,
    client: Option<Client>,
    server: Option<ServerHandle>,
    password: Option<String>,
    active: Option<String>,
    subscription: Option<rchamber_api::Subscription>,
    cursors: HashMap<String, Option<String>>,
    pending_sessions: bool,
    pending_messages: bool,
    pending_inbox: bool,
    pending_mcp: bool,
    mcp_attempts: u32,
    snippets: Vec<snippets::Snippet>,
    tracked: Vec<TrackedProject>,
    cached_sessions: HashMap<String, Vec<Session>>,
    next_flush: Option<Instant>,
}

enum Control {
    Continue,
    Reconnect,
    Exit,
}

impl Inner {
    fn new(config: LaunchConfig, tx: UnboundedSender<BackendEvent>, wake: Wake) -> Self {
        Self {
            tx,
            wake,
            config,
            client: None,
            server: None,
            password: None,
            active: None,
            subscription: None,
            cursors: HashMap::new(),
            pending_sessions: false,
            pending_messages: false,
            pending_inbox: false,
            pending_mcp: false,
            mcp_attempts: 0,
            snippets: Vec::new(),
            tracked: Vec::new(),
            cached_sessions: HashMap::new(),
            next_flush: None,
        }
    }

    async fn run(&mut self, mut actions: UnboundedReceiver<Action>) {
        loop {
            self.connect().await;
            let mut sub = self.subscription.take();
            loop {
                let due = self.next_flush;
                let has_sub = sub.is_some();
                tokio::select! {
                    action = actions.recv() => {
                        match action {
                            Some(Action::Shutdown) | None => return,
                            Some(Action::Reconnect) => {
                                break;
                            }
                            Some(action) => {
                                if matches!(self.handle(action).await, Control::Exit) {
                                    return;
                                }
                            }
                        }
                    }
                    raw = async {
                        match sub.as_mut() {
                            Some(sub) => sub.rx.recv().await,
                            None => std::future::pending::<Option<RawEvent>>().await,
                        }
                    }, if has_sub => {
                        if let Some(raw) = raw {
                            self.event(raw).await;
                        }
                    }
                    _ = tokio::time::sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => {
                        self.flush().await;
                    }
                }
            }
            if let Some(sub) = sub.take() {
                sub.stop();
            }
            self.subscription = None;
            self.drop_connection();
        }
    }

    async fn connect(&mut self) {
        self.emit(BackendEvent::Connection(Connection::Connecting));
        let options = LaunchOptions {
            directory: self.config.workspace.as_ref().map(std::path::PathBuf::from),
            binary: None,
            port: None,
            password: self.config.password.clone(),
            external_url: self.config.endpoint.clone(),
        };
        match rchamber_api::launch(options).await {
            Ok(handle) => {
                self.password = handle.password.clone();
                self.client = Some(handle.client.clone());
                self.server = Some(handle);
                self.emit(BackendEvent::Connection(Connection::Ready));
                if let Some(server) = &self.server {
                    self.emit(BackendEvent::Server(Box::new(server.info.clone())));
                }
                if let Some(client) = &self.client {
                    self.subscription = Some(client.subscribe(None));
                }
                self.bootstrap().await;
            }
            Err(err) => {
                self.client = None;
                self.server = None;
                self.emit(BackendEvent::Connection(Connection::Failed(
                    err.to_string(),
                )));
            }
        }
    }

    fn drop_connection(&mut self) {
        self.server = None;
        self.client = None;
        self.subscription = None;
        self.active = None;
        self.cursors.clear();
        self.pending_mcp = false;
        self.mcp_attempts = 0;
    }

    async fn bootstrap(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        tracing::debug!("bootstrapping OpenCode client");
        let models = with_retry(|| {
            let client = client.clone();
            async move { client.models().await }
        })
        .await;
        let agents = with_retry(|| {
            let client = client.clone();
            async move { client.agents().await }
        })
        .await;
        let commands = with_retry(|| {
            let client = client.clone();
            async move { client.commands().await }
        })
        .await;
        let projects = with_retry(|| {
            let client = client.clone();
            async move { client.projects().await }
        })
        .await;
        let vcs = with_retry(|| {
            let client = client.clone();
            async move { client.vcs_info().await }
        })
        .await;
        let skills = with_retry(|| {
            let client = client.clone();
            async move { client.skills().await }
        })
        .await;
        let providers = with_retry(|| {
            let client = client.clone();
            async move { client.providers().await }
        })
        .await;
        match models {
            Ok(models) => self.emit(BackendEvent::Models(models)),
            Err(err) => self.error("Could not load models", err),
        }
        match agents {
            Ok(agents) => self.emit(BackendEvent::Agents(agents)),
            Err(err) => self.error("Could not load agents", err),
        }
        match commands {
            Ok(commands) => self.emit(BackendEvent::Commands(commands)),
            Err(err) => self.error("Could not load commands", err),
        }
        match projects {
            Ok(projects) => self.emit(BackendEvent::Projects(projects)),
            Err(err) => self.error("Could not load projects", err),
        }
        match vcs {
            Ok(vcs) => self.emit(BackendEvent::Vcs(Some(vcs))),
            Err(_) => self.emit(BackendEvent::Vcs(None)),
        }
        if let Ok(skills) = skills {
            self.emit(BackendEvent::Skills(skills))
        }
        if let Ok(providers) = providers {
            self.emit(BackendEvent::Providers(providers))
        }
        self.load_mcp(&client).await;
        self.reload_snippets(client.directory());
        if !self.tracked.is_empty() {
            self.refresh_sessions(&client).await;
        }
    }

    fn reload_snippets(&mut self, workspace: Option<&str>) {
        let snippets = snippets::load(workspace);
        self.emit(BackendEvent::Snippets(snippets.clone()));
        self.snippets = snippets;
    }

    async fn handle(&mut self, action: Action) -> Control {
        if let Action::SetTrackedProjects(projects) = &action {
            let directories: HashSet<&str> = projects
                .iter()
                .map(|project| project.directory.as_str())
                .collect();
            self.cached_sessions
                .retain(|directory, _| directories.contains(directory.as_str()));
            self.tracked = projects.clone();
            if let Some(client) = self.client.clone() {
                self.refresh_sessions(&client).await;
            }
            return Control::Continue;
        }
        let Some(client) = self.client.clone() else {
            match action {
                Action::Reconnect => return Control::Reconnect,
                Action::Shutdown => return Control::Exit,
                Action::SetWorkspace(directory) => {
                    self.config.workspace = Some(directory.clone());
                    self.emit(BackendEvent::WorkspaceChanged(directory));
                    return Control::Reconnect;
                }
                _ => {
                    self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: "Not connected to an OpenCode server".into(),
                    });
                    return Control::Continue;
                }
            }
        };
        match action {
            Action::SetTrackedProjects(_) => {}
            Action::SelectSession(id) => {
                self.activate(client, id).await;
            }
            Action::CreateSession {
                directory,
                prompt,
                files,
                model,
                agent,
                allow_all,
            } => {
                let permissions = allow_all.then(|| {
                    serde_json::json!([
                        { "action": "*", "resource": "*", "effect": "allow" }
                    ])
                });
                match client
                    .create_session(
                        &directory,
                        None,
                        agent.as_deref(),
                        model.as_ref(),
                        permissions.as_ref(),
                    )
                    .await
                {
                    Ok(mut session) => {
                        let id = session.id.clone();
                        mark_work_open(&mut session);
                        if let Some(metadata) = session.metadata.clone()
                            && let Err(err) = client.update_session_metadata(&id, metadata).await
                        {
                            self.error("Could not track session", err);
                        }
                        self.emit(BackendEvent::SessionUpsert(Box::new(session)));
                        self.activate(client.clone(), id.clone()).await;
                        if let Some(prompt) = prompt.filter(|p| !p.trim().is_empty()) {
                            self.send_prompt(&client, &id, &prompt, &files, "steer")
                                .await;
                        }
                    }
                    Err(err) => self.error("Could not create session", err),
                }
            }
            Action::Send {
                session_id,
                text,
                files,
                delivery,
            } => {
                if text.trim().is_empty() && files.is_empty() {
                    return Control::Continue;
                }
                self.send_prompt(&client, &session_id, &text, &files, &delivery)
                    .await;
            }
            Action::LoadInbox(session_id) => {
                self.refresh_inbox(&client, &session_id).await;
            }
            Action::CancelInbox {
                session_id,
                inbox_id,
            } => match client.cancel_inbox(&session_id, &inbox_id).await {
                Ok(()) => self.refresh_inbox(&client, &session_id).await,
                Err(err) => self.error("Could not cancel queued message", err),
            },
            Action::SetInboxDelivery {
                session_id,
                inbox_id,
                delivery,
            } => match client
                .set_inbox_delivery(&session_id, &inbox_id, &delivery)
                .await
            {
                Ok(()) => self.refresh_inbox(&client, &session_id).await,
                Err(err) => self.error("Could not change delivery", err),
            },
            Action::ReorderInbox {
                session_id,
                inbox_ids,
            } => {
                self.reorder_inbox(&client, &session_id, &inbox_ids).await;
            }
            Action::Interrupt(session_id) => match client.interrupt(&session_id).await {
                Ok(()) => self.emit(BackendEvent::Toast {
                    kind: ToastKind::Info,
                    message: "Stopping the agent…".into(),
                }),
                Err(err) => self.error("Could not stop the agent", err),
            },
            Action::RenameSession { id, title } => match client.rename_session(&id, &title).await {
                Ok(()) => {
                    self.emit(BackendEvent::Live(LiveEvent::SessionRenamed {
                        session_id: id,
                        title,
                    }));
                }
                Err(err) => self.error("Could not rename session", err),
            },
            Action::ArchiveSession { id, archived } => {
                match client.archive_session(&id, archived).await {
                    Ok(()) => {
                        self.schedule_sessions();
                        if archived {
                            self.emit(BackendEvent::SessionRemoved(id));
                        }
                    }
                    Err(err) => self.error("Could not archive session", err),
                }
            }
            Action::DeleteSession(id) => match client.delete_session(&id).await {
                Ok(()) => {
                    self.emit(BackendEvent::SessionRemoved(id));
                    self.schedule_sessions();
                }
                Err(err) => self.error("Could not delete session", err),
            },
            Action::SetSessionPinned { id, pinned } => {
                self.set_session_metadata_flag(&client, &id, "pinned", pinned)
                    .await;
            }
            Action::SetSessionWork { id, work } => {
                self.set_session_metadata_flag(&client, &id, "work", work)
                    .await;
            }
            Action::SetSessionPermissions { id, allow_all } => {
                let permissions = if allow_all {
                    serde_json::json!([
                        { "action": "*", "resource": "*", "effect": "allow" }
                    ])
                } else {
                    serde_json::json!([])
                };
                match client.update_session_permissions(&id, &permissions).await {
                    Ok(()) => match client.get_session(&id).await {
                        Ok(session) => {
                            self.emit(BackendEvent::SessionUpsert(Box::new(session)));
                        }
                        Err(err) => self.error("Could not reload session", err),
                    },
                    Err(err) => self.error("Could not change permissions", err),
                }
            }
            Action::MarkSessionViewed(id) => match view_session(&client, &id).await {
                Ok(Some(idle)) => {
                    self.emit(BackendEvent::SessionViewed { id, idle });
                }
                Ok(None) => {}
                Err(err) => self.error("Could not mark session as viewed", err),
            },
            Action::MarkAllSessionsViewed(ids) => {
                let mut failed = 0usize;
                for id in ids.into_iter().take(2000) {
                    match view_session(&client, &id).await {
                        Ok(Some(idle)) => {
                            self.emit(BackendEvent::SessionViewed { id, idle });
                        }
                        Ok(None) => {}
                        Err(_) => failed += 1,
                    }
                }
                if failed > 0 {
                    self.emit(BackendEvent::Toast {
                        kind: ToastKind::Warning,
                        message: format!("Could not mark {failed} sessions as viewed"),
                    });
                }
            }
            Action::MoveSession { id, directory } => {
                if let Err(err) = client.move_session(&id, &directory).await {
                    self.error("Could not move session", err);
                } else {
                    self.refresh_sessions(&client).await;
                }
            }
            Action::ArchiveSessions { ids, archived } => {
                let mut changed = false;
                let mut failed = 0usize;
                for id in ids {
                    match client.archive_session(&id, archived).await {
                        Ok(()) => changed = true,
                        Err(_) => failed += 1,
                    }
                }
                if failed > 0 {
                    self.emit(BackendEvent::Toast {
                        kind: ToastKind::Warning,
                        message: format!("Could not archive {failed} sessions"),
                    });
                }
                if changed {
                    self.schedule_sessions();
                    self.emit(BackendEvent::Live(LiveEvent::SessionsDirty));
                }
            }
            Action::DeleteSessions(ids) => {
                let mut failed = 0usize;
                for id in ids {
                    match client.delete_session(&id).await {
                        Ok(()) => self.emit(BackendEvent::SessionRemoved(id)),
                        Err(_) => failed += 1,
                    }
                }
                if failed > 0 {
                    self.emit(BackendEvent::Toast {
                        kind: ToastKind::Warning,
                        message: format!("Could not delete {failed} sessions"),
                    });
                }
                self.schedule_sessions();
            }
            Action::SwitchModel { session_id, model } => {
                match client.switch_model(&session_id, &model).await {
                    Ok(()) => {
                        self.emit(BackendEvent::Live(LiveEvent::SessionModel {
                            session_id,
                            model,
                        }));
                    }
                    Err(err) => self.error("Could not switch model", err),
                }
            }
            Action::SwitchAgent { session_id, agent } => {
                match client.switch_agent(&session_id, &agent).await {
                    Ok(()) => {
                        self.emit(BackendEvent::Live(LiveEvent::SessionAgent {
                            session_id,
                            agent,
                        }));
                    }
                    Err(err) => self.error("Could not switch agent", err),
                }
            }
            Action::ReplyPermission {
                session_id,
                request_id,
                decision,
            } => match client
                .reply_permission(&session_id, &request_id, &decision, None)
                .await
            {
                Ok(()) => {
                    self.emit(BackendEvent::Live(LiveEvent::PermissionReplied {
                        session_id,
                        request_id,
                    }));
                }
                Err(err) => self.error("Could not answer permission", err),
            },
            Action::SetWorkspace(directory) => {
                self.config.workspace = Some(directory.clone());
                let mut next = client.clone();
                next.set_directory(Some(directory.clone()));
                self.client = Some(next.clone());
                self.emit(BackendEvent::WorkspaceChanged(directory.clone()));
                self.reload_snippets(next.directory());
                self.refresh_sessions(&next).await;
                match next.projects().await {
                    Ok(projects) => self.emit(BackendEvent::Projects(projects)),
                    Err(err) => self.error("Could not load projects", err),
                }
            }
            Action::LoadSessionPreview(id) => {
                let text = match client.list_messages(&id, 1).await {
                    Ok(messages) => messages.last().map(preview_text).unwrap_or_default(),
                    Err(_) => String::new(),
                };
                self.emit(BackendEvent::SessionPreview { id, text });
            }
            Action::LoadOlder(session_id) => {
                let cursor = self.cursors.get(&session_id).cloned().flatten();
                if let Some(cursor) = cursor {
                    match client.list_messages_before(&session_id, 100, &cursor).await {
                        Ok((messages, previous)) => {
                            if let Some(slot) = self.cursors.get_mut(&session_id) {
                                *slot = previous.clone();
                            }
                            self.emit(BackendEvent::OlderMessages {
                                session_id,
                                messages,
                                next: previous,
                            });
                        }
                        Err(err) => self.error("Could not load earlier messages", err),
                    }
                }
            }
            Action::LoadDiff(session_id) => match client.session_diff(&session_id).await {
                Ok(files) => self.emit(BackendEvent::Diff { session_id, files }),
                Err(err) => self.error("Could not load changes", err),
            },
            Action::LoadDirectory(path) => match client.fs_list(&path).await {
                Ok(entries) => self.emit(BackendEvent::Files { path, entries }),
                Err(err) => self.error("Could not read directory", err),
            },
            Action::LoadFile(path) => match client.fs_read(&path).await {
                Ok(bytes) => self.emit(BackendEvent::FileContents { path, bytes }),
                Err(err) => self.error("Could not read file", err),
            },
            Action::SearchFiles(query) => match client.fs_find(&query, 60).await {
                Ok(entries) => self.emit(BackendEvent::FileSearch { query, entries }),
                Err(err) => self.error("Could not search files", err),
            },
            Action::Configure {
                endpoint,
                password,
                workspace,
            } => {
                self.config.endpoint = endpoint;
                self.config.password = password;
                self.config.workspace = workspace;
                return Control::Reconnect;
            }
            Action::Reconnect => return Control::Reconnect,
            Action::Shutdown => return Control::Exit,
        }
        Control::Continue
    }

    fn expand_snippets(&self, text: &str, client: &Client) -> String {
        if self.snippets.is_empty() {
            snippets::expand(text, client.directory())
        } else {
            snippets::expand_with(text, &self.snippets)
        }
    }

    async fn send_prompt(
        &mut self,
        client: &Client,
        session_id: &str,
        text: &str,
        files: &[rchamber_api::PromptFile],
        delivery: &str,
    ) {
        let expanded = self.expand_snippets(text, client);
        let text = expanded.as_str();
        match client
            .prompt_full(session_id, text, files, Some(delivery))
            .await
        {
            Ok(_) => {
                self.active = Some(session_id.to_string());
                self.emit(BackendEvent::PromptQueued {
                    session_id: session_id.to_string(),
                });
                tokio::time::sleep(Duration::from_millis(140)).await;
                self.refresh_messages(client, session_id).await;
                self.refresh_inbox(client, session_id).await;
            }
            Err(err) => self.error("Could not send message", err),
        }
    }

    async fn activate(&mut self, client: Client, id: String) {
        match client.get_session(&id).await {
            Ok(session) => {
                self.active = Some(id.clone());
                self.emit(BackendEvent::SessionActivated(Box::new(session)));
            }
            Err(err) => {
                self.error("Could not open session", err);
                return;
            }
        }
        let messages = client.messages_page(&id, 100);
        let permissions = client.pending_permissions(&id);
        let diff = client.session_diff(&id);
        let (messages, permissions, diff) = tokio::join!(messages, permissions, diff);
        match messages {
            Ok(page) => {
                self.cursors
                    .insert(id.clone(), page.cursor.and_then(|c| c.next));
                self.emit(BackendEvent::Messages {
                    session_id: id.clone(),
                    messages: page.data,
                });
            }
            Err(err) => self.error("Could not load messages", err),
        }
        match permissions {
            Ok(requests) => self.emit(BackendEvent::Permissions {
                session_id: id.clone(),
                requests,
            }),
            Err(err) => self.error("Could not load permissions", err),
        }
        match diff {
            Ok(files) => self.emit(BackendEvent::Diff {
                session_id: id.clone(),
                files,
            }),
            Err(err) => self.error("Could not load changes", err),
        }
        self.refresh_inbox(&client, &id).await;
    }

    async fn set_session_metadata_flag(
        &mut self,
        client: &Client,
        id: &str,
        key: &str,
        value: bool,
    ) {
        let mut session = match client.get_session(id).await {
            Ok(session) => session,
            Err(err) => {
                self.error("Could not load session", err);
                return;
            }
        };
        let mut root = match session.metadata.take() {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let openchamber = root
            .entry("openchamber")
            .or_insert_with(|| Value::Object(Map::new()));
        if !openchamber.is_object() {
            *openchamber = Value::Object(Map::new());
        }
        if let Some(openchamber) = openchamber.as_object_mut() {
            match key {
                "pinned" if value => {
                    openchamber.insert(
                        "pinned".into(),
                        serde_json::json!({ "at": crate::state::now_ms() }),
                    );
                }
                "pinned" => {
                    openchamber.remove("pinned");
                }
                "work" if value => {
                    openchamber.insert(
                        "work".into(),
                        serde_json::json!({
                            "state": "open",
                            "openedAt": crate::state::now_ms(),
                            "openedBy": "user",
                        }),
                    );
                }
                "work" => {
                    openchamber.insert(
                        "work".into(),
                        serde_json::json!({
                            "state": "done",
                            "doneAt": crate::state::now_ms(),
                        }),
                    );
                }
                _ => {
                    openchamber.insert(key.to_string(), Value::Bool(value));
                }
            }
        }
        let metadata = Value::Object(root);
        session.metadata = Some(metadata.clone());
        if let Err(err) = client.update_session_metadata(id, metadata).await {
            self.error("Could not update session", err);
            return;
        }
        match client.get_session(id).await {
            Ok(updated) => self.emit(BackendEvent::SessionMetadataUpdated(Box::new(updated))),
            Err(err) => {
                self.emit(BackendEvent::SessionMetadataUpdated(Box::new(session)));
                self.error("Could not reload session", err);
            }
        }
    }

    async fn reorder_inbox(&mut self, client: &Client, session_id: &str, inbox_ids: &[String]) {
        let current = match client.inbox(session_id).await {
            Ok(items) => items,
            Err(err) => {
                self.error("Could not load queue", err);
                return;
            }
        };
        if current.len() != inbox_ids.len() {
            self.error(
                "Could not reorder queue",
                ApiError::Other("the queue changed while reordering".into()),
            );
            self.refresh_inbox(client, session_id).await;
            return;
        }
        let mut by_id = HashMap::new();
        for item in current {
            let Some(id) = item.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            if item.get("type").and_then(|value| value.as_str()) != Some("user") {
                self.error(
                    "Could not reorder queue",
                    ApiError::Other("only user messages can be reordered".into()),
                );
                self.refresh_inbox(client, session_id).await;
                return;
            }
            by_id.insert(id.to_string(), item);
        }
        let mut payloads = Vec::new();
        for id in inbox_ids {
            let Some(item) = by_id.get(id) else {
                self.error(
                    "Could not reorder queue",
                    ApiError::Other("the queue changed while reordering".into()),
                );
                self.refresh_inbox(client, session_id).await;
                return;
            };
            let payload = item.get("payload");
            let text = payload
                .and_then(|payload| payload.get("text"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let files = payload
                .and_then(|payload| payload.get("files"))
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<Vec<rchamber_api::PromptFile>>(value).ok()
                })
                .unwrap_or_default();
            let delivery = item
                .get("delivery")
                .and_then(|value| value.as_str())
                .unwrap_or("queue")
                .to_string();
            payloads.push((text, files, delivery));
        }
        for id in inbox_ids {
            if let Err(err) = client.cancel_inbox(session_id, id).await {
                self.error("Could not reorder queue", err);
                self.refresh_inbox(client, session_id).await;
                return;
            }
        }
        for (text, files, delivery) in payloads {
            let expanded = self.expand_snippets(&text, client);
            if let Err(err) = client
                .prompt_full(session_id, &expanded, &files, Some(&delivery))
                .await
            {
                self.error("Could not restore reordered message", err);
                break;
            }
        }
        self.refresh_inbox(client, session_id).await;
    }

    async fn refresh_inbox(&mut self, client: &Client, session_id: &str) {
        match client.inbox(session_id).await {
            Ok(items) => self.emit(BackendEvent::Inbox {
                session_id: session_id.to_string(),
                items,
            }),
            Err(err) => self.error("Could not load queue", err),
        }
    }

    async fn event(&mut self, raw: RawEvent) {
        let event = live::read(&raw);
        match &event {
            LiveEvent::TransportConnected => {
                self.emit(BackendEvent::Connection(Connection::Ready));
                self.schedule_sessions();
                self.schedule_messages();
            }
            LiveEvent::TransportDisconnected(reason) => {
                self.emit(BackendEvent::Connection(Connection::Reconnecting(
                    reason.clone(),
                )));
            }
            LiveEvent::SessionsDirty | LiveEvent::SessionDeleted { .. } => {
                self.schedule_sessions();
            }
            LiveEvent::MessagesDirty { session_id } | LiveEvent::InboxDelivered { session_id } => {
                if self.active.as_deref() == Some(session_id.as_str()) {
                    self.schedule_messages();
                    self.schedule_inbox();
                }
            }
            LiveEvent::Execution {
                session_id, state, ..
            } if *state != ExecutionState::Started
                && self.active.as_deref() == Some(session_id.as_str()) =>
            {
                self.schedule_messages();
                self.schedule_inbox();
            }
            _ => {}
        }
        self.emit(BackendEvent::Live(event));
    }

    fn schedule_sessions(&mut self) {
        self.pending_sessions = true;
        self.arm_flush();
    }

    fn schedule_messages(&mut self) {
        self.pending_messages = true;
        self.arm_flush();
    }

    fn schedule_inbox(&mut self) {
        self.pending_inbox = true;
        self.arm_flush();
    }

    fn arm_flush(&mut self) {
        self.arm_flush_in(Duration::from_millis(350));
    }

    fn arm_flush_in(&mut self, delay: Duration) {
        let due = Instant::now() + delay;
        self.next_flush = Some(match self.next_flush {
            Some(current) => current.min(due),
            None => due,
        });
    }

    async fn flush(&mut self) {
        self.next_flush = None;
        if self.pending_sessions {
            self.pending_sessions = false;
            if let Some(client) = self.client.clone() {
                self.refresh_sessions(&client).await;
            }
        }
        if self.pending_messages {
            self.pending_messages = false;
            if let (Some(client), Some(active)) = (self.client.clone(), self.active.clone()) {
                self.refresh_messages(&client, &active).await;
            }
        }
        if self.pending_inbox {
            self.pending_inbox = false;
            if let (Some(client), Some(active)) = (self.client.clone(), self.active.clone()) {
                self.refresh_inbox(&client, &active).await;
            }
        }
        if self.pending_mcp {
            self.pending_mcp = false;
            if let Some(client) = self.client.clone() {
                self.mcp_attempts += 1;
                let settled = self.refresh_mcp(&client).await;
                if !settled && self.mcp_attempts < MCP_SETTLE_ATTEMPTS {
                    self.pending_mcp = true;
                    self.arm_flush_in(MCP_SETTLE_DELAY);
                }
            }
        }
    }

    async fn refresh_sessions(&mut self, client: &Client) {
        if self.tracked.is_empty() {
            self.cached_sessions.clear();
            self.emit(BackendEvent::Sessions(Vec::new()));
            return;
        }
        let mut failures = 0usize;
        for project in self.tracked.clone() {
            let request = client.clone();
            let directory = project.directory.clone();
            let result = with_retry(|| {
                let client = request.clone();
                let directory = directory.clone();
                async move {
                    client
                        .list_sessions_directory(&directory, SESSIONS_PER_PROJECT)
                        .await
                }
            })
            .await;
            match result {
                Ok(page) => {
                    self.cached_sessions
                        .insert(project.directory.clone(), page.data);
                }
                Err(_) => failures += 1,
            }
        }
        let mut sessions = Vec::new();
        let mut seen = HashSet::new();
        for project in &self.tracked {
            let Some(cached) = self.cached_sessions.get(&project.directory) else {
                continue;
            };
            for session in cached {
                if session.time.created + 1_000.0 < project.since {
                    continue;
                }
                if seen.insert(session.id.clone()) {
                    sessions.push(session.clone());
                }
            }
        }
        if failures > 0 {
            self.emit(BackendEvent::Toast {
                kind: ToastKind::Warning,
                message: format!("Could not refresh {failures} project(s)"),
            });
        }
        self.emit(BackendEvent::Sessions(sessions));
    }

    async fn load_mcp(&mut self, client: &Client) {
        self.mcp_attempts = 0;
        if !self.refresh_mcp(client).await {
            self.pending_mcp = true;
            self.arm_flush_in(MCP_SETTLE_DELAY);
        }
    }

    async fn refresh_mcp(&mut self, client: &Client) -> bool {
        let result = with_retry(|| {
            let client = client.clone();
            async move { client.mcp_servers().await }
        })
        .await;
        match result {
            Ok(servers) => {
                let settled =
                    !servers.is_empty() && servers.iter().all(|server| server.is_connected());
                self.emit(BackendEvent::Mcp(servers));
                settled
            }
            Err(ApiError::Status { status: 404, .. }) => true,
            Err(err) => {
                tracing::warn!("Could not load MCP servers: {err}");
                false
            }
        }
    }

    async fn refresh_messages(&mut self, client: &Client, session_id: &str) {
        let result = with_retry(|| {
            let client = client.clone();
            let session_id = session_id.to_string();
            async move { client.messages_page(&session_id, 100).await }
        })
        .await;
        match result {
            Ok(page) => {
                self.cursors
                    .insert(session_id.to_string(), page.cursor.and_then(|c| c.next));
                self.emit(BackendEvent::Messages {
                    session_id: session_id.to_string(),
                    messages: page.data,
                });
            }
            Err(err) => self.error("Could not refresh messages", err),
        }
    }

    fn emit(&self, event: BackendEvent) {
        tracing::debug!(target: "rchamber::event", kind = event_kind(&event), "emit");
        let _ = self.tx.send(event);
        (self.wake)();
    }

    fn error(&self, context: &str, error: ApiError) {
        let message = format!("{context}: {error}");
        tracing::warn!("{message}");
        self.emit(BackendEvent::Toast {
            kind: ToastKind::Error,
            message,
        });
    }
}

fn preview_text(message: &Message) -> String {
    let text = match message {
        Message::User(message) => message.text.clone(),
        Message::Assistant(message) => message
            .content
            .iter()
            .filter_map(|part| match part {
                Part::Text(text) => Some(text.text.trim()),
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > 240 {
        format!("{}…", text.chars().take(240).collect::<String>())
    } else {
        text
    }
}

fn mark_work_open(session: &mut Session) {
    let mut root = match session.metadata.take() {
        Some(Value::Object(metadata)) => metadata,
        _ => Map::new(),
    };
    let openchamber = root
        .entry("openchamber")
        .or_insert_with(|| Value::Object(Map::new()));
    if !openchamber.is_object() {
        *openchamber = Value::Object(Map::new());
    }
    if let Some(openchamber) = openchamber.as_object_mut() {
        openchamber.insert(
            "work".into(),
            serde_json::json!({
                "state": "open",
                "openedAt": crate::state::now_ms(),
                "openedBy": "auto",
            }),
        );
    }
    session.metadata = Some(Value::Object(root));
}

async fn with_retry<T, F, Fut>(mut call: F) -> Result<T, ApiError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiError>>,
{
    let mut attempt = 0u32;
    loop {
        match call().await {
            Ok(value) => return Ok(value),
            Err(error) if error.is_retryable() && attempt < 24 => {
                attempt += 1;
                let delay = Duration::from_millis((150u64 << attempt.min(4)).min(2_000));
                tokio::time::sleep(delay).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn view_session(client: &Client, id: &str) -> Result<Option<f64>, ApiError> {
    let session = client.get_session(id).await?;
    let Some(idle) = session.time.idle else {
        return Ok(None);
    };
    client.view_session(id, idle).await?;
    Ok(Some(idle))
}

fn event_kind(event: &BackendEvent) -> &'static str {
    match event {
        BackendEvent::Connection(_) => "connection",
        BackendEvent::Server(_) => "server",
        BackendEvent::Sessions(_) => "sessions",
        BackendEvent::SessionUpsert(_) => "session-upsert",
        BackendEvent::SessionActivated(_) => "session-activated",
        BackendEvent::SessionRemoved(_) => "session-removed",
        BackendEvent::Messages { .. } => "messages",
        BackendEvent::OlderMessages { .. } => "older-messages",
        BackendEvent::Live(_) => "live",
        BackendEvent::Models(_) => "models",
        BackendEvent::Providers(_) => "providers",
        BackendEvent::Agents(_) => "agents",
        BackendEvent::Commands(_) => "commands",
        BackendEvent::Projects(_) => "projects",
        BackendEvent::SessionMetadataUpdated(_) => "session-metadata-updated",
        BackendEvent::SessionViewed { .. } => "session-viewed",
        BackendEvent::SessionPreview { .. } => "session-preview",
        BackendEvent::Permissions { .. } => "permissions",
        BackendEvent::Diff { .. } => "diff",
        BackendEvent::Vcs(_) => "vcs",
        BackendEvent::Files { .. } => "files",
        BackendEvent::FileContents { .. } => "file-contents",
        BackendEvent::FileSearch { .. } => "file-search",
        BackendEvent::Skills(_) => "skills",
        BackendEvent::Mcp(_) => "mcp",
        BackendEvent::Snippets(_) => "snippets",
        BackendEvent::Inbox { .. } => "inbox",
        BackendEvent::Toast { .. } => "toast",
        BackendEvent::PromptQueued { .. } => "prompt-queued",
        BackendEvent::WorkspaceChanged(_) => "workspace-changed",
    }
}
