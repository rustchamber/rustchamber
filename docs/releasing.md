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
git commit -m "RustChamber 0.0.1-alpha"
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
git tag -a v0.0.1-alpha -m "RustChamber 0.0.1-alpha"
git push origin v0.0.1-alpha
```

GitHub Actions builds a universal macOS binary (arm64 + x86_64), creates
`RustChamber.app`, signs it, and publishes:

- `RustChamber-0.0.1-alpha-macos-universal.dmg`
- `RustChamber-0.0.1-alpha-macos-universal.zip`

The DMG contains the app and an Applications shortcut. The ZIP is there for users who
prefer it. Versions with a prerelease suffix, such as `0.0.1-alpha`, are published as a
GitHub prerelease automatically.

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
gh workflow run homebrew.yml -f version=0.0.1-alpha
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

The script needs `jq`. Artifacts are written to `dist/`. It uses an ad-hoc signature unless
`CODESIGN_IDENTITY` is set.

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

## Adding Linux and Windows

The release workflow currently builds only macOS. To add the other targets, create jobs
with the same structure:

- **Linux** — build on `ubuntu-24.04`, install `libxkbcommon-dev` and the xcb development
  packages, then package the binary with the desktop entry, icon, and a `.tar.gz`.
- **Windows** — build on `windows-2025` and package `rchamber.exe` with the icon and a ZIP.

Keep the asset names parallel (`RustChamber-<version>-linux-x86_64.tar.gz`,
`RustChamber-<version>-windows-x86_64.zip`) so the release page stays readable.

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
- [ ] Homebrew cask updated by the `homebrew` job
- [ ] `brew install --cask rustchamber/tap/rustchamber` opens without a Gatekeeper prompt
