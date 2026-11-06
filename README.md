<p align="center">
  <img src="packaging/macos/AppIcon.png" width="88" alt="RustChamber icon">
</p>

# RustChamber

A native desktop workspace for [OpenCode](https://opencode.ai), written in Rust with
`egui`. There is no webview, no Electron, and no JavaScript runtime.

RustChamber talks to the OpenCode server directly over HTTP and Server-Sent Events. Chat,
terminal, files, changes, and git live in the same window, with a keyboard-first interface
built for long sessions. Projects are explicit: a fresh install starts empty, and only the
projects and sessions you create from RustChamber are shown.

[rustchamber.lipecode.com](https://rustchamber.lipecode.com) — download, support, and roadmap.

![RustChamber home screen](docs/screenshots/home.png)

## Install

### Homebrew

```bash
brew install --cask rustchamber/tap/rustchamber
```

The cask installs the universal build and clears the macOS quarantine flag, so the app
opens without the "Apple could not verify" dialog. If you already installed RustChamber
from the DMG, adopt it once with
`brew install --cask --adopt rustchamber/tap/rustchamber`. Upgrade with
`brew upgrade --cask rustchamber`; remove settings as well with
`brew uninstall --zap --cask rustchamber`.

### Direct download

macOS builds are published on the [Releases](https://github.com/rustchamber/rustchamber/releases)
page as a universal disk image for Apple Silicon and Intel.

1. Download `RustChamber-<version>-macos-universal.dmg`.
2. Open the image and drag RustChamber to Applications.
3. Install the [OpenCode CLI](https://opencode.ai/docs) and make sure `opencode` is on your
   `PATH`.

Builds are ad-hoc signed unless Apple Developer credentials are configured. A direct
download is quarantined by macOS, so the first launch is blocked; open System Settings →
Privacy & Security and choose **Open Anyway**. If the app is reported as damaged, clear the
quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/RustChamber.app
```

### Updates

RustChamber checks the latest GitHub release a few seconds after launch and every hour
after that. When a newer version exists, a blue download arrow appears at the bottom of the
sidebar. It opens the release notes, downloads the update in the background while you keep
working, and offers **Restart to update** when the download finishes.

The update path follows how RustChamber was installed. A Homebrew install runs
`brew upgrade --cask rustchamber`; a DMG install replaces the app bundle in place and
reopens the new version. If the app runs from a mounted disk image or a folder it cannot
write to, the dialog links to the release instead.

Linux and Windows packages are not published yet. Both platforms build from source.

## Build from source

Requirements: Rust 1.95+, a C toolchain, and the `opencode` CLI.

```bash
git clone https://github.com/rustchamber/rustchamber.git
cd rustchamber
cargo run -p rchamber --release
```

On Debian or Ubuntu:

```bash
sudo apt install libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

To build the macOS app bundle and disk image locally:

```bash
bash scripts/package-macos.sh
```

This needs `jq` and both Apple targets. The output lands in `dist/`.

## What it does

- **Chat** — streaming Markdown, reasoning, tool and shell cards, plans, permission prompts,
  errors, retries, and completion times.
- **Projects** — a local project registry with an add-project modal, folder browsing,
  repository clone, and import from OpenCode. Closing a project keeps its sessions.
- **Sessions** — project-scoped lists, a centered new-session composer with a project or
  Chat target, in-progress tracking, mark done, pin, folders, search, archive, and delete.
- **Permissions** — a per-session shield that switches between asking for every tool and
  allowing everything. New sessions start on the safer ask mode.
- **Composer** — project targeting, `@` files, `/` commands, `$` skills, `#` snippets,
  pasted images, and a queue while the agent is busy.
- **Attachments** — files, images sent inline, HEIC converted to JPEG, and Office/ODF
  documents unpacked into text plus embedded images.
- **Files** — project tree, viewer, editor, find, save, and a guard for large files.
- **Changes** — session diff and per-turn diffs with file statistics.
- **Git** — status, stage, unstage, discard, commit, branches, log, fetch, pull, and push.
- **Terminal** — a real PTY with tabs, colors, scrollback, alternate screen, and resize.
- **Appearance** — light and dark theme pairs, bundled Inter, JetBrains Mono, and Instrument
  Serif, plus density, type size, and motion controls.
- **Command palette** — sessions, commands, and settings from `⌘K`.

![RustChamber chat with session context](docs/screenshots/chat.png)

![RustChamber git panel with an unstaged README change](docs/screenshots/git.png)

## Settings and appearance

![RustChamber appearance settings](docs/screenshots/settings.png)

![RustChamber about window](docs/screenshots/about.png)

## Connecting to OpenCode

By default RustChamber starts a managed `opencode serve` process on a free loopback port with
a random password, then connects to it. The process is stopped when the app exits.

To use an existing server, open **Settings → General** and set the endpoint and password.
The same values can be written to the settings file:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/rustchamber/settings.json` |
| Linux | `~/.config/rustchamber/settings.json` |
| Windows | `%APPDATA%\rustchamber\settings.json` |

The endpoint password is stored in that file as plain text.

### Workspace data

Project selection, folders, display preferences, new-session drafts, and the last active
project live in a separate workspace file:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/rustchamber/workspace.json` |
| Linux | `~/.config/rustchamber/workspace.json` |
| Windows | `%APPDATA%\rustchamber\workspace.json` |

Writes are atomic. Remove the file while RustChamber is closed to start again with no
projects. Sessions themselves remain in OpenCode; RustChamber only shows sessions created
after a project was added with the current installation.

### Permission modes

The shield in the composer toggles the active session between **ask always** and
**allow all**. The choice is written to OpenCode's session permission rules, so it survives
restarts and is visible to other clients. A new session inherits the last mode chosen in the
new-session composer; the default is ask.

Useful environment variables:

| Variable | Purpose |
| --- | --- |
| `RCHAMBER_OPENCODE_BIN` | Path to the `opencode` binary |
| `RCHAMBER_WORKSPACE` | Workspace directory for this run |
| `RCHAMBER_THEME` | `dark`, `light`, or `system` |
| `RCHAMBER_SURFACE` | Open `files`, `git`, `changes`, `terminal`, or `context` |
| `RCHAMBER_ZONE` | Dock the panel `open` (right) or `bottom` |
| `RCHAMBER_OPEN_SESSION` | Open a session id at startup |
| `RCHAMBER_UPDATE_FEED` | Override the releases API used by the update check |
| `OPENCODE_PASSWORD` | Reuse an explicit server password |

Snippets are Markdown files in `~/.config/opencode/snippets` or
`<workspace>/.opencode/snippets`. Optional frontmatter adds `aliases` and a `description`;
`#name` in the composer expands the snippet before sending.

## Keyboard

| Action | macOS | Linux / Windows |
| --- | --- | --- |
| Command palette | `⌘K` | `Ctrl+K` |
| New session | `⌘N` | `Ctrl+N` |
| Previous / next session | `⌘⌥←` / `⌘⌥→` | `Ctrl+Alt+←` / `Ctrl+Alt+→` |
| Model selector | `⌘⇧M` | `Ctrl+Shift+M` |
| Toggle sidebar | `⌘B` | `Ctrl+B` |
| Toggle terminal | `⌘J` | `Ctrl+J` |
| Close tab | `⌘W` | `Ctrl+W` |
| Save file | `⌘S` | `Ctrl+S` |
| Settings | `⌘,` | `Ctrl+,` |
| Close overlay | `Esc` | `Esc` |

## Architecture

The UI thread never performs I/O. Screens emit `Action` values to a Tokio backend actor,
which owns the OpenCode client, the SSE stream, and the managed server process. The backend
projects results back as `BackendEvent` values; the UI reduces them into state and repaints
on demand.

| Crate | Responsibility |
| --- | --- |
| `rchamber-api` | Typed OpenCode client: auth, sessions, messages, models, permissions, files, VCS, PTY, SSE |
| `rchamber-app` | Actions, backend actor, event projection, streaming reducer, project/session store, drafts, attachments, snippets |
| `rchamber-ui` | egui shell, chat, composer, panels, settings, theming |
| `rchamber-git` | Native git operations with timeouts and porcelain parsing |
| `rchamber-term` | PTY session and VT emulator |
| `rchamber-update` | Release feed, background download, signature checks, and self-install |
| `rchamber` | Binary entry point |

More detail is in [`docs/architecture.md`](docs/architecture.md).

## Design notes

The interface is immediate mode. That keeps dense, virtualized views cheap and removes the
DOM, but it also means the app owns layout, focus, text measurement, and hit testing. The
backend/UI split keeps network latency out of the frame loop.

A webview would have been faster to ship. The goal here was a native binary with no
JavaScript runtime, a real PTY, and direct control over how diffs and terminal cells are
drawn.

## Status

macOS is the tested target. Linux and Windows compile in CI and are expected to work, but
they have not received the same hands-on testing.

Known limits:

- No embedded web preview, plugin manager, or MCP configuration editor.
- Providers, agents, commands, and skills are shown read-only, as reported by OpenCode.
- AI session renaming, Mini Chat, and worktree moves are visible in menus but disabled
  until their backends exist.
- Linux and Windows builds check for updates but open the release page instead of installing.
- An OpenCode CLI must be installed; the app does not bundle it.

## License

MIT. See [`LICENSE`](LICENSE). Third-party attribution is recorded in [`NOTICE`](NOTICE).
