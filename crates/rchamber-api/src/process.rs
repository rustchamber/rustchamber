use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::client::{ApiError, Client, ClientConfig};
use crate::models::ServerInfo;

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub directory: Option<PathBuf>,
    pub binary: Option<PathBuf>,
    pub port: Option<u16>,
    pub password: Option<String>,
    pub external_url: Option<String>,
}

pub struct ServerHandle {
    pub client: Client,
    pub info: ServerInfo,
    pub password: Option<String>,
    pub endpoint: String,
    child: Option<Child>,
}

impl ServerHandle {
    pub fn is_managed(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.start_kill();
        }
    }
}

pub async fn launch(options: LaunchOptions) -> Result<ServerHandle, ApiError> {
    if let Some(url) = options.external_url.clone() {
        let password = options
            .password
            .or_else(|| std::env::var("OPENCODE_PASSWORD").ok())
            .filter(|p| !p.is_empty());
        let mut config = ClientConfig::new(&url)?;
        config.password = password.clone();
        if let Some(dir) = readable_directory(options.directory.as_deref()) {
            config.directory = Some(dir);
        }
        let client = Client::new(config)?;
        let info = wait_ready(&client, Duration::from_secs(8)).await?;
        return Ok(ServerHandle {
            client,
            info,
            password,
            endpoint: url,
            child: None,
        });
    }

    let binary = options
        .binary
        .clone()
        .or_else(find_opencode_binary)
        .ok_or_else(|| {
            ApiError::Config(
                "OpenCode binary not found. Install opencode or set RCHAMBER_OPENCODE_BIN.".into(),
            )
        })?;
    let port = match options.port {
        Some(port) => port,
        None => free_port().map_err(|e| ApiError::Other(format!("no free port: {e}")))?,
    };
    let password = options
        .password
        .or_else(|| std::env::var("OPENCODE_PASSWORD").ok())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(random_password);
    let endpoint = format!("http://127.0.0.1:{port}");

    tracing::debug!(
        endpoint = %endpoint,
        password_len = password.len(),
        "starting managed OpenCode server"
    );
    let mut command = Command::new(&binary);
    command
        .arg("serve")
        .arg("--hostname")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .env("OPENCODE_PASSWORD", &password)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .env("OPENCODE_SERVER_USERNAME", "opencode")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = options.directory.as_deref()
        && dir.is_dir()
    {
        command.current_dir(dir);
    }

    let mut child = command
        .spawn()
        .map_err(|e| ApiError::Other(format!("could not start {}: {e}", binary.display())))?;
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, "stdout");
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, "stderr");
    }

    let mut config = ClientConfig::new(&endpoint)?;
    config.password = Some(password.clone());
    if let Some(dir) = readable_directory(options.directory.as_deref()) {
        config.directory = Some(dir);
    }
    let client = Client::new(config)?;
    let info = match wait_ready(&client, Duration::from_secs(45)).await {
        Ok(info) => info,
        Err(err) => {
            let _ = child.start_kill();
            return Err(err);
        }
    };

    Ok(ServerHandle {
        client,
        info,
        password: Some(password),
        endpoint,
        child: Some(child),
    })
}

fn spawn_log_reader<R>(reader: R, source: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "opencode", "%{source}% {line}");
        }
    });
}

async fn wait_ready(client: &Client, timeout: Duration) -> Result<ServerInfo, ApiError> {
    let started = tokio::time::Instant::now();
    let mut last: Option<ApiError> = None;
    while started.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_secs(2), client.info()).await {
            Ok(Ok(info)) => return Ok(info),
            Ok(Err(err)) => last = Some(err),
            Err(_) => last = Some(ApiError::Other("health request timed out".into())),
        }
        tokio::time::sleep(Duration::from_millis(180)).await;
    }
    let error = last.unwrap_or_else(|| ApiError::Other("server did not become ready".into()));
    tracing::debug!(error = %error, "OpenCode server did not become ready");
    Err(error)
}

fn readable_directory(directory: Option<&Path>) -> Option<String> {
    let dir = directory?;
    if dir.is_dir() {
        Some(dir.to_string_lossy().to_string())
    } else {
        None
    }
}

fn random_password() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 24] = rng.random();
    let mut out = String::with_capacity(48);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn find_opencode_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RCHAMBER_OPENCODE_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(if cfg!(windows) {
                "opencode.exe"
            } else {
                "opencode"
            });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let file = if cfg!(windows) {
        "opencode.exe"
    } else {
        "opencode"
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(home) = &home {
        // The official installer and common package layouts. A desktop session
        // launched from the application menu often has a shorter PATH than an
        // interactive shell, so these must not depend on PATH alone.
        candidates.push(home.join(".opencode").join("bin").join(file));
        candidates.push(home.join(".local").join("bin").join(file));
        candidates.push(home.join(".bun").join("bin").join(file));
        candidates.push(home.join(".npm-global").join("bin").join(file));
    }
    candidates.push(PathBuf::from("/opt/opencode/bin").join(file));
    candidates.push(PathBuf::from("/usr/local/bin").join(file));
    candidates.push(PathBuf::from("/usr/bin").join(file));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub fn default_workspace() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    })
}
