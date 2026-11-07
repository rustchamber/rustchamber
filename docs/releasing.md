# Releasing

## First push to GitHub

Create an empty repository named `rustchamber` in the `rustchamber` GitHub organization. Do
not initialize it with a README, license, or `.gitignore`; the local tree already has those.

```bash
cd rustchamber
git init
git add .
git status
```

Check the staged file list before committing. `labs/` and `target/` are ignored by
`.gitignore`; neither belongs in the repository. `labs/` is a local reference checkout of
another project and is about 395 MB.

```bash
git commit -m "RustChamber 0.1.0-beta"
git branch -M main
git remote add origin git@github.com:rustchamber/rustchamber.git
git push -u origin main
```

The repository URL in `Cargo.toml`, the About window, and the README points to
`rustchamber/rustchamber`. If the repository moves to another account or name, update those
three places.

For the repository page:

- Description: `Native Rust desktop workspace for OpenCode`
- Topics: `rust`, `egui`, `opencode`, `desktop`, `developer-tools`, `terminal`, `git`
- Social preview: `packaging/macos/AppIcon.png`
- Enable Releases and Actions in the repository settings.

## Cut a release

The release workflow runs on any `v*` tag and on manual dispatch.

```bash
git tag -a v0.1.0-beta -m "RustChamber 0.1.0-beta"
git push origin v0.1.0-beta
```

GitHub Actions builds a universal macOS binary (arm64 + x86_64) and a Linux x86_64
tarball, then publishes:

- `RustChamber-0.1.0-beta-macos-universal.dmg`
- `RustChamber-0.1.0-beta-macos-universal.zip`
- `RustChamber-0.1.0-beta-linux-x86_64.tar.gz`

The DMG contains the app and an Applications shortcut. The ZIP is there for users who
prefer it. The Linux tarball contains the `rchamber` binary, a desktop entry, hicolor
icons, AppStream metadata, and `install.sh`. Versions with a prerelease suffix, such as
`0.1.0-beta`, are published as a GitHub prerelease automatically.

Before tagging, bump the workspace version in `Cargo.toml` and add the matching section to
`CHANGELOG.md`.

## Homebrew tap

The cask lives in the [rustchamber/homebrew-tap](https://github.com/rustchamber/homebrew-tap)
repository at `Casks/rustchamber.rb`. Users install with:

```bash
brew install --cask rustchamber/tap/rustchamber
```

After the release is published, the `homebrew` job in the release workflow updates the
cask version and `sha256` and pushes the change to the tap. It authenticates with the
`HOMEBREW_TAP_DEPLOY_KEY` secret, an SSH deploy key with write access to the tap
repository. Rotate it by generating a new key, adding the public half under the tap's
Settings → Deploy keys, and storing the private half in the secret.

To update the cask without cutting a new release, run the workflow manually:

```bash
gh workflow run homebrew.yml -f version=0.1.0-beta
```

The cask runs a `postflight_steps` step that removes `com.apple.quarantine` from the
installed app.
Builds are not notarized, so this is what keeps Homebrew installs from showing the
Gatekeeper dialog. Remove that step once the project ships a notarized Developer ID build.

## Build a release locally

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash scripts/package-macos.sh
```

The macOS script needs `jq`. Artifacts are written to `dist/`. It uses an ad-hoc signature
unless `CODESIGN_IDENTITY` is set.

On Linux, the packaging script builds for the host by default:

```bash
bash scripts/package-linux.sh
```

It writes `dist/RustChamber-<version>-linux-<arch>.tar.gz`. When nfpm is on `PATH` (or
`NFPM=/path/to/nfpm` is set) it also writes the `.deb`, `.rpm`, and Arch `.pkg.tar.zst`
packages. Setting `PACKAGES=1` makes a missing nfpm an error; `PACKAGES=0` builds only the
tarball. The asset name follows `ARCH`, which defaults to the host architecture, or to
`TARGET` when that is set. For a cross build, set `TARGET=aarch64-unknown-linux-gnu` and
install that Rust target and linker. The script needs `jq` and `tar`; nfpm is required for
the native packages, and the build needs the system libraries listed in the README.

To produce the portable release exactly as CI does (glibc 2.31, packages included):

```bash
bash scripts/package-linux-container.sh
```

This needs podman or docker, downloads `ubuntu:20.04` and nfpm, and refuses to finish if
the resulting binary needs a glibc newer than 2.31. Override the image with
`LINUX_BUILD_IMAGE` when testing a different floor.

## Signing and notarization

An ad-hoc signature is enough to run locally, but macOS still warns on download. For a
warning-free download, the repository needs an Apple Developer account and these Actions
secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 of the exported Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `CODESIGN_IDENTITY` | Full identity, for example `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` | Apple account email |
| `APPLE_PASSWORD` | App-specific password for notarization |
| `APPLE_TEAM_ID` | Ten-character team identifier |

The workflow imports the certificate into a temporary keychain, signs with the hardened
runtime, notarizes the app and the disk image with `notarytool`, and staples both tickets.
If the secrets are absent, the workflow still publishes an ad-hoc signed build.

Ad-hoc builds are quarantined by macOS after a direct download. Users can open them through
System Settings → Privacy & Security, or remove the flag:

```bash
xattr -dr com.apple.quarantine /Applications/RustChamber.app
```

Homebrew installs skip that dialog because the cask removes the quarantine attribute after
installation. The direct DMG still needs notarization to open cleanly on first launch.

To export the certificate:

```bash
base64 -i DeveloperID.p12 | pbcopy
```

Paste the result into the `APPLE_CERTIFICATE` secret.

## Linux packages

The release workflow has three jobs:

1. `macos` builds the universal app, signs and notarizes it when the Apple secrets exist,
   and uploads the DMG and ZIP as workflow artifacts.
2. `linux` runs `scripts/package-linux-container.sh` on `ubuntu-24.04`. That script starts
   an `ubuntu:20.04` container, installs the X11/Wayland/OpenGL build dependencies, builds
   the binary, and runs nfpm to produce the tarball, the `.deb`, the `.rpm`, the Arch
   `.pkg.tar.zst`, and a `.sha256` file. The container pins the glibc floor at 2.31; the
   script fails if the binary references a newer `GLIBC_*` symbol. Building on the runner
   directly would inherit glibc 2.39 and break Ubuntu 22.04, Debian 12, RHEL 9, and other
   supported systems.
3. `publish` runs after both succeed, creates the GitHub release as a draft, uploads every
   artifact, and publishes it. Keeping the release creation in one job avoids two builds
   racing to create the same tag.

The published Linux assets are:

```
RustChamber-<version>-linux-x86_64.tar.gz
RustChamber-<version>-linux-x86_64.deb
RustChamber-<version>-linux-x86_64.rpm
RustChamber-<version>-linux-x86_64.pkg.tar.zst
RustChamber-<version>-linux-x86_64.sha256
```

The `.deb` targets Debian/Ubuntu (glibc 2.31, `libxkbcommon`, `libxcb`, `libgl1`,
`libwayland-client0`, and the X11 cursor/randr/xi libraries). The `.rpm` uses soname
requirements so it resolves on Fedora, RHEL, and openSUSE. The Arch package uses native
package names. All three install `/usr/bin/rchamber`, the desktop entry, the icons, and the
AppStream metadata, and refresh the desktop/icon caches in the package scripts.

The tarball layout is:

```
RustChamber-<version>-linux-x86_64/
├── rchamber
├── install.sh
├── check-deps.sh
├── README.md
├── LICENSE
├── NOTICE
└── share/
    ├── applications/dev.rustchamber.RustChamber.desktop
    ├── icons/hicolor/{128x128,256x256,512x512}/apps/dev.rustchamber.RustChamber.png
    └── metainfo/dev.rustchamber.RustChamber.metainfo.xml
```

`install.sh` installs under `$PREFIX` (default `$HOME/.local`), rewrites the desktop
entry's `Exec` line to the absolute binary path so the application menu works without
`~/.local/bin` on the session `PATH`, and runs `check-deps.sh`, which prints the
distribution-specific install command for any missing X11/Wayland/OpenGL library. Pass
`--uninstall` to remove the installed files while keeping settings.

The in-app updater treats a writable standalone binary as a native install. It downloads
the tarball, unpacks it in the cache directory, runs the staged binary with `--version` to
confirm the release, then swaps the executable after the app exits and relaunches it. An
AppImage sets `APPIMAGE`, and a binary installed in a read-only directory (including
`/usr/bin/rchamber` from a package), is reported as an external install: the update dialog
opens the release page instead of attempting a write.

Builds for other architectures use the same asset naming scheme
(`RustChamber-<version>-linux-aarch64.*`) and need a matching `linux` matrix entry and an
aarch64 build container. The package script maps `x86_64` to `amd64` and `aarch64` to
`arm64` for nfpm.

## Flatpak and Flathub

The Flatpak manifest lives in `packaging/flatpak/`. `dev.rustchamber.RustChamber.yml` is the
Flathub manifest; `dev.rustchamber.RustChamber.local.yml` builds the current checkout. The
latter is what the `Flatpak` workflow builds.

For each release:

1. Bump `RCHAMBER_VERSION` in both manifests and add the tag/commit to the Flathub
   manifest.
2. Regenerate `packaging/flatpak/cargo-sources.json` from `Cargo.lock`.
3. Add the matching `<release>` entry to the AppStream metainfo.
4. Run `flatpak-builder --user --install --force-clean ...local.yml` and test the app.
5. Update the Flathub repository with a PR containing the new manifest/tag/commit.
6. Flathub builds and hosts the update; the app itself must not self-update inside the
   sandbox.

The app ID is `dev.rustchamber.RustChamber` and the project owns `rustchamber.dev`.
`--filesystem=home` covers normal projects; `--talk-name=org.freedesktop.Flatpak` is
needed for the host OpenCode, git, shell, and file opener. See [`flatpak.md`](flatpak.md)
for the full local build and submission instructions.

Windows is still unsupported. Adding it means a `windows-2025` job that packages
`rchamber.exe` with the icon and a ZIP, plus a `windows_zip` arm in the updater.

## Checklist

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Version updated in `Cargo.toml`
- [ ] `CHANGELOG.md` updated
- [ ] Screenshots current
- [ ] `git status` contains no `labs/`, `target/`, or `dist/`
- [ ] Tag pushed
- [ ] Release assets present and the DMG opens
- [ ] `RustChamber-<version>-linux-x86_64.tar.gz` extracts, `./rchamber --version` matches,
      and `./install.sh` adds a desktop entry
- [ ] The Linux binary reports `GLIBC_2.31` or older as its highest `GLIBC_*` symbol
- [ ] `.deb`, `.rpm`, and `.pkg.tar.zst` install `/usr/bin/rchamber` and the desktop entry
- [ ] `check-deps.sh` names the right install command on Debian, Fedora, Arch, and openSUSE
- [ ] `packaging/flatpak/cargo-sources.json` regenerated from `Cargo.lock`
- [ ] `flatpak-builder` builds `dev.rustchamber.RustChamber.local.yml`
- [ ] The Flatpak app starts, connects to host OpenCode, and opens the host shell/git
- [ ] Flathub manifest uses the new tag and commit
- [ ] Homebrew cask updated by the `homebrew` job
- [ ] `brew install --cask rustchamber/tap/rustchamber` opens without a Gatekeeper prompt
