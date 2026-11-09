use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rchamber_api::PromptFile;
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
use crate::extensions::{BridgeMessage, ExtensionHost};
use crate::live;
use crate::snippets;
use crate::workspace::paths_match;
use rchamber_ext::{HostCall, HostContext, SessionContext};
use serde_json::json;

pub type Wake = Arc<dyn Fn() + Send + Sync>;

const MCP_SETTLE_ATTEMPTS: u32 = 8;
const MCP_SETTLE_DELAY: Duration = Duration::from_secs(2);
const SESSIONS_PER_PROJECT: u32 = 100;

fn confine_extension_path(
    base: &std::path::Path,
    value: &str,
) -> Result<std::path::PathBuf, String> {
    let base = std::fs::canonicalize(base).map_err(|error| error.to_string())?;
    let requested = std::path::Path::new(value);
    let path = if requested.is_absolute() {
        std::path::PathBuf::from(value)
    } else {
        if requested
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err("the path is outside the open project".into());
        }
        base.join(requested)
    };
    if let Ok(canonical) = std::fs::canonicalize(&path) {
        if !canonical.starts_with(&base) {
            return Err("the path is outside the open project".into());
        }
        return Ok(canonical);
    }
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("the path is a symbolic link".into());
    }
    let parent = path
        .parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
        .ok_or_else(|| "the parent folder does not exist".to_string())?;
    if !parent.starts_with(&base) {
        return Err("the path is outside the open project".into());
    }
    let name = path
        .file_name()
        .ok_or_else(|| "the path has no file name".to_string())?;
    Ok(parent.join(name))
}

fn sanitize_prompt_fragment(value: &str) -> String {
    value.replace("</RustChamber", "< /RustChamber")
}

const UI_BLOCK_GUIDANCE: &str = "RustChamber draws native UI blocks. When a table, chart, form or status reads better than prose, end the answer with a fenced ```openui-lang block containing a program such as:\nroot = Stack([title, rows])\ntitle = Text(\"Deploys\")\nrows = Table([\"Service\", \"State\"], [[\"api\", \"running\"]])\nUse positional arguments only. Components: Stack, Row, Card, Tabs, Text, Heading, Code, Badge, Callout, KeyValue, Table, List, Steps, Stat, Progress, BarChart, LineChart, Choice, Input, DateInput, Slider, Details, Divider, Button. A Choice or Input asks the user and the answer comes back as a message.";

#[derive(Clone)]
pub struct ActionSink(UnboundedSender<Action>);

impl ActionSink {
    pub fn send(&self, action: Action) {
        let _ = self.0.send(action);
    }
}

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

    pub fn sink(&self) -> ActionSink {
        ActionSink(self.actions.clone())
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
    extensions: ExtensionHost,
    next_extension_status: Option<Instant>,
    extension_status_due: HashMap<String, Instant>,
    extension_theme: String,
    extension_locale: String,
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
            extensions: {
                let mut extensions = ExtensionHost::new(rchamber_ext::default_root());
                extensions.set_context(HostContext {
                    theme: "dark".into(),
                    locale: "en".into(),
                    ..HostContext::default()
                });
                extensions.ensure_workers();
                extensions
            },
            next_extension_status: Some(Instant::now() + Duration::from_millis(1_200)),
            extension_status_due: HashMap::new(),
            extension_theme: "dark".into(),
            extension_locale: "en".into(),
        }
    }

    async fn run(&mut self, mut actions: UnboundedReceiver<Action>) {
        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
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
                    Some(event) = self.extensions.events.recv() => {
                        if let Some(event) = self.extensions.worker_event(event) {
                            self.emit(event);
                        }
                    }
                    Some(message) = self.extensions.bridge_rx.recv() => {
                        self.extension_bridge(message).await;
                    }
                    _ = tokio::time::sleep_until(self.next_extension_status.unwrap_or_else(Instant::now)), if self.next_extension_status.is_some() => {
                        self.refresh_extension_status();
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
                self.sync_extension_context();
                self.extensions.ensure_workers();
                self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                self.refresh_extension_status();
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
            self.sync_extension_context();
            if let Some(client) = self.client.clone() {
                self.refresh_sessions(&client).await;
            }
            return Control::Continue;
        }
        if let Some(control) = self.handle_extension_action(&action).await {
            return control;
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
            Action::InstallExtension { .. }
            | Action::InstallExtensionUrl { .. }
            | Action::ApproveExtension { .. }
            | Action::SetExtensionEnabled { .. }
            | Action::RemoveExtension(_)
            | Action::ReloadExtension(_)
            | Action::ExtensionCall { .. }
            | Action::ExtensionMentions { .. }
            | Action::ExtensionBlock { .. }
            | Action::ExtensionCommand { .. }
            | Action::ExtensionAction { .. }
            | Action::ExtensionStatusRefresh
            | Action::SetExtensionAppearance { .. } => {}
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

    async fn handle_extension_action(&mut self, action: &Action) -> Option<Control> {
        match action {
            Action::InstallExtension { path } => {
                let path = std::path::PathBuf::from(path);
                let result = if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
                {
                    self.extensions.install_zip(&path)
                } else {
                    self.extensions.install_path(&path)
                };
                match result {
                    Ok(extension) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.emit(BackendEvent::ExtensionInstalled {
                            extension_id: extension.manifest.id.clone(),
                        });
                        self.emit(BackendEvent::Toast {
                            kind: ToastKind::Success,
                            message: format!(
                                "Added {}. Review its permissions to turn it on.",
                                extension.manifest.name
                            ),
                        });
                        self.sync_extension_context();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("Could not add the extension: {error}"),
                    }),
                }
                Some(Control::Continue)
            }
            Action::InstallExtensionUrl { url } => {
                match self.install_extension_from_url(url).await {
                    Ok(extension) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.emit(BackendEvent::ExtensionInstalled {
                            extension_id: extension.manifest.id.clone(),
                        });
                        self.emit(BackendEvent::Toast {
                            kind: ToastKind::Success,
                            message: format!(
                                "Added {}. Review its permissions to turn it on.",
                                extension.manifest.name
                            ),
                        });
                        self.sync_extension_context();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("Could not add the extension: {error}"),
                    }),
                }
                Some(Control::Continue)
            }
            Action::ApproveExtension { id, grants } => {
                match self.extensions.approve(id, grants.clone()) {
                    Ok(()) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.sync_extension_context();
                        self.refresh_extension_status();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: error,
                    }),
                }
                Some(Control::Continue)
            }
            Action::SetExtensionEnabled { id, enabled } => {
                match self.extensions.set_enabled(id, *enabled) {
                    Ok(()) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.sync_extension_context();
                        self.refresh_extension_status();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: error,
                    }),
                }
                Some(Control::Continue)
            }
            Action::RemoveExtension(id) => {
                match self.extensions.remove(id) {
                    Ok(()) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.emit(BackendEvent::Toast {
                            kind: ToastKind::Info,
                            message: "Extension removed".into(),
                        });
                        self.sync_extension_context();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: error,
                    }),
                }
                Some(Control::Continue)
            }
            Action::ReloadExtension(id) => {
                match self.extensions.reload(id) {
                    Ok(()) => {
                        self.emit(BackendEvent::Extensions(self.extensions.catalog()));
                        self.sync_extension_context();
                        self.refresh_extension_status();
                    }
                    Err(error) => self.emit(BackendEvent::Toast {
                        kind: ToastKind::Error,
                        message: error,
                    }),
                }
                Some(Control::Continue)
            }
            Action::ExtensionCall {
                id,
                call_id,
                method,
                args,
            } => {
                self.sync_extension_context();
                if !self.extensions.call(id, call_id, method, args.clone()) {
                    self.emit(BackendEvent::ExtensionError {
                        call_id: call_id.clone(),
                        extension_id: id.clone(),
                        method: method.clone(),
                        message: "the extension is not running".into(),
                    });
                }
                Some(Control::Continue)
            }
            Action::ExtensionMentions { query } => {
                self.sync_extension_context();
                self.extensions.request_mentions(query);
                Some(Control::Continue)
            }
            Action::ExtensionBlock { name, args } => {
                self.sync_extension_context();
                let _ = self.extensions.request_block(name, args);
                Some(Control::Continue)
            }
            Action::ExtensionCommand { id, name, args } => {
                self.sync_extension_context();
                self.extensions.call(
                    id,
                    &format!("command:{name}"),
                    "command",
                    vec![json!(name), json!(args)],
                );
                Some(Control::Continue)
            }
            Action::ExtensionAction { id, action, item } => {
                self.sync_extension_context();
                self.extensions.call(
                    id,
                    &format!("action:{action}"),
                    "action",
                    vec![json!(action), item.clone()],
                );
                Some(Control::Continue)
            }
            Action::ExtensionStatusRefresh => {
                self.refresh_extension_status();
                Some(Control::Continue)
            }
            Action::SetExtensionAppearance { theme, locale } => {
                self.extension_theme = theme.clone();
                self.extension_locale = locale.clone();
                self.sync_extension_context();
                Some(Control::Continue)
            }
            _ => None,
        }
    }

    async fn install_extension_from_url(
        &mut self,
        url: &str,
    ) -> Result<rchamber_ext::InstalledExtension, String> {
        let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
        if parsed.scheme() != "https" {
            return Err("extension downloads must use https".into());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > 40 * 1024 * 1024)
        {
            return Err("the archive is larger than 40 MiB".into());
        }
        let path = std::env::temp_dir().join(format!(
            "rchamber-extension-{}-{}.zip",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
            .map_err(|error| error.to_string())?;
        let mut written = 0usize;
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            written = written.saturating_add(chunk.len());
            if written > 40 * 1024 * 1024 {
                let _ = tokio::fs::remove_file(&path).await;
                return Err("the archive is larger than 40 MiB".into());
            }
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .map_err(|error| error.to_string())?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file)
            .await
            .map_err(|error| error.to_string())?;
        drop(file);
        let result = self.extensions.install_zip(&path);
        let _ = tokio::fs::remove_file(&path).await;
        result
    }

    fn sync_extension_context(&mut self) {
        let session = self.active.as_ref().and_then(|id| {
            self.cached_sessions
                .values()
                .flatten()
                .find(|session| &session.id == id)
                .map(|session| SessionContext {
                    id: session.id.clone(),
                    title: session.title().to_string(),
                    directory: session.directory().map(str::to_string),
                })
        });
        self.extensions.set_context(HostContext {
            project: self.config.workspace.clone(),
            session,
            theme: self.extension_theme.clone(),
            locale: self.extension_locale.clone(),
            workspace: self.config.workspace.clone(),
        });
        self.extensions.ensure_workers();
    }

    fn refresh_extension_status(&mut self) {
        self.sync_extension_context();
        let statuses = self.extensions.status_extensions();
        let now = Instant::now();
        self.extension_status_due
            .retain(|id, _| statuses.iter().any(|(candidate, _, _)| candidate == id));
        for (id, method, interval) in &statuses {
            let due = self.extension_status_due.entry(id.clone()).or_insert(now);
            if now >= *due {
                self.extensions.call(id, "status", method, Vec::new());
                *due = now + Duration::from_millis(*interval);
            }
        }
        let next = self
            .extension_status_due
            .values()
            .min()
            .copied()
            .unwrap_or(now + Duration::from_secs(30));
        self.next_extension_status = Some(next.max(now + Duration::from_millis(250)));
    }

    async fn extension_bridge(&mut self, message: BridgeMessage) {
        match message {
            BridgeMessage::Call {
                extension_id,
                call,
                reply,
            } => {
                let answer = if self.extensions.is_active(&extension_id) {
                    self.host_call(&extension_id, call).await
                } else {
                    Err("the extension is no longer active".into())
                };
                let _ = reply.send(answer);
            }
            BridgeMessage::Effect {
                extension_id,
                effect,
            } => {
                if self.extensions.is_active(&extension_id) {
                    self.host_effect(&extension_id, effect).await;
                }
            }
        }
    }

    async fn host_call(
        &mut self,
        _extension_id: &str,
        call: HostCall,
    ) -> Result<serde_json::Value, String> {
        match call {
            HostCall::Http {
                method,
                url,
                headers,
                body,
            } => {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(20))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|error| error.to_string())?;
                let method = reqwest::Method::from_bytes(method.as_bytes())
                    .map_err(|error| error.to_string())?;
                let mut request = client.request(method, &url);
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                if let Some(body) = body {
                    request = request.body(body);
                }
                let mut response = request.send().await.map_err(|error| error.to_string())?;
                let status = response.status().as_u16();
                if response
                    .content_length()
                    .is_some_and(|length| length > 2 * 1024 * 1024)
                {
                    return Err("the response is larger than 2 MiB".into());
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
                    bytes.extend_from_slice(&chunk);
                    if bytes.len() > 2 * 1024 * 1024 {
                        return Err("the response is larger than 2 MiB".into());
                    }
                }
                Ok(json!({
                    "status": status,
                    "body": String::from_utf8_lossy(&bytes),
                }))
            }
            HostCall::ReadFile { path } => {
                let path = self.extension_path(&path)?;
                let mut file = tokio::fs::File::open(&path)
                    .await
                    .map_err(|error| error.to_string())?;
                let mut bytes = Vec::new();
                tokio::io::AsyncReadExt::read_to_end(
                    &mut tokio::io::AsyncReadExt::take(&mut file, 2 * 1024 * 1024 + 1),
                    &mut bytes,
                )
                .await
                .map_err(|error| error.to_string())?;
                if bytes.len() > 2 * 1024 * 1024 {
                    return Err("the file is larger than 2 MiB".into());
                }
                Ok(json!({
                    "content": String::from_utf8_lossy(&bytes),
                    "path": path.to_string_lossy(),
                }))
            }
            HostCall::WriteFile { path, content } => {
                let path = self.extension_path(&path)?;
                if content.len() > 1024 * 1024 {
                    return Err("the content is larger than 1 MiB".into());
                }
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                tokio::fs::write(&path, content)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(json!({ "path": path.to_string_lossy() }))
            }
            HostCall::ListDir { path } => {
                let path = self.extension_path(&path)?;
                let mut entries = Vec::new();
                let mut reader = tokio::fs::read_dir(&path)
                    .await
                    .map_err(|error| error.to_string())?;
                while let Some(entry) = reader
                    .next_entry()
                    .await
                    .map_err(|error| error.to_string())?
                {
                    if entries.len() >= 500 {
                        break;
                    }
                    let kind = entry.file_type().await.ok().map(|kind| {
                        if kind.is_dir() {
                            "directory"
                        } else if kind.is_file() {
                            "file"
                        } else {
                            "other"
                        }
                    });
                    entries.push(json!({
                        "name": entry.file_name().to_string_lossy(),
                        "kind": kind.unwrap_or("other"),
                    }));
                }
                Ok(serde_json::Value::Array(entries))
            }
            HostCall::Stat { path } => {
                let path = self.extension_path(&path)?;
                match tokio::fs::metadata(&path).await {
                    Ok(metadata) => Ok(json!({
                        "kind": if metadata.is_dir() { "directory" } else if metadata.is_file() { "file" } else { "other" },
                        "size": metadata.len(),
                    })),
                    Err(_) => Ok(json!({ "kind": "missing" })),
                }
            }
            HostCall::SessionsList => {
                let Some(client) = self.client.clone() else {
                    return Err("not connected to OpenCode".into());
                };
                let sessions = client
                    .list_sessions(200)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(serde_json::Value::Array(
                    sessions
                        .into_iter()
                        .map(|session| {
                            json!({
                                "id": session.id,
                                "title": session.title(),
                                "directory": session.directory(),
                            })
                        })
                        .collect(),
                ))
            }
            HostCall::SessionStart { project, title } => {
                let Some(client) = self.client.clone() else {
                    return Err("not connected to OpenCode".into());
                };
                let directory = project
                    .or_else(|| self.config.workspace.clone())
                    .ok_or_else(|| "no project is open".to_string())?;
                if !self.tracked.is_empty()
                    && !self
                        .tracked
                        .iter()
                        .any(|tracked| paths_match(&tracked.directory, &directory))
                {
                    return Err("the project is not tracked by RustChamber".into());
                }
                let session = client
                    .create_session(&directory, title.as_deref(), None, None, None)
                    .await
                    .map_err(|error| error.to_string())?;
                self.emit(BackendEvent::SessionUpsert(Box::new(session.clone())));
                Ok(json!({
                    "id": session.id,
                    "title": session.title(),
                }))
            }
            HostCall::SessionOpen { id } => {
                self.emit(BackendEvent::ExtensionOpenSession(id));
                Ok(json!({}))
            }
            HostCall::Prompt { session, text } => {
                let Some(client) = self.client.clone() else {
                    return Err("not connected to OpenCode".into());
                };
                let Some(session) = session.or_else(|| self.active.clone()) else {
                    return Err("no session is open".into());
                };
                self.send_prompt(&client, &session, &text, &[], "steer")
                    .await;
                Ok(json!({}))
            }
            HostCall::Generate { .. } => {
                Err("text generation with the small model is not available in this build".into())
            }
            HostCall::Exec { command, args, cwd } => {
                let cwd = match cwd {
                    Some(cwd) => self.extension_path(&cwd)?,
                    None => std::path::PathBuf::from(
                        self.config
                            .workspace
                            .clone()
                            .ok_or_else(|| "no project is open".to_string())?,
                    ),
                };
                let mut process = tokio::process::Command::new(&command);
                process
                    .args(&args)
                    .current_dir(&cwd)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);
                let mut child = process.spawn().map_err(|error| error.to_string())?;
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let run = async {
                    let read = |pipe: Option<tokio::process::ChildStdout>, limit: usize| async move {
                        let Some(mut pipe) = pipe else {
                            return Ok::<Vec<u8>, String>(Vec::new());
                        };
                        let mut bytes = Vec::new();
                        tokio::io::AsyncReadExt::read_to_end(
                            &mut tokio::io::AsyncReadExt::take(&mut pipe, limit as u64 + 1),
                            &mut bytes,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                        Ok(bytes)
                    };
                    let err_read = async {
                        let Some(mut pipe) = stderr else {
                            return Ok::<Vec<u8>, String>(Vec::new());
                        };
                        let mut bytes = Vec::new();
                        tokio::io::AsyncReadExt::read_to_end(
                            &mut tokio::io::AsyncReadExt::take(&mut pipe, 64 * 1024 + 1),
                            &mut bytes,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                        Ok(bytes)
                    };
                    let (stdout, stderr, status) =
                        tokio::join!(read(stdout, 256 * 1024), err_read, child.wait());
                    Ok::<_, String>((stdout?, stderr?, status.map_err(|e| e.to_string())?))
                };
                let (stdout, stderr, status) = tokio::time::timeout(Duration::from_secs(20), run)
                    .await
                    .map_err(|_| "the command timed out".to_string())?
                    .map_err(|error: String| error)?;
                let mut stdout = String::from_utf8_lossy(&stdout).to_string();
                let mut stderr = String::from_utf8_lossy(&stderr).to_string();
                if stdout.len() > 256 * 1024 {
                    stdout.truncate(256 * 1024);
                    stdout.push_str("\n… truncated");
                }
                if stderr.len() > 64 * 1024 {
                    stderr.truncate(64 * 1024);
                    stderr.push_str("\n… truncated");
                }
                Ok(json!({
                    "status": status.code(),
                    "stdout": stdout,
                    "stderr": stderr,
                }))
            }
        }
    }

    fn extension_path(&self, value: &str) -> Result<std::path::PathBuf, String> {
        let base = self
            .config
            .workspace
            .clone()
            .ok_or_else(|| "no project is open".to_string())?;
        confine_extension_path(std::path::Path::new(&base), value)
    }

    async fn host_effect(&mut self, extension_id: &str, effect: rchamber_ext::HostEffect) {
        use rchamber_ext::HostEffect;
        let allows = |capability: &str| self.extensions.capability(extension_id, capability);
        match effect {
            HostEffect::Toast { kind, message } => {
                let kind = match kind.as_str() {
                    "success" => ToastKind::Success,
                    "warning" => ToastKind::Warning,
                    "error" => ToastKind::Error,
                    _ => ToastKind::Info,
                };
                self.emit(BackendEvent::Toast { kind, message });
            }
            HostEffect::Compose { text, send } => {
                if send && !allows("prompt") {
                    return;
                }
                if send {
                    if let (Some(client), Some(session)) =
                        (self.client.clone(), self.active.clone())
                    {
                        self.send_prompt(&client, &session, &text, &[], "steer")
                            .await;
                    }
                } else {
                    self.emit(BackendEvent::ExtensionCompose { text, send: false });
                }
            }
            HostEffect::Prompt { text, send } => {
                if !allows("prompt") {
                    return;
                }
                if send {
                    if let (Some(client), Some(session)) =
                        (self.client.clone(), self.active.clone())
                    {
                        self.send_prompt(&client, &session, &text, &[], "steer")
                            .await;
                    }
                } else {
                    self.emit(BackendEvent::ExtensionCompose { text, send: false });
                }
            }
            HostEffect::Attach {
                provider_id,
                id,
                title,
                url,
            } => {
                if !allows("prompt") {
                    return;
                }
                self.emit(BackendEvent::ExtensionAttach {
                    provider_id,
                    id,
                    title,
                    url,
                })
            }
            HostEffect::SetBadge { count } => {
                self.emit(BackendEvent::ExtensionBadge {
                    id: extension_id.to_string(),
                    count,
                });
            }
            HostEffect::Answer { text } => {
                if !allows("prompt") {
                    return;
                }
                if let (Some(client), Some(session)) = (self.client.clone(), self.active.clone()) {
                    self.send_prompt(&client, &session, &text, &[], "steer")
                        .await;
                }
            }
            HostEffect::OpenUrl { url } => {
                if !allows("prompt") {
                    return;
                }
                self.emit(BackendEvent::ExtensionOpenUrl(url));
            }
            HostEffect::Notify { title, body } => {
                if !allows("notifications") {
                    return;
                }
                self.emit(BackendEvent::Toast {
                    kind: ToastKind::Info,
                    message: format!("{title}: {body}"),
                });
            }
            HostEffect::Clipboard(text) => {
                if !allows("clipboard") {
                    return;
                }
                self.emit(BackendEvent::ExtensionClipboard(text));
            }
        }
    }

    fn expand_snippets(&self, text: &str, client: &Client) -> String {
        if self.snippets.is_empty() {
            snippets::expand(text, client.directory())
        } else {
            snippets::expand_with(text, &self.snippets)
        }
    }

    fn write_prompt_context(&self, session_id: &str, context: &str) -> Option<std::path::PathBuf> {
        let base = directories::BaseDirs::new()
            .map(|base| base.config_dir().join("rustchamber").join("context"))
            .unwrap_or_else(|| std::env::temp_dir().join("rustchamber-context"));
        std::fs::create_dir_all(&base).ok()?;
        let safe: String = session_id
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
            .collect();
        let path = base.join(format!("{safe}.md"));
        std::fs::write(&path, context).ok()?;
        Some(path)
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
        let active = self.extensions.active();
        let blocks: Vec<String> = active
            .iter()
            .flat_map(|extension| {
                extension.manifest.contributes.blocks.iter().map(|block| {
                    format!(
                        "{}({}): {}",
                        block.name,
                        block.args.join(", "),
                        block.description
                    )
                })
            })
            .collect();
        let instructions: Vec<String> = active
            .into_iter()
            .filter_map(|extension| {
                let text = extension.manifest.contributes.instructions.clone()?;
                Some(format!(
                    "From {}: {}",
                    sanitize_prompt_fragment(&extension.manifest.name),
                    sanitize_prompt_fragment(&text)
                ))
            })
            .collect();
        let mut context = String::new();
        if !blocks.is_empty() || !instructions.is_empty() {
            context.push_str(UI_BLOCK_GUIDANCE);
            if !blocks.is_empty() {
                context.push_str("\nExtension components:\n");
                context.push_str(&blocks.join("\n"));
            }
            if !instructions.is_empty() {
                context.push_str("\n\nExtension instructions:\n");
                context.push_str(&instructions.join("\n\n"));
            }
        }
        let mut files = files.to_vec();
        if !context.trim().is_empty()
            && let Some(path) = self.write_prompt_context(session_id, &context)
            && let Some(file) = PromptFile::from_path(
                &path,
                Some("RustChamber context".into()),
                "text/markdown",
                Some("Context for this turn.".into()),
            )
        {
            files.push(file);
        }
        let text = expanded.as_str();
        match client
            .prompt_full(session_id, text, &files, Some(delivery))
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
                if let Some(directory) = session.directory().map(str::to_string) {
                    let sessions = self.cached_sessions.entry(directory).or_default();
                    if let Some(existing) = sessions
                        .iter_mut()
                        .find(|existing| existing.id == session.id)
                    {
                        *existing = session.clone();
                    } else {
                        sessions.push(session.clone());
                    }
                }
                self.active = Some(id.clone());
                self.sync_extension_context();
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
        if let Some(location) = &raw.location
            && !self
                .tracked
                .iter()
                .any(|project| paths_match(&project.directory, &location.directory))
        {
            return;
        }
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
        match &event {
            LiveEvent::Execution {
                session_id,
                state,
                error,
            } if *state != ExecutionState::Started => {
                let outcome = match state {
                    ExecutionState::Succeeded => "completed",
                    ExecutionState::Failed => "failed",
                    ExecutionState::Interrupted => "cancelled",
                    ExecutionState::Started => "started",
                };
                self.sync_extension_context();
                self.extensions.broadcast_event(
                    "turn.finished",
                    json!({
                        "session": session_id,
                        "directory": self.config.workspace,
                        "outcome": outcome,
                        "error": error.as_ref().map(|error| error.message.clone()),
                    }),
                );
            }
            LiveEvent::PermissionAsked(request) => {
                self.sync_extension_context();
                self.extensions.broadcast_event(
                    "permission.asked",
                    json!({
                        "session": request.session_id,
                        "directory": self.config.workspace,
                        "tool": request.action,
                        "title": request.message,
                    }),
                );
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
        BackendEvent::Extensions(_) => "extensions",
        BackendEvent::ExtensionInstalled { .. } => "extension-installed",
        BackendEvent::ExtensionResult { .. } => "extension-result",
        BackendEvent::ExtensionBlock { .. } => "extension-block",
        BackendEvent::ExtensionMentions { .. } => "extension-mentions",
        BackendEvent::ExtensionError { .. } => "extension-error",
        BackendEvent::ExtensionCompose { .. } => "extension-compose",
        BackendEvent::ExtensionAttach { .. } => "extension-attach",
        BackendEvent::ExtensionBadge { .. } => "extension-badge",
        BackendEvent::ExtensionOpenSession(_) => "extension-open-session",
        BackendEvent::ExtensionClipboard(_) => "extension-clipboard",
        BackendEvent::ExtensionOpenUrl(_) => "extension-open-url",
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.extensions.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confines_extension_paths_to_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        assert!(confine_extension_path(base, "a.txt").is_ok());
        assert!(confine_extension_path(base, "../a.txt").is_err());
        assert!(confine_extension_path(base, "/etc/hosts").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_dangling_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("payload");
        std::os::unix::fs::symlink("/tmp/definitely-missing-rchamber-target", &link).unwrap();
        assert!(confine_extension_path(dir.path(), "payload").is_err());
    }
}
