use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Native,
    Homebrew,
    External,
}

#[derive(Clone, Debug)]
pub struct Environment {
    pub kind: InstallKind,
    pub bundle: Option<PathBuf>,
    pub detail: Option<String>,
    pub brew: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct PreparedInstall {
    pub kind: InstallKind,
    pub bundle: PathBuf,
    pub staged: Option<PathBuf>,
    pub log: PathBuf,
}

#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "dev.rustchamber.RustChamber";
const APP_BUNDLE: &str = "/Applications/RustChamber.app";

pub fn discover() -> Environment {
    if cfg!(target_os = "macos") {
        return discover_macos();
    }
    if cfg!(target_os = "linux") {
        return discover_linux();
    }
    Environment {
        kind: InstallKind::External,
        bundle: current_bundle(),
        detail: Some(
            "Automatic updates are not available on this platform. Download the new version from the release page."
                .into(),
        ),
        brew: None,
    }
}

fn discover_macos() -> Environment {
    let bundle = current_bundle();
    let Some(bundle) = bundle else {
        return Environment {
            kind: InstallKind::External,
            bundle: None,
            detail: Some(
                "This build is not running from an app bundle, so it cannot replace itself.".into(),
            ),
            brew: None,
        };
    };
    if bundle.starts_with("/Volumes") {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(bundle),
            detail: Some(
                "RustChamber is running from a mounted disk image. Move it to Applications to update in place."
                    .into(),
            ),
            brew: None,
        };
    }
    let brew = brew_path();
    if is_brew_managed(&bundle, brew.as_deref()) {
        return Environment {
            kind: InstallKind::Homebrew,
            bundle: Some(bundle),
            detail: None,
            brew,
        };
    }
    let writable = bundle.parent().is_some_and(directory_writable);
    if !writable {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(bundle),
            detail: Some(
                "The folder that holds RustChamber is not writable. Move it to Applications or update it manually."
                    .into(),
            ),
            brew: None,
        };
    }
    Environment {
        kind: InstallKind::Native,
        bundle: Some(bundle),
        detail: None,
        brew: None,
    }
}

fn discover_linux() -> Environment {
    let Some(executable) = current_bundle() else {
        return Environment {
            kind: InstallKind::External,
            bundle: None,
            detail: Some(
                "Could not locate the RustChamber executable, so it cannot replace itself.".into(),
            ),
            brew: None,
        };
    };
    if std::env::var_os("APPIMAGE").is_some() {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(executable),
            detail: Some(
                "RustChamber is running from an AppImage. Download the new AppImage from the release page."
                    .into(),
            ),
            brew: None,
        };
    }
    if let Ok(app_id) = std::env::var("FLATPAK_ID")
        && !app_id.trim().is_empty()
    {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(executable),
            detail: Some(format!(
                "RustChamber is a Flatpak. Update it with `flatpak update {}`.",
                app_id.trim()
            )),
            brew: None,
        };
    }
    let system_package = executable.starts_with("/usr/bin")
        || executable.starts_with("/usr/sbin")
        || executable.starts_with("/nix/store")
        || std::env::var_os("SNAP").is_some();
    if system_package {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(executable),
            detail: Some(
                "RustChamber was installed by a system package manager. Update it with apt, dnf, pacman, or your distribution's package tool."
                    .into(),
            ),
            brew: None,
        };
    }
    let writable = executable.parent().is_some_and(directory_writable);
    if !writable {
        return Environment {
            kind: InstallKind::External,
            bundle: Some(executable),
            detail: Some(
                "RustChamber is installed in a folder that is not writable. Download the new version and replace it manually."
                    .into(),
            ),
            brew: None,
        };
    }
    Environment {
        kind: InstallKind::Native,
        bundle: Some(executable),
        detail: None,
        brew: None,
    }
}

pub fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(target_os = "macos") {
        bundle_ancestor(&exe)
    } else {
        Some(exe)
    }
}

#[cfg(target_os = "macos")]
fn bundle_ancestor(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|path| {
            path.extension().is_some_and(|ext| ext == "app")
                && path.join("Contents/Info.plist").is_file()
        })
        .map(Path::to_path_buf)
}

#[cfg(not(target_os = "macos"))]
fn bundle_ancestor(_exe: &Path) -> Option<PathBuf> {
    None
}

fn is_brew_managed(bundle: &Path, brew: Option<&Path>) -> bool {
    let Some(brew) = brew else {
        return false;
    };
    let resolved = bundle
        .canonicalize()
        .unwrap_or_else(|_| bundle.to_path_buf());
    if resolved
        .components()
        .any(|part| part.as_os_str() == "Caskroom")
    {
        return true;
    }
    if resolved != Path::new(APP_BUNDLE) {
        return false;
    }
    Command::new(brew)
        .args(["list", "--cask", "rustchamber"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn brew_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(prefix) = std::env::var_os("HOMEBREW_PREFIX") {
        candidates.push(PathBuf::from(prefix).join("bin/brew"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/brew"));
    candidates.push(PathBuf::from("/usr/local/bin/brew"));
    candidates.into_iter().find(|path| path.is_file())
}

fn directory_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".rchamber-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

pub fn cache_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.cache_dir().join("rustchamber").join("updates"))
        .unwrap_or_else(|| std::env::temp_dir().join("rustchamber-updates"))
}

pub fn prepare_archive(version: &str, archive: &Path) -> Result<PathBuf, String> {
    let stage = cache_dir().join(format!("stage-{version}"));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage)
        .map_err(|error| format!("Could not create the update folder: {error}"))?;
    prepare_platform_archive(version, archive, &stage)
}

#[cfg(target_os = "macos")]
fn prepare_platform_archive(
    version: &str,
    archive: &Path,
    stage: &Path,
) -> Result<PathBuf, String> {
    let status = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(archive)
        .arg(stage)
        .status()
        .map_err(|error| format!("Could not unpack the update: {error}"))?;
    if !status.success() {
        return Err("The downloaded update could not be unpacked.".into());
    }
    let app = find_app(stage)
        .ok_or_else(|| "The downloaded update does not contain RustChamber.app.".to_string())?;
    verify_app(&app, version)?;
    let _ = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&app)
        .status();
    Ok(app)
}

#[cfg(target_os = "linux")]
fn prepare_platform_archive(
    version: &str,
    archive: &Path,
    stage: &Path,
) -> Result<PathBuf, String> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(stage)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| format!("Could not unpack the update: {error}"))?;
    if !status.success() {
        return Err("The downloaded update could not be unpacked.".into());
    }
    let binary = find_binary(stage).ok_or_else(|| {
        "The downloaded update does not contain the RustChamber executable.".to_string()
    })?;
    verify_linux_binary(&binary, version)?;
    Ok(binary)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn prepare_platform_archive(
    _version: &str,
    _archive: &Path,
    _stage: &Path,
) -> Result<PathBuf, String> {
    Err("Automatic updates are not available on this platform.".into())
}

#[cfg(target_os = "macos")]
fn find_app(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "app") {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_app(&path)
        {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn find_binary(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|name| name == "rchamber") {
            return Some(path);
        }
        if path.is_dir() {
            directories.push(path);
        }
    }
    directories.into_iter().find_map(|path| find_binary(&path))
}

#[cfg(target_os = "linux")]
fn verify_linux_binary(binary: &Path, version: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Could not make the update executable: {error}"))?;
    }
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not verify the downloaded app: {error}"))?;
    if !output.status.success() {
        return Err("The downloaded app could not be started.".into());
    }
    let text = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !text.split_whitespace().any(|part| part == version) {
        return Err(format!(
            "The downloaded app reports a different version than {version}."
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn verify_app(app: &Path, version: &str) -> Result<(), String> {
    let plist = app.join("Contents/Info.plist");
    let id = plist_value(&plist, "CFBundleIdentifier")
        .ok_or_else(|| "The downloaded app has no bundle identifier.".to_string())?;
    if id != BUNDLE_ID {
        return Err(format!(
            "The downloaded app is not RustChamber (bundle {id})."
        ));
    }
    let installed = plist_value(&plist, "CFBundleShortVersionString")
        .ok_or_else(|| "The downloaded app has no version.".to_string())?;
    if installed != version {
        return Err(format!(
            "The downloaded app is version {installed}, expected {version}."
        ));
    }
    let status = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("Could not verify the update signature: {error}"))?;
    if !status.success() {
        return Err("The downloaded update is not correctly signed.".into());
    }
    Ok(())
}

pub fn bundle_version(bundle: &Path) -> Option<String> {
    plist_value(
        &bundle.join("Contents/Info.plist"),
        "CFBundleShortVersionString",
    )
}

fn plist_value(plist: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(plist)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not read the downloaded update: {error}"))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("Could not read the downloaded update: {error}"))?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn run_brew_upgrade(brew: &Path) -> Result<(), String> {
    let output = Command::new(brew)
        .args(["upgrade", "--cask", "rustchamber"])
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_COLOR", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run Homebrew: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    let tail: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = tail[tail.len().saturating_sub(8)..].join("\n");
    Err(if tail.is_empty() {
        "Homebrew could not install the update.".into()
    } else {
        format!("Homebrew could not install the update:\n{tail}")
    })
}

pub fn brew_update(brew: &Path) {
    let _ = Command::new(brew)
        .arg("update")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_COLOR", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn brew_cache_path(brew: &Path) -> Option<PathBuf> {
    let output = Command::new(brew)
        .args(["--cache", "--cask", "rustchamber"])
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

pub fn spawn_install(prepared: &PreparedInstall) -> Result<(), String> {
    if prepared.kind == InstallKind::Native {
        let staged = prepared
            .staged
            .as_deref()
            .ok_or_else(|| "The update is not ready to install.".to_string())?;
        let available = if cfg!(target_os = "macos") {
            staged.is_dir()
        } else {
            staged.is_file()
        };
        if !available {
            return Err("The downloaded update is no longer available. Download it again.".into());
        }
        let parent = prepared
            .bundle
            .parent()
            .ok_or_else(|| "Could not find the folder that holds RustChamber.".to_string())?;
        if !directory_writable(parent) {
            return Err(
                "The folder that holds RustChamber is not writable. Download the new version and replace it manually."
                    .into(),
            );
        }
    }
    let script = cache_dir().join("apply-update.sh");
    std::fs::create_dir_all(cache_dir())
        .map_err(|error| format!("Could not create the update folder: {error}"))?;
    let text = install_script(prepared);
    let mut file = std::fs::File::create(&script)
        .map_err(|error| format!("Could not write the update script: {error}"))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("Could not write the update script: {error}"))?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700));
    }
    Command::new("/bin/sh")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(&prepared.bundle)
        .arg(
            prepared
                .staged
                .as_deref()
                .map(Path::to_string_lossy)
                .unwrap_or_default()
                .to_string(),
        )
        .arg(&prepared.log)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start the update: {error}"))
}

fn install_script(prepared: &PreparedInstall) -> String {
    let replace = match prepared.kind {
        InstallKind::Native if cfg!(target_os = "macos") => {
            r#"
next="$bundle.updating"
old="$bundle.previous"
fail() {
  echo "$1"
  /usr/bin/open -n "$bundle" 2>/dev/null || true
  exit 1
}
rm -rf "$next"
echo "copying update to $next"
/usr/bin/ditto "$staged" "$next" || fail "could not copy the update"
/usr/bin/xattr -dr com.apple.quarantine "$next" 2>/dev/null || true
if [ -e "$bundle" ]; then
  rm -rf "$old"
  mv "$bundle" "$old" || fail "could not move the current app aside"
fi
if ! mv "$next" "$bundle"; then
  [ -e "$old" ] && mv "$old" "$bundle"
  fail "could not move the update into place"
fi
if [ -n "$staged" ]; then
  rm -rf "$(dirname "$staged")"
fi
rm -f "$(dirname "$log")"/*.zip "$(dirname "$log")"/*.part 2>/dev/null || true
"#
        }
        InstallKind::Native => {
            r#"
next="$bundle.new"
old="$bundle.previous"
fail() {
  echo "$1"
  nohup "$bundle" >/dev/null 2>&1 &
  exit 1
}
rm -f "$next"
echo "copying update to $next"
cp "$staged" "$next" || fail "could not copy the update"
chmod 755 "$next" || fail "could not make the update executable"
if [ -e "$bundle" ]; then
  rm -f "$old"
  mv "$bundle" "$old" || fail "could not move the current app aside"
fi
if ! mv "$next" "$bundle"; then
  [ -e "$old" ] && mv "$old" "$bundle"
  fail "could not move the update into place"
fi
rm -f "$old"
if [ -n "$staged" ]; then
  rm -rf "$(dirname "$staged")"
fi
rm -f "$(dirname "$log")"/*.tar.gz "$(dirname "$log")"/*.part 2>/dev/null || true
"#
        }
        _ => "",
    };
    let reopen = if cfg!(target_os = "macos") {
        r#"
if ! /usr/bin/open -n "$bundle"; then
  echo "could not reopen RustChamber"
  if [ -e "$bundle.previous" ]; then
    echo "restoring the previous version"
    rm -rf "$bundle"
    mv "$bundle.previous" "$bundle" && /usr/bin/open -n "$bundle" || true
  fi
fi
"#
    } else {
        r#"
nohup "$bundle" >/dev/null 2>&1 &
"#
    };
    format!(
        r#"#!/bin/sh
set -u
log="$4"
mkdir -p "$(dirname "$log")"
exec >>"$log" 2>&1
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) applying RustChamber update"
parent="$1"
bundle="$2"
staged="$3"
tries=0
while kill -0 "$parent" 2>/dev/null; do
  tries=$((tries + 1))
  if [ "$tries" -gt 600 ]; then
    echo "timed out waiting for RustChamber to exit"
    exit 1
  fi
  sleep 0.1
done
sleep 0.3
{replace}
{reopen}
rm -rf "$bundle.previous"
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) update finished"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_script_waits_for_the_app() {
        let prepared = PreparedInstall {
            kind: InstallKind::Native,
            bundle: PathBuf::from("/opt/RustChamber"),
            staged: Some(PathBuf::from("/tmp/stage/rchamber")),
            log: PathBuf::from("/tmp/log"),
        };
        let script = install_script(&prepared);
        assert!(script.contains("kill -0 \"$parent\""));
        assert!(script.contains("mv \"$next\" \"$bundle\""));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_script_replaces_the_bundle_on_macos() {
        let prepared = PreparedInstall {
            kind: InstallKind::Native,
            bundle: PathBuf::from("/Applications/RustChamber.app"),
            staged: Some(PathBuf::from("/tmp/stage/RustChamber.app")),
            log: PathBuf::from("/tmp/log"),
        };
        let script = install_script(&prepared);
        assert!(script.contains("ditto \"$staged\" \"$next\""));
        assert!(script.contains("open -n \"$bundle\""));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_script_replaces_the_executable_on_linux() {
        let prepared = PreparedInstall {
            kind: InstallKind::Native,
            bundle: PathBuf::from("/home/user/.local/bin/rchamber"),
            staged: Some(PathBuf::from("/tmp/stage/rchamber")),
            log: PathBuf::from("/tmp/log"),
        };
        let script = install_script(&prepared);
        assert!(script.contains("cp \"$staged\" \"$next\""));
        assert!(script.contains("chmod 755 \"$next\""));
        assert!(script.contains("mv \"$next\" \"$bundle\""));
        assert!(script.contains("nohup \"$bundle\""));
        assert!(!script.contains("ditto"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn homebrew_script_only_reopens_the_bundle() {
        let prepared = PreparedInstall {
            kind: InstallKind::Homebrew,
            bundle: PathBuf::from("/Applications/RustChamber.app"),
            staged: None,
            log: PathBuf::from("/tmp/log"),
        };
        let script = install_script(&prepared);
        assert!(!script.contains("ditto"));
        assert!(script.contains("open -n \"$bundle\""));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finds_the_app_inside_a_staging_folder() {
        let root = std::env::temp_dir().join(format!("rchamber-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("RustChamber.app/Contents")).unwrap();
        assert_eq!(find_app(&root), Some(root.join("RustChamber.app")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_the_binary_inside_a_staging_folder() {
        let root = std::env::temp_dir().join(format!("rchamber-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("RustChamber-0.2.6-beta-linux-x86_64");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("rchamber"), b"binary").unwrap();
        assert_eq!(find_binary(&root), Some(package.join("rchamber")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unpacks_and_verifies_a_linux_archive() {
        use std::os::unix::fs::PermissionsExt;

        let version = "9.9.9";
        let root = std::env::temp_dir().join(format!("rchamber-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join(format!("RustChamber-{version}-linux-x86_64"));
        std::fs::create_dir_all(&package).unwrap();
        let binary = package.join("rchamber");
        std::fs::write(
            &binary,
            format!("#!/bin/sh\necho \"RustChamber {version}\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let archive = root.join("update.tar.gz");
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&root)
            .arg(package.file_name().unwrap())
            .status()
            .unwrap();
        assert!(status.success());

        let staged = prepare_archive(version, &archive).unwrap();
        assert_eq!(staged.file_name().unwrap(), "rchamber");
        assert!(staged.is_file());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(cache_dir().join(format!("stage-{version}")));
    }
}
