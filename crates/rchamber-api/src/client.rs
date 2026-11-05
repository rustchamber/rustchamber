use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::models::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptFile {
    #[serde(default)]
    pub data: String,
    pub mime: String,
    pub source: PromptFileSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention: Option<PromptMention>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PromptFileSource {
    Inline,
    Uri { uri: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptMention {
    pub start: i64,
    pub end: i64,
    pub text: String,
}

impl PromptFile {
    pub fn path(
        uri: impl Into<String>,
        name: Option<String>,
        mime: impl Into<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            data: String::new(),
            mime: mime.into(),
            source: PromptFileSource::Uri { uri: uri.into() },
            name,
            description,
            mention: None,
        }
    }

    pub fn inline(
        data: String,
        mime: impl Into<String>,
        name: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            data,
            mime: mime.into(),
            source: PromptFileSource::Inline,
            name,
            description,
            mention: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base_url: Url,
    pub username: String,
    pub password: Option<String>,
    pub directory: Option<String>,
}

impl ClientConfig {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, ApiError> {
        let base_url = Url::parse(base_url.as_ref())
            .map_err(|e| ApiError::Config(format!("invalid server url: {e}")))?;
        Ok(Self::from_url(base_url))
    }

    pub fn from_url(base_url: Url) -> Self {
        Self {
            base_url,
            username: "opencode".into(),
            password: None,
            directory: None,
        }
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn with_directory(mut self, directory: impl Into<String>) -> Self {
        self.directory = Some(directory.into());
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("could not decode response: {0}")]
    Decode(String),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("{0}")]
    Other(String),
}

impl ApiError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, ApiError::Status { status: 401, .. })
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            ApiError::Status { status, .. } => matches!(status, 502 | 503),
            ApiError::Http(error) => error.is_connect() || error.is_timeout() || error.is_request(),
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct Client {
    base: Url,
    username: String,
    password: Option<String>,
    directory: Option<String>,
    http: reqwest::Client,
    stream: reqwest::Client,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("rchamber/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let stream = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .user_agent(concat!("rchamber/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base: config.base_url,
            username: config.username,
            password: config.password,
            directory: config.directory,
            http,
            stream,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    pub fn directory(&self) -> Option<&str> {
        self.directory.as_deref()
    }

    pub fn set_directory(&mut self, directory: Option<String>) {
        self.directory = directory;
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!(
            "{}/{}",
            self.base.as_str().trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let mut req = self.http.request(method, url);
        if let Some(password) = &self.password {
            req = req.basic_auth(&self.username, Some(password));
        }
        if let Some(dir) = &self.directory {
            req = req.header("x-opencode-directory", dir);
        }
        req
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T, ApiError> {
        let response = req.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &body));
        }
        serde_json::from_str(&body)
            .map_err(|e| ApiError::Decode(format!("{e}: {}", truncate(&body, 400))))
    }

    async fn send_optional(&self, req: reqwest::RequestBuilder) -> Result<Option<Value>, ApiError> {
        let response = req.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &body));
        }
        decode_optional_body(&body)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.send(self.request(reqwest::Method::GET, path)).await
    }

    async fn get_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        self.send(self.request(reqwest::Method::GET, path).query(query))
            .await
    }

    async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.send(self.request(reqwest::Method::POST, path).json(body))
            .await
    }

    async fn post_optional<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Option<Value>, ApiError> {
        self.send_optional(self.request(reqwest::Method::POST, path).json(body))
            .await
    }

    async fn patch_optional<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Option<Value>, ApiError> {
        self.send_optional(self.request(reqwest::Method::PATCH, path).json(body))
            .await
    }

    async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let response = self.request(reqwest::Method::DELETE, path).send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(parse_error(status.as_u16(), &body))
    }

    pub async fn info(&self) -> Result<ServerInfo, ApiError> {
        self.get("/api/info").await
    }

    fn session_query(&self, limit: u32, search: Option<&str>) -> Vec<(&'static str, String)> {
        let mut query = vec![
            ("limit", limit.to_string()),
            ("order", "desc".to_string()),
            ("parentID", "null".to_string()),
            ("archived", "false".to_string()),
        ];
        if let Some(directory) = &self.directory {
            query.push(("directory", directory.clone()));
        }
        if let Some(search) = search {
            query.push(("search", search.to_string()));
        }
        query
    }

    pub async fn list_sessions(&self, limit: u32) -> Result<Vec<Session>, ApiError> {
        let query = self.session_query(limit, None);
        let response: SessionList = self.get_query("/api/session", &query).await?;
        Ok(response.data)
    }

    pub async fn search_sessions(&self, query: &str) -> Result<Vec<Session>, ApiError> {
        let params = self.session_query(60, Some(query));
        let response: SessionList = self.get_query("/api/session", &params).await?;
        Ok(response.data)
    }

    pub async fn get_session(&self, id: &str) -> Result<Session, ApiError> {
        let envelope: Data<Session> = self.get(&format!("/api/session/{id}")).await?;
        Ok(envelope.data)
    }

    pub async fn create_session(
        &self,
        directory: &str,
        title: Option<&str>,
        agent: Option<&str>,
        model: Option<&ModelRef>,
    ) -> Result<Session, ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            title: Option<&'a str>,
            agent: Option<&'a str>,
            model: Option<&'a ModelRef>,
            location: LocationRef,
        }
        let envelope: Data<Session> = self
            .post(
                "/api/session",
                &Body {
                    title,
                    agent,
                    model,
                    location: LocationRef {
                        directory: directory.to_string(),
                    },
                },
            )
            .await?;
        Ok(envelope.data)
    }

    pub async fn rename_session(&self, id: &str, title: &str) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            title: &'a str,
        }
        self.patch_optional(&format!("/api/session/{id}"), &Body { title })
            .await?;
        Ok(())
    }

    pub async fn archive_session(&self, id: &str, archived: bool) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body {
            archived: bool,
        }
        self.patch_optional(&format!("/api/session/{id}"), &Body { archived })
            .await?;
        Ok(())
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/session/{id}")).await
    }

    pub async fn list_messages(&self, session: &str, limit: u32) -> Result<Vec<Message>, ApiError> {
        Ok(self.messages_page(session, limit).await?.data)
    }

    pub async fn messages_page(&self, session: &str, limit: u32) -> Result<MessageList, ApiError> {
        let mut page: MessageList = self
            .get_query(
                &format!("/api/session/{session}/message"),
                &[("limit", limit.to_string()), ("order", "desc".into())],
            )
            .await?;
        page.data.reverse();
        Ok(page)
    }

    pub async fn list_messages_before(
        &self,
        session: &str,
        limit: u32,
        cursor: &str,
    ) -> Result<(Vec<Message>, Option<String>), ApiError> {
        let mut response: MessageList = self
            .get_query(
                &format!("/api/session/{session}/message"),
                &[("limit", limit.to_string()), ("cursor", cursor.to_string())],
            )
            .await?;
        response.data.reverse();
        Ok((response.data, response.cursor.and_then(|c| c.next)))
    }

    pub async fn prompt(&self, session: &str, text: &str) -> Result<Value, ApiError> {
        self.prompt_full(session, text, &[], None).await
    }

    pub async fn prompt_full(
        &self,
        session: &str,
        text: &str,
        files: &[PromptFile],
        delivery: Option<&str>,
    ) -> Result<Value, ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            text: &'a str,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            files: Vec<&'a PromptFile>,
            #[serde(skip_serializing_if = "Option::is_none")]
            delivery: Option<&'a str>,
        }
        self.post(
            &format!("/api/session/{session}/prompt"),
            &Body {
                text,
                files: files.iter().collect(),
                delivery,
            },
        )
        .await
    }

    pub async fn prompt_with_id(
        &self,
        session: &str,
        id: &str,
        text: &str,
    ) -> Result<Value, ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            id: &'a str,
            text: &'a str,
        }
        self.post(
            &format!("/api/session/{session}/prompt"),
            &Body { id, text },
        )
        .await
    }

    pub async fn interrupt(&self, session: &str) -> Result<(), ApiError> {
        self.send_optional(self.request(
            reqwest::Method::POST,
            &format!("/api/session/{session}/interrupt"),
        ))
        .await?;
        Ok(())
    }

    pub async fn switch_model(&self, session: &str, model: &ModelRef) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a ModelRef,
        }
        self.post_optional(&format!("/api/session/{session}/model"), &Body { model })
            .await?;
        Ok(())
    }

    pub async fn switch_agent(&self, session: &str, agent: &str) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            agent: &'a str,
        }
        self.post_optional(&format!("/api/session/{session}/agent"), &Body { agent })
            .await?;
        Ok(())
    }

    pub async fn reply_permission(
        &self,
        session: &str,
        request: &str,
        decision: &str,
        message: Option<&str>,
    ) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            decision: &'a str,
            message: Option<&'a str>,
        }
        self.post_optional(
            &format!("/api/session/{session}/permission/{request}/reply"),
            &Body { decision, message },
        )
        .await?;
        Ok(())
    }

    pub async fn pending_permissions(
        &self,
        session: &str,
    ) -> Result<Vec<PermissionRequest>, ApiError> {
        let envelope: Data<Vec<PermissionRequest>> = self
            .get(&format!("/api/session/{session}/permission"))
            .await?;
        Ok(envelope.data)
    }

    pub async fn inbox(&self, session: &str) -> Result<Vec<Value>, ApiError> {
        let envelope: Data<Vec<Value>> = self.get(&format!("/api/session/{session}/inbox")).await?;
        Ok(envelope.data)
    }

    pub async fn cancel_inbox(&self, session: &str, inbox: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/session/{session}/inbox/{inbox}"))
            .await
    }

    pub async fn set_inbox_delivery(
        &self,
        session: &str,
        inbox: &str,
        delivery: &str,
    ) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            delivery: &'a str,
        }
        self.patch_optional(
            &format!("/api/session/{session}/inbox/{inbox}"),
            &Body { delivery },
        )
        .await?;
        Ok(())
    }

    pub async fn models(&self) -> Result<Vec<Model>, ApiError> {
        let envelope: Data<Vec<Model>> = self.get("/api/model").await?;
        Ok(envelope.data)
    }

    pub async fn providers(&self) -> Result<Vec<Provider>, ApiError> {
        let envelope: Data<Vec<Provider>> = self.get("/api/provider").await?;
        Ok(envelope.data)
    }

    pub async fn agents(&self) -> Result<Vec<Agent>, ApiError> {
        let envelope: Data<Vec<Agent>> = self.get("/api/agent").await?;
        Ok(envelope.data)
    }

    pub async fn commands(&self) -> Result<Vec<Command>, ApiError> {
        let envelope: Data<Vec<Command>> = self.get("/api/command").await?;
        Ok(envelope.data)
    }

    pub async fn skills(&self) -> Result<Vec<Skill>, ApiError> {
        let envelope: Data<Vec<Skill>> = self.get("/api/skill").await?;
        Ok(envelope.data)
    }

    pub async fn mcp_servers(&self) -> Result<Vec<McpServer>, ApiError> {
        let envelope: Data<Vec<McpServer>> = self.get("/api/mcp").await?;
        Ok(envelope.data)
    }

    pub async fn projects(&self) -> Result<Vec<Project>, ApiError> {
        self.get("/api/project").await
    }

    pub async fn vcs_info(&self) -> Result<VcsInfo, ApiError> {
        let envelope: Data<VcsInfo> = self.get("/api/vcs").await?;
        Ok(envelope.data)
    }

    pub async fn vcs_status(&self) -> Result<Vec<VcsFileStatus>, ApiError> {
        let envelope: Data<Vec<VcsFileStatus>> = self.get("/api/vcs/status").await?;
        Ok(envelope.data)
    }

    pub async fn vcs_diff(&self, mode: &str) -> Result<Vec<FileDiff>, ApiError> {
        let envelope: Data<Vec<FileDiff>> = self
            .get_query("/api/vcs/diff", &[("mode", mode.to_string())])
            .await?;
        Ok(envelope.data)
    }

    pub async fn session_diff(&self, session: &str) -> Result<Vec<FileDiff>, ApiError> {
        let envelope: Data<Vec<FileDiff>> =
            self.get(&format!("/api/session/{session}/diff")).await?;
        Ok(envelope.data)
    }

    pub async fn fs_list(&self, path: &str) -> Result<Vec<FileEntry>, ApiError> {
        let envelope: Data<Vec<FileEntry>> = self
            .get_query("/api/fs/list", &[("path", path.to_string())])
            .await?;
        Ok(envelope.data)
    }

    pub async fn fs_find(&self, query: &str, limit: u32) -> Result<Vec<FileEntry>, ApiError> {
        let envelope: Data<Vec<FileEntry>> = self
            .get_query(
                "/api/fs/find",
                &[("query", query.to_string()), ("limit", limit.to_string())],
            )
            .await?;
        Ok(envelope.data)
    }

    pub async fn fs_read(&self, path: &str) -> Result<Vec<u8>, ApiError> {
        let encoded = path
            .split('/')
            .map(urlencoding)
            .collect::<Vec<_>>()
            .join("/");
        let response = self
            .request(reqwest::Method::GET, &format!("/api/fs/read/{encoded}"))
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(parse_error(status, &body));
        }
        Ok(response.bytes().await?.to_vec())
    }

    pub fn subscribe(&self, types: Option<Vec<String>>) -> Subscription {
        let client = self.clone();
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::unbounded_channel();
        let token = cancel.clone();
        tokio::spawn(async move {
            let mut attempt: u32 = 0;
            loop {
                if token.is_cancelled() {
                    break;
                }
                match client.event_stream(&types, &tx, &token).await {
                    Ok(()) => {
                        attempt = 0;
                    }
                    Err(err) => {
                        if token.is_cancelled() {
                            break;
                        }
                        let _ = tx.send(RawEvent::disconnected(err.to_string()));
                        attempt = attempt.saturating_add(1);
                        let delay = Duration::from_millis((250u64 << attempt.min(5)).min(8_000));
                        tokio::select! {
                            _ = token.cancelled() => break,
                            _ = tokio::time::sleep(delay) => {}
                        }
                        continue;
                    }
                }
            }
        });
        Subscription { rx, cancel }
    }

    async fn event_stream(
        &self,
        types: &Option<Vec<String>>,
        tx: &mpsc::UnboundedSender<RawEvent>,
        cancel: &CancellationToken,
    ) -> Result<(), ApiError> {
        let mut req = self
            .stream
            .request(reqwest::Method::GET, self.url("/api/event"))
            .header("accept", "text/event-stream")
            .header("accept-encoding", "identity");
        if let Some(password) = &self.password {
            req = req.basic_auth(&self.username, Some(password));
        }
        if let Some(dir) = &self.directory {
            req = req.header("x-opencode-directory", dir);
        }
        if let Some(types) = types
            && !types.is_empty()
        {
            req = req.query(&[("types", types.join(","))]);
        }
        let response = req.send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(parse_error(status.as_u16(), &body));
        }
        let _ = tx.send(RawEvent::connected());
        let mut stream = response.bytes_stream();
        let mut parser = SseParser::default();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                chunk = tokio::time::timeout(Duration::from_secs(45), stream.next()) => {
                    match chunk {
                        Err(_) => return Err(ApiError::Other("event stream went idle".into())),
                        Ok(Some(Ok(bytes))) => {
                            for frame in parser.push(&bytes) {
                                if let Some(event) = RawEvent::parse(&frame)
                                    && tx.send(event).is_err() {
                                        return Ok(());
                                    }
                            }
                        }
                        Ok(Some(Err(err))) => return Err(ApiError::Http(err)),
                        Ok(None) => return Ok(()),
                    }
                }
            }
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base.as_str().trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub async fn health(&self) -> bool {
        self.info().await.is_ok()
    }
}

fn urlencoding(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

fn decode_optional_body(body: &str) -> Result<Option<Value>, ApiError> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(body)
        .map(Some)
        .map_err(|e| ApiError::Decode(format!("{e}: {}", truncate(body, 400))))
}

fn parse_error(status: u16, body: &str) -> ApiError {
    #[derive(Deserialize)]
    struct Tagged {
        #[serde(rename = "_tag")]
        tag: Option<String>,
        message: Option<String>,
    }
    if let Ok(tagged) = serde_json::from_str::<Tagged>(body) {
        let message = tagged
            .message
            .or(tagged.tag)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return ApiError::Status { status, message };
    }
    ApiError::Status {
        status,
        message: truncate(body, 300).to_string(),
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionList {
    #[serde(default)]
    pub data: Vec<Session>,
    pub cursor: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Data<T> {
    data: T,
}

#[derive(Clone, Debug)]
pub struct RawEvent {
    pub id: String,
    pub kind: String,
    pub data: Value,
    pub location: Option<LocationRef>,
    pub system: Option<SystemEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    Connected,
    Disconnected,
}

impl RawEvent {
    pub fn connected() -> Self {
        Self {
            id: String::new(),
            kind: "transport.connected".into(),
            data: Value::Null,
            location: None,
            system: Some(SystemEvent::Connected),
        }
    }

    pub fn disconnected(reason: String) -> Self {
        Self {
            id: String::new(),
            kind: "transport.disconnected".into(),
            data: Value::String(reason),
            location: None,
            system: Some(SystemEvent::Disconnected),
        }
    }

    fn parse(frame: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            id: String,
            #[serde(rename = "type")]
            kind: String,
            #[serde(default)]
            data: Value,
            location: Option<LocationRef>,
        }
        let wire: Wire = serde_json::from_str(frame).ok()?;
        Some(Self {
            id: wire.id,
            kind: wire.kind,
            data: wire.data,
            location: wire.location,
            system: None,
        })
    }
}

pub struct Subscription {
    pub rx: mpsc::UnboundedReceiver<RawEvent>,
    pub cancel: CancellationToken,
}

impl Subscription {
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((idx, separator)) = find_separator(&self.buffer) {
            let block = self.buffer[..idx].to_vec();
            self.buffer.drain(..idx + separator);
            let text = String::from_utf8_lossy(&block);
            let mut data = String::new();
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.trim_start());
                }
            }
            if !data.is_empty() {
                frames.push(data);
            }
        }
        frames
    }
}

fn find_separator(buffer: &[u8]) -> Option<(usize, usize)> {
    for idx in 0..buffer.len() {
        if buffer[idx] == b'\n' && idx + 1 < buffer.len() && buffer[idx + 1] == b'\n' {
            return Some((idx, 2));
        }
        if buffer[idx] == b'\r'
            && idx + 3 < buffer.len()
            && buffer[idx + 1] == b'\n'
            && buffer[idx + 2] == b'\r'
            && buffer[idx + 3] == b'\n'
        {
            return Some((idx, 4));
        }
    }
    None
}

pub fn directory_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_optional_body_accepts_empty_valid_and_rejects_malformed() {
        assert!(matches!(decode_optional_body(""), Ok(None)));
        assert!(matches!(decode_optional_body(" \n\t"), Ok(None)));
        assert_eq!(
            decode_optional_body(r#"{"ok":true}"#).unwrap(),
            Some(serde_json::json!({"ok": true}))
        );
        assert!(decode_optional_body("not json").is_err());
    }
}
