use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);
const CLONE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git failed: {0}")]
    Failed(String),
    #[error("not a git repository: {0}")]
    NotRepository(String),
    #[error("git timed out after {0}s")]
    Timeout(u64),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileStatus>,
}

impl Status {
    pub fn is_clean(&self) -> bool {
        self.files.is_empty()
    }

    pub fn staged(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|file| file.staged)
    }

    pub fn unstaged(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|file| file.has_worktree_changes())
    }

    pub fn staged_count(&self) -> usize {
        self.staged().count()
    }
}

#[derive(Clone, Debug)]
pub struct FileStatus {
    pub path: String,
    pub index: char,
    pub worktree: char,
    pub staged: bool,
    pub additions: u32,
    pub deletions: u32,
}

impl Default for FileStatus {
    fn default() -> Self {
        Self {
            path: String::new(),
            index: '.',
            worktree: '.',
            staged: false,
            additions: 0,
            deletions: 0,
        }
    }
}

impl FileStatus {
    pub fn has_worktree_changes(&self) -> bool {
        self.index == '?' || (self.worktree != '.' && self.worktree != ' ')
    }

    pub fn letter(&self, staged: bool) -> char {
        let letter = if staged { self.index } else { self.worktree };
        match letter {
            '.' | ' ' => '·',
            '?' => 'U',
            other => other,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileDiff {
    pub file: String,
    pub patch: String,
    pub additions: u32,
    pub deletions: u32,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub time: i64,
}

pub fn discover(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    if current.is_file() {
        current = current.parent()?.to_path_buf();
    }
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn is_repo(path: &Path) -> bool {
    discover(path).is_some()
}

pub fn init(repo: &Path) -> Result<(), GitError> {
    if repo.as_os_str().is_empty() {
        return Err(GitError::Failed("path is empty".into()));
    }
    if !repo.is_dir() {
        return Err(GitError::Failed(format!(
            "directory does not exist: {}",
            repo.display()
        )));
    }
    run(repo, &["init", "--quiet"]).map(|_| ())
}

pub fn status(repo: &Path) -> Result<Status, GitError> {
    let output = run(
        repo,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=normal",
            "-z",
        ],
    )?;
    let mut status = Status::default();
    let mut fields = output.split('\0').collect::<Vec<_>>().into_iter();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        if let Some(rest) = field.strip_prefix("# branch.head ") {
            let name = rest.trim();
            status.branch = (!name.is_empty() && name != "(detached)").then(|| name.to_string());
        } else if let Some(rest) = field.strip_prefix("# branch.upstream ") {
            let name = rest.trim();
            if !name.is_empty() {
                status.upstream = Some(name.to_string());
            }
        } else if let Some(rest) = field.strip_prefix("# branch.ab ") {
            for token in rest.split_whitespace() {
                if let Some(value) = token.strip_prefix('+') {
                    status.ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = token.strip_prefix('-') {
                    status.behind = value.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = field.strip_prefix("1 ") {
            if let Some(file) = parse_ordinary(rest) {
                status.files.push(file);
            }
        } else if let Some(rest) = field.strip_prefix("2 ") {
            if let Some(file) = parse_renamed(rest, &mut fields) {
                status.files.push(file);
            }
        } else if let Some(rest) = field.strip_prefix("u ") {
            if let Some(file) = parse_unmerged(rest) {
                status.files.push(file);
            }
        } else if let Some(rest) = field.strip_prefix("? ") {
            status.files.push(FileStatus {
                path: rest.to_string(),
                index: '?',
                worktree: '?',
                staged: false,
                additions: 0,
                deletions: 0,
            });
        }
    }
    status
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    apply_stats(repo, &mut status);
    Ok(status)
}

pub fn diff(
    repo: &Path,
    path: Option<&str>,
    staged: bool,
    context: u32,
) -> Result<Vec<FileDiff>, GitError> {
    let unified = format!("--unified={}", context.min(500));
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", unified.as_str()];
    if staged {
        args.push("--cached");
    }
    let path = path.map(str::trim).filter(|value| !value.is_empty());
    if let Some(path) = path {
        args.push("--");
        args.push(path);
    }
    let output = run(repo, &args)?;
    Ok(parse_diff(&output))
}

pub fn log(repo: &Path, limit: usize) -> Result<Vec<Commit>, GitError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 500);
    let count = format!("--max-count={limit}");
    let output = match run(
        repo,
        &[
            "log",
            "--no-color",
            &count,
            "--pretty=format:%H%x1f%s%x1f%an%x1f%at",
        ],
    ) {
        Ok(output) => output,
        Err(error) if is_empty_history(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut commits = Vec::new();
    for line in output.lines() {
        let mut parts = line.splitn(4, '\u{1f}');
        let (Some(hash), Some(subject), Some(author), Some(time)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if hash.is_empty() {
            continue;
        }
        commits.push(Commit {
            hash: hash.to_string(),
            subject: subject.to_string(),
            author: author.to_string(),
            time: time.parse().unwrap_or(0),
        });
    }
    Ok(commits)
}

pub fn branches(repo: &Path) -> Result<Vec<String>, GitError> {
    let output = run(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    let mut names = output
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn checkout(repo: &Path, branch: &str) -> Result<(), GitError> {
    let branch = valid_name(branch, "branch")?;
    run(repo, &["checkout", branch]).map(|_| ())
}

pub fn create_branch(repo: &Path, name: &str) -> Result<(), GitError> {
    let name = valid_name(name, "branch")?;
    run(repo, &["branch", name]).map(|_| ())
}

pub fn stage(repo: &Path, path: &str) -> Result<(), GitError> {
    let path = valid_path(path)?;
    run(repo, &["add", "--", path]).map(|_| ())
}

pub fn unstage(repo: &Path, path: &str) -> Result<(), GitError> {
    let path = valid_path(path)?;
    match run(repo, &["reset", "--quiet", "HEAD", "--", path]) {
        Ok(_) => Ok(()),
        Err(error) => match run(repo, &["rm", "--cached", "--quiet", "--", path]) {
            Ok(_) => Ok(()),
            Err(_) => Err(error),
        },
    }
}

pub fn discard(repo: &Path, path: &str, staged: bool) -> Result<(), GitError> {
    let path = valid_path(path)?;
    if !is_tracked(repo, path) {
        return run(repo, &["clean", "-f", "-d", "--quiet", "--", path]).map(|_| ());
    }
    if staged {
        match run(
            repo,
            &[
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
                path,
            ],
        ) {
            Ok(_) => Ok(()),
            Err(_) => run(repo, &["rm", "--force", "--quiet", "--", path]).map(|_| ()),
        }
    } else {
        run(repo, &["restore", "--worktree", "--", path]).map(|_| ())
    }
}

pub fn commit(repo: &Path, message: &str) -> Result<(), GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::Failed("commit message is empty".into()));
    }
    run(repo, &["commit", "-m", message]).map(|_| ())
}

pub fn push(repo: &Path) -> Result<(), GitError> {
    run(repo, &["push"]).map(|_| ())
}

pub fn pull(repo: &Path) -> Result<(), GitError> {
    run(repo, &["pull", "--no-edit"]).map(|_| ())
}

pub fn fetch(repo: &Path) -> Result<(), GitError> {
    run(repo, &["fetch", "--all", "--prune"]).map(|_| ())
}

pub fn clone_repo(url: &str, destination: &Path) -> Result<(), GitError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(GitError::Failed("repository URL is empty".into()));
    }
    if url.starts_with('-') || url.contains('\0') || url.contains('\n') || url.contains('\r') {
        return Err(GitError::Failed("repository URL is invalid".into()));
    }
    if url.starts_with("http://") || url.starts_with("file://") {
        return Err(GitError::Failed(
            "repository URL must use HTTPS or SSH".into(),
        ));
    }
    if let Some(rest) = url.strip_prefix("https://")
        && rest
            .split('/')
            .next()
            .is_some_and(|authority| authority.contains('@'))
    {
        return Err(GitError::Failed(
            "repository URL must not contain credentials".into(),
        ));
    }
    if destination.as_os_str().is_empty() {
        return Err(GitError::Failed("clone destination is empty".into()));
    }
    if let Some(parent) = destination.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        return Err(GitError::Failed(format!(
            "clone destination parent does not exist: {}",
            parent.display()
        )));
    }
    if destination.exists() {
        if !destination.is_dir() {
            return Err(GitError::Failed(format!(
                "clone destination is not a directory: {}",
                destination.display()
            )));
        }
        if std::fs::read_dir(destination)?.next().is_some() {
            return Err(GitError::Failed(format!(
                "clone destination is not empty: {}",
                destination.display()
            )));
        }
    }

    let mut command = Command::new("git");
    command
        .args(["clone", "--"])
        .arg(url)
        .arg(destination)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| GitError::Failed(format!("could not start git: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GitError::Failed("git stdout is unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| GitError::Failed("git stderr is unavailable".into()))?;
    let out_reader = std::thread::spawn(move || read_stream(stdout));
    let err_reader = std::thread::spawn(move || read_stream(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= CLONE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(GitError::Timeout(CLONE_TIMEOUT.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Failed(format!("git process error: {error}")));
            }
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let message = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    Err(GitError::Failed(if message.is_empty() {
        "git clone failed".into()
    } else {
        message
    }))
}

fn run(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.pager=cat",
            "-c",
            "color.ui=false",
            "-c",
            "core.quotepath=false",
        ])
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("PAGER", "cat")
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| GitError::Failed(format!("could not start git: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GitError::Failed("git stdout is unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| GitError::Failed("git stderr is unavailable".into()))?;
    let out_reader = std::thread::spawn(move || read_stream(stdout));
    let err_reader = std::thread::spawn(move || read_stream(stderr));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(GitError::Timeout(TIMEOUT.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Failed(format!("git process error: {error}")));
            }
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if status.success() {
        return Ok(stdout);
    }
    let message = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    if message.contains("not a git repository") {
        return Err(GitError::NotRepository(repo.display().to_string()));
    }
    if message.is_empty() {
        let name = args.first().copied().unwrap_or("git");
        return Err(GitError::Failed(format!("git {name} failed")));
    }
    Err(GitError::Failed(message))
}

fn read_stream(stream: impl Read) -> String {
    let mut buffer = Vec::new();
    let _ = stream.take(256 * 1024).read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).to_string()
}

fn valid_path(path: &str) -> Result<&str, GitError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(GitError::Failed("file path is empty".into()));
    }
    if path.starts_with('-') {
        return Err(GitError::Failed("file path is invalid".into()));
    }
    Ok(path)
}

fn valid_name<'a>(name: &'a str, kind: &str) -> Result<&'a str, GitError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(GitError::Failed(format!("{kind} name is empty")));
    }
    if name.starts_with('-') || name.contains('\0') || name.contains('\n') {
        return Err(GitError::Failed(format!("{kind} name is invalid")));
    }
    Ok(name)
}

fn is_tracked(repo: &Path, path: &str) -> bool {
    run(repo, &["ls-files", "--error-unmatch", "--", path]).is_ok()
}

fn apply_stats(repo: &Path, status: &mut Status) {
    let index_stats = numstat(repo, true).unwrap_or_default();
    let worktree_stats = numstat(repo, false).unwrap_or_default();
    for file in &mut status.files {
        let mut additions = 0;
        let mut deletions = 0;
        if file.staged
            && let Some((added, removed)) = index_stats.get(&file.path)
        {
            additions += added;
            deletions += removed;
        }
        if file.has_worktree_changes()
            && file.index != '?'
            && let Some((added, removed)) = worktree_stats.get(&file.path)
        {
            additions += added;
            deletions += removed;
        }
        file.additions = additions;
        file.deletions = deletions;
    }
}

fn numstat(repo: &Path, staged: bool) -> Result<HashMap<String, (u32, u32)>, GitError> {
    let mut args = vec!["diff", "--numstat", "--no-renames", "--no-color"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    let output = run(repo, &args)?;
    let mut stats = HashMap::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        stats.insert(
            path.to_string(),
            (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)),
        );
    }
    Ok(stats)
}

fn parse_ordinary(rest: &str) -> Option<FileStatus> {
    let mut parts = rest.splitn(8, ' ');
    let xy = parts.next()?;
    let path = parts.nth(6)?;
    Some(file_status(xy, path))
}

fn parse_renamed(rest: &str, fields: &mut std::vec::IntoIter<&str>) -> Option<FileStatus> {
    let mut parts = rest.splitn(9, ' ');
    let xy = parts.next()?;
    let path = parts.nth(7)?;
    let _original = fields.next();
    Some(file_status(xy, path))
}

fn parse_unmerged(rest: &str) -> Option<FileStatus> {
    let mut parts = rest.splitn(10, ' ');
    let xy = parts.next()?;
    let path = parts.nth(8)?;
    let (index, worktree) = xy_pair(xy);
    Some(FileStatus {
        path: path.to_string(),
        index,
        worktree,
        staged: false,
        additions: 0,
        deletions: 0,
    })
}

fn file_status(xy: &str, path: &str) -> FileStatus {
    let (index, worktree) = xy_pair(xy);
    FileStatus {
        path: path.to_string(),
        index,
        worktree,
        staged: index != '.' && index != ' ' && index != '?',
        additions: 0,
        deletions: 0,
    }
}

fn xy_pair(xy: &str) -> (char, char) {
    let mut chars = xy.chars();
    (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'))
}

fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut in_hunk = false;

    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.starts_with("diff --git ") {
            if let Some(file) = current.take()
                && !file.patch.trim().is_empty()
            {
                files.push(file);
            }
            let mut file = FileDiff {
                status: Some("modified".to_string()),
                ..FileDiff::default()
            };
            file.file = header_path(line.trim_start_matches("diff --git "));
            current = Some(file);
            in_hunk = false;
        }

        let Some(file) = current.as_mut() else {
            continue;
        };
        file.patch.push_str(raw);

        if in_hunk {
            if line.starts_with('+') {
                file.additions += 1;
            } else if line.starts_with('-') {
                file.deletions += 1;
            }
            continue;
        }

        if line.starts_with("@@") {
            in_hunk = true;
        } else if line.starts_with("new file mode") {
            file.status = Some("added".to_string());
        } else if line.starts_with("deleted file mode") {
            file.status = Some("deleted".to_string());
        } else if line.starts_with("rename from ") {
            file.status = Some("renamed".to_string());
        } else if let Some(name) = line.strip_prefix("--- ") {
            if name.trim() == "/dev/null" {
                file.status = Some("added".to_string());
            } else if file.file.is_empty() {
                file.file = strip_prefix_path(name);
            }
        } else if let Some(name) = line.strip_prefix("+++ ") {
            if name.trim() == "/dev/null" {
                file.status = Some("deleted".to_string());
            } else {
                file.file = strip_prefix_path(name);
            }
        }
    }

    if let Some(file) = current.take()
        && !file.patch.trim().is_empty()
    {
        files.push(file);
    }
    files
}

fn header_path(rest: &str) -> String {
    if let Some(index) = rest.rfind(" b/") {
        return strip_prefix_path(&rest[index + 1..]);
    }
    rest.split_whitespace()
        .last()
        .map(strip_prefix_path)
        .unwrap_or_default()
}

fn strip_prefix_path(value: &str) -> String {
    let value = value.trim();
    let value = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    };
    value
        .strip_prefix("a/")
        .or_else(|| value.strip_prefix("b/"))
        .unwrap_or(value)
        .to_string()
}

fn is_empty_history(error: &GitError) -> bool {
    let GitError::Failed(message) = error else {
        return false;
    };
    message.contains("does not have any commits yet")
        || message.contains("unknown revision")
        || message.contains("bad revision")
        || message.contains("bad default revision")
        || message.contains("ambiguous argument 'HEAD'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "rchamber-git-{label}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn clone_rejects_invalid_arguments() {
        let target = temp_dir("invalid");
        std::fs::create_dir_all(&target).unwrap();
        assert!(matches!(
            clone_repo("", &target.join("repo")),
            Err(GitError::Failed(message)) if message.contains("empty")
        ));
        assert!(matches!(
            clone_repo("--upload-pack=evil", &target.join("repo")),
            Err(GitError::Failed(message)) if message.contains("invalid")
        ));
        std::fs::remove_dir_all(&target).unwrap();
    }

    #[test]
    fn clone_rejects_non_empty_destination() {
        let target = temp_dir("non-empty");
        let destination = target.join("repo");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("file.txt"), "content").unwrap();
        assert!(matches!(
            clone_repo("https://example.com/owner/repo.git", &destination),
            Err(GitError::Failed(message)) if message.contains("not empty")
        ));
        std::fs::remove_dir_all(&target).unwrap();
    }

    #[test]
    fn clone_creates_a_local_repository() {
        let target = temp_dir("local");
        let source = target.join("source");
        std::fs::create_dir_all(&source).unwrap();
        init(&source).unwrap();
        let destination = target.join("cloned");
        clone_repo(&source.to_string_lossy(), &destination).unwrap();
        assert!(destination.join(".git").exists());
        std::fs::remove_dir_all(&target).unwrap();
    }
}
