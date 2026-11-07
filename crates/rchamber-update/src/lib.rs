mod github;
mod install;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub use install::InstallKind;
use install::{Environment, PreparedInstall};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

const FIRST_CHECK: Duration = Duration::from_secs(3);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);
const FEED: &str = "https://api.github.com/repos/rustchamber/rustchamber/releases?per_page=20";
const ATOM: &str = "https://github.com/rustchamber/rustchamber/releases.atom";
const USER_AGENT: &str = "RustChamber";

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: String,
    pub published_label: Option<String>,
    pub release_url: String,
    pub download_url: String,
    pub download_size: Option<u64>,
    pub sha256: Option<String>,
    pub install: InstallKind,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Updating,
    Ready,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub phase: Phase,
    pub info: Option<UpdateInfo>,
    pub received: u64,
    pub total: Option<u64>,
    pub fraction: Option<f32>,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn progress(&self) -> f32 {
        if let Some(fraction) = self.fraction {
            return fraction.clamp(0.0, 1.0);
        }
        match self.total {
            Some(total) if total > 0 => {
                (self.received as f64 / total as f64).clamp(0.0, 1.0) as f32
            }
            _ => 0.0,
        }
    }

    pub fn ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Downloading | Phase::Updating | Phase::Checking
        )
    }
}

enum Command {
    Check,
    Download,
    Shutdown,
}

pub struct Updater {
    commands: UnboundedSender<Command>,
    events: UnboundedReceiver<Snapshot>,
    latest: Snapshot,
    prepared: Arc<Mutex<Option<PreparedInstall>>>,
}

impl Updater {
    pub fn spawn(current_version: &str, wake: Wake) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let prepared = Arc::new(Mutex::new(None));
        let worker_prepared = Arc::clone(&prepared);
        let version = current_version.to_string();
        std::thread::Builder::new()
            .name("rchamber-update".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    return;
                };
                runtime.block_on(async move {
                    let mut worker = Worker::new(version, event_tx, wake, worker_prepared);
                    worker.run(command_rx).await;
                });
            })
            .expect("spawn updater thread");
        Self {
            commands: command_tx,
            events: event_rx,
            latest: Snapshot::default(),
            prepared,
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(snapshot) = self.events.try_recv() {
            self.latest = snapshot;
            changed = true;
        }
        changed
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.latest
    }

    pub fn check(&self) {
        let _ = self.commands.send(Command::Check);
    }

    pub fn download(&self) {
        let _ = self.commands.send(Command::Download);
    }

    pub fn restart(&self) -> Result<(), String> {
        let prepared = self
            .prepared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| "The update has not finished downloading.".to_string())?;
        install::spawn_install(&prepared)
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

struct Worker {
    current: semver::Version,
    current_raw: String,
    client: reqwest::Client,
    events: UnboundedSender<Snapshot>,
    wake: Wake,
    snapshot: Snapshot,
    prepared: Arc<Mutex<Option<PreparedInstall>>>,
    environment: Environment,
    flatpak: bool,
}

impl Worker {
    fn new(
        current_version: String,
        events: UnboundedSender<Snapshot>,
        wake: Wake,
        prepared: Arc<Mutex<Option<PreparedInstall>>>,
    ) -> Self {
        let current = semver::Version::parse(&current_version)
            .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            current,
            current_raw: current_version,
            client,
            events,
            wake,
            snapshot: Snapshot::default(),
            prepared,
            environment: install::discover(),
            flatpak: std::env::var_os("FLATPAK_ID").is_some(),
        }
    }

    async fn run(&mut self, mut commands: UnboundedReceiver<Command>) {
        let mut next_check = tokio::time::Instant::now() + FIRST_CHECK;
        if self.flatpak || std::env::var_os("RCHAMBER_NO_UPDATE_CHECK").is_some() {
            next_check = tokio::time::Instant::now() + Duration::from_secs(60 * 60 * 24 * 365);
        }
        loop {
            tokio::select! {
                command = commands.recv() => {
                    match command {
                        None | Some(Command::Shutdown) => return,
                        Some(Command::Check) => {
                            self.check().await;
                            next_check = tokio::time::Instant::now() + self.next_interval();
                        }
                        Some(Command::Download) => self.download().await,
                    }
                }
                _ = tokio::time::sleep_until(next_check) => {
                    self.check().await;
                    next_check = tokio::time::Instant::now() + self.next_interval();
                }
            }
        }
    }

    async fn check(&mut self) {
        if self.flatpak {
            self.snapshot.phase = Phase::Idle;
            self.snapshot.info = None;
            self.snapshot.error = None;
            self.publish();
            return;
        }
        if self.snapshot.phase == Phase::Ready || self.snapshot.phase == Phase::Downloading {
            return;
        }
        self.snapshot.phase = Phase::Checking;
        self.snapshot.error = None;
        self.publish();
        let custom = std::env::var("RCHAMBER_UPDATE_FEED").ok();
        let feed = custom.clone().unwrap_or_else(|| FEED.to_string());
        match self.fetch_release(&feed, custom.is_none()).await {
            Ok(Some(release)) => {
                let info = self.info(release);
                tracing::info!(
                    version = %info.version,
                    install = ?info.install,
                    "update available"
                );
                self.log(&format!("update available: {}", info.version));
                self.snapshot.info = Some(info);
                self.snapshot.phase = Phase::Available;
            }
            Ok(None) => {
                self.log("up to date");
                self.snapshot.info = None;
                self.snapshot.phase = Phase::Idle;
            }
            Err(error) => {
                tracing::warn!("update check failed: {error}");
                self.log(&format!("update check failed: {error}"));
                self.snapshot.phase = if self.snapshot.info.is_some() {
                    Phase::Available
                } else {
                    Phase::Idle
                };
                self.snapshot.error = Some(error);
            }
        }
        self.publish();
    }

    async fn fetch_release(
        &self,
        feed: &str,
        allow_atom: bool,
    ) -> Result<Option<github::Release>, String> {
        match self.fetch_json(feed).await {
            Ok(release) => Ok(release),
            Err(error) if allow_atom => {
                tracing::warn!(
                    "GitHub API update check failed, falling back to the release feed: {error}"
                );
                match self.fetch_atom().await {
                    Ok(release) => Ok(release),
                    Err(atom_error) => Err(format!(
                        "{error} The release feed also failed: {atom_error}"
                    )),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn fetch_json(&self, feed: &str) -> Result<Option<github::Release>, String> {
        let response = self
            .client
            .get(feed)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(|error| format!("Could not reach the update server: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "The update server answered {}.",
                response.status().as_u16()
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read the update feed: {error}"))?;
        let releases = github::parse_releases(&body)?;
        Ok(github::pick_release(&releases, &self.current))
    }

    async fn fetch_atom(&self) -> Result<Option<github::Release>, String> {
        let response = self
            .client
            .get(ATOM)
            .header(reqwest::header::ACCEPT, "application/atom+xml")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(|error| format!("Could not reach the release feed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "The release feed answered {}.",
                response.status().as_u16()
            ));
        }
        let body = response
            .text()
            .await
            .map_err(|error| format!("Could not read the release feed: {error}"))?;
        let releases = github::parse_atom(&body)?;
        Ok(github::pick_release(&releases, &self.current))
    }

    fn log(&self, message: &str) {
        use std::io::Write as _;
        let path = install::cache_dir().join("update.log");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{} {message}", chrono::Utc::now().to_rfc3339());
        }
    }

    fn info(&self, release: github::Release) -> UpdateInfo {
        let version = release
            .version()
            .map(|version| version.to_string())
            .unwrap_or_else(|| release.tag_name.trim_start_matches('v').to_string());
        let platform_asset = release.platform_asset();
        let install = if platform_asset.is_none() {
            InstallKind::External
        } else {
            self.environment.kind
        };
        let asset = if install == InstallKind::Homebrew {
            release.macos_dmg().or_else(|| release.macos_zip())
        } else {
            platform_asset
        };
        let download_url = asset
            .map(|asset| asset.browser_download_url.clone())
            .unwrap_or_else(|| github::release_url(&release));
        let download_size = asset.map(|asset| asset.size).filter(|size| *size > 0);
        let sha256 = asset
            .and_then(|asset| asset.digest.as_deref())
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .map(str::to_string);
        let detail = if install == InstallKind::External {
            self.environment.detail.clone().or_else(|| {
                platform_asset.is_none().then(|| {
                    format!(
                        "This release has no {} build attached.",
                        github::platform_name(github::current_platform())
                    )
                })
            })
        } else {
            None
        };
        UpdateInfo {
            version,
            current_version: self.current_raw.clone(),
            notes: release.notes(),
            published_label: release.published_at.as_deref().and_then(date_label),
            release_url: github::release_url(&release),
            download_url,
            download_size,
            sha256,
            install,
            detail,
        }
    }

    async fn download(&mut self) {
        if self.snapshot.phase == Phase::Ready {
            return;
        }
        let Some(info) = self.snapshot.info.clone() else {
            return;
        };
        match info.install {
            InstallKind::External => {}
            InstallKind::Homebrew => self.homebrew(info).await,
            InstallKind::Native => self.native(info).await,
        }
    }

    async fn native(&mut self, info: UpdateInfo) {
        self.snapshot.phase = Phase::Downloading;
        self.snapshot.received = 0;
        self.snapshot.total = info.download_size;
        self.snapshot.fraction = None;
        self.snapshot.error = None;
        self.publish();
        match self.download_and_stage(&info).await {
            Ok(staged) => {
                let bundle = self.environment.bundle.clone().unwrap_or_default();
                self.remember(PreparedInstall {
                    kind: InstallKind::Native,
                    bundle,
                    staged: Some(staged),
                    log: install::cache_dir().join("update.log"),
                });
                self.snapshot.phase = Phase::Ready;
                self.snapshot.received = self.snapshot.total.unwrap_or(self.snapshot.received);
            }
            Err(error) => {
                tracing::warn!("update download failed: {error}");
                self.snapshot.phase = Phase::Available;
                self.snapshot.error = Some(error);
            }
        }
        self.publish();
    }

    async fn download_and_stage(&mut self, info: &UpdateInfo) -> Result<PathBuf, String> {
        let dir = install::cache_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Could not create the update folder: {error}"))?;
        let name = info
            .download_url
            .rsplit('/')
            .next()
            .filter(|name| name.ends_with(".zip") || name.ends_with(".tar.gz"))
            .map(str::to_string)
            .unwrap_or_else(|| default_archive_name(&info.version));
        let archive = dir.join(name);
        let verified = archive.is_file()
            && info.sha256.as_deref().is_some_and(|expected| {
                install::sha256_file(&archive).is_ok_and(|hash| hash == expected)
            });
        if !verified {
            let partial = archive.with_extension("part");
            self.fetch_asset(info, &archive, &partial, None).await?;
        }
        let actual = install::sha256_file(&archive)?;
        if let Some(expected) = info.sha256.as_deref()
            && actual != expected
        {
            let _ = std::fs::remove_file(&archive);
            return Err("The downloaded update failed its integrity check.".into());
        }
        self.snapshot.total = self.snapshot.total.or(info.download_size);
        self.snapshot.received = self.snapshot.total.unwrap_or(0);
        self.publish();
        install::prepare_archive(&info.version, &archive)
    }

    async fn fetch_asset(
        &mut self,
        info: &UpdateInfo,
        target: &Path,
        partial: &Path,
        fraction: Option<(f32, f32)>,
    ) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let _ = std::fs::remove_file(partial);
        let mut response = self
            .client
            .get(&info.download_url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(|error| format!("Download failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Download failed: the server answered {}.",
                response.status().as_u16()
            ));
        }
        self.snapshot.total = response.content_length().or(info.download_size);
        if let Some((start, _)) = fraction {
            self.snapshot.fraction = Some(start);
        }
        self.publish();
        let mut file = tokio::fs::File::create(partial)
            .await
            .map_err(|error| format!("Could not save the update: {error}"))?;
        let mut received = 0u64;
        let mut last_publish = Instant::now();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("Download failed: {error}"))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("Could not save the update: {error}"))?;
            received += chunk.len() as u64;
            self.snapshot.received = received;
            if let Some((start, end)) = fraction {
                let ratio = match self.snapshot.total {
                    Some(total) if total > 0 => {
                        (received as f64 / total as f64).clamp(0.0, 1.0) as f32
                    }
                    _ => 0.0,
                };
                self.snapshot.fraction = Some(start + (end - start) * ratio);
            }
            if last_publish.elapsed() >= Duration::from_millis(90) {
                last_publish = Instant::now();
                self.publish();
            }
        }
        file.flush()
            .await
            .map_err(|error| format!("Could not save the update: {error}"))?;
        drop(file);
        if let Some(total) = self.snapshot.total
            && received != total
        {
            let _ = std::fs::remove_file(partial);
            return Err(format!(
                "The download stopped early ({received} of {total} bytes)."
            ));
        }
        std::fs::rename(partial, target)
            .map_err(|error| format!("Could not finish the update: {error}"))?;
        self.snapshot.received = received;
        if let Some((_, end)) = fraction {
            self.snapshot.fraction = Some(end);
        }
        self.publish();
        Ok(())
    }

    async fn homebrew(&mut self, info: UpdateInfo) {
        let Some(brew) = self.environment.brew.clone() else {
            self.snapshot.error = Some("Homebrew could not be found.".into());
            self.publish();
            return;
        };
        let Some(bundle) = self.environment.bundle.clone() else {
            self.snapshot.error = Some("RustChamber is not running from an app bundle.".into());
            self.publish();
            return;
        };
        self.snapshot.phase = Phase::Updating;
        self.snapshot.error = None;
        self.snapshot.received = 0;
        self.snapshot.total = info.download_size;
        self.snapshot.fraction = Some(0.0);
        self.publish();

        let brew_for_update = brew.clone();
        let _ = tokio::task::spawn_blocking(move || install::brew_update(&brew_for_update)).await;
        self.snapshot.fraction = Some(0.03);
        self.publish();

        let brew_for_cache = brew.clone();
        let cache = tokio::task::spawn_blocking(move || install::brew_cache_path(&brew_for_cache))
            .await
            .ok()
            .flatten()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(&info.version))
                    && info.download_url.ends_with(".dmg")
            });
        if let Some(path) = cache {
            if let Some(parent) = path.parent()
                && let Err(error) = std::fs::create_dir_all(parent)
            {
                return self
                    .homebrew_failed(format!("Could not prepare the Homebrew cache: {error}"));
            }
            let cached = path.is_file()
                && info.sha256.as_deref().is_none_or(|expected| {
                    install::sha256_file(&path).is_ok_and(|hash| hash == expected)
                });
            if !cached {
                let partial = path.with_extension("part");
                if let Err(error) = self
                    .fetch_asset(&info, &path, &partial, Some((0.05, 0.85)))
                    .await
                {
                    return self.homebrew_failed(error);
                }
                if let Some(expected) = info.sha256.as_deref() {
                    match install::sha256_file(&path) {
                        Ok(actual) if actual == expected => {}
                        Ok(_) => {
                            let _ = std::fs::remove_file(&path);
                            return self.homebrew_failed(
                                "The downloaded Homebrew package failed its integrity check."
                                    .into(),
                            );
                        }
                        Err(error) => return self.homebrew_failed(error),
                    }
                }
            }
            self.snapshot.fraction = Some(0.88);
            self.publish();
        } else {
            self.snapshot.fraction = Some(0.10);
            self.publish();
        }

        self.snapshot.fraction = Some(0.92);
        self.publish();
        let result = tokio::task::spawn_blocking(move || install::run_brew_upgrade(&brew)).await;
        let result = match result {
            Ok(result) => result,
            Err(error) => Err(format!("Could not run Homebrew: {error}")),
        };
        if let Err(error) = result {
            return self.homebrew_failed(error);
        }
        match install::bundle_version(&bundle) {
            Some(installed) if installed == info.version => {
                self.remember(PreparedInstall {
                    kind: InstallKind::Homebrew,
                    bundle: bundle.to_path_buf(),
                    staged: None,
                    log: install::cache_dir().join("update.log"),
                });
                self.snapshot.fraction = Some(1.0);
                self.snapshot.phase = Phase::Ready;
            }
            Some(installed) => {
                return self.homebrew_failed(format!(
                    "Homebrew finished, but {installed} is still installed instead of {}. Run `brew update` and try again.",
                    info.version
                ));
            }
            None => {
                return self
                    .homebrew_failed("Could not read the version Homebrew installed.".into());
            }
        }
        self.publish();
    }

    fn homebrew_failed(&mut self, error: String) {
        self.snapshot.phase = Phase::Available;
        self.snapshot.fraction = None;
        self.snapshot.error = Some(error);
        self.publish();
    }

    fn remember(&self, prepared: PreparedInstall) {
        *self
            .prepared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(prepared);
    }

    fn next_interval(&self) -> Duration {
        if self.snapshot.phase == Phase::Idle && self.snapshot.error.is_some() {
            RETRY_INTERVAL
        } else {
            CHECK_INTERVAL
        }
    }

    fn publish(&self) {
        let _ = self.events.send(self.snapshot.clone());
        (self.wake)();
    }
}

fn default_archive_name(version: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("RustChamber-{version}-macos-universal.zip")
    } else if cfg!(target_os = "windows") {
        format!("RustChamber-{version}-windows-x86_64.zip")
    } else {
        format!("RustChamber-{version}-linux-x86_64.tar.gz")
    }
}

fn date_label(value: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(value).ok()?;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    Some(format!(
        "{} {}, {}",
        MONTHS[parsed.month0() as usize],
        parsed.day(),
        parsed.year()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_clamped_and_survives_unknown_totals() {
        let mut snapshot = Snapshot::default();
        assert_eq!(snapshot.progress(), 0.0);
        snapshot.received = 50;
        snapshot.total = Some(100);
        assert!((snapshot.progress() - 0.5).abs() < f32::EPSILON);
        snapshot.received = 250;
        assert_eq!(snapshot.progress(), 1.0);
    }

    #[test]
    fn formats_release_dates() {
        assert_eq!(
            date_label("2026-11-04T12:00:00Z").as_deref(),
            Some("Nov 4, 2026")
        );
        assert_eq!(date_label("not a date"), None);
    }

    #[test]
    fn busy_states_do_not_offer_a_check() {
        for phase in [Phase::Downloading, Phase::Updating, Phase::Checking] {
            assert!(
                Snapshot {
                    phase,
                    ..Snapshot::default()
                }
                .busy()
            );
        }
        assert!(!Snapshot::default().busy());
    }
}
