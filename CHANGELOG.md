# Changelog

## 0.2.9 - 2026-11-09

- Native extensions: a folder with `rchamber.json` and Lua adds panels, status sections,
  `@` mentions, slash commands, message actions, tool renderers and chat blocks. Install by
  folder, zip or https archive, review one capability list, and disable without losing data.
- `/new-extension` teaches the agent the native API and ends with an install link that opens
  the approval sheet. `rchamber ext check <folder>` validates a package from the terminal.
- Native `openui-lang` blocks in chat: tables, charts, stats, progress, steps, choices,
  inputs, sliders and buttons draw with the app's own theme.
- Shell code blocks in a finished reply have a Run button and show their output in place.
- Native read-only file viewers for SQLite, Parquet, ZIP, Jupyter, XLSX/ODS, DOCX and PPTX;
  CSV exports to Excel.
- Settings → Shortcuts is searchable and every listed command can be rebound or cleared.
- Chat text size is independent from the interface scale.
- Session totals aggregate every assistant message, next to the existing turn stats.
- New Patina light and dark themes.
- Session ids in a message open the session; Shift+Enter continues a numbered list; a
  message can be quoted with “Looks good” in one click.
- Extension tool calls show their declared title, icon and table/code output.
- Extension panels open automatically after approval, use the puzzle-piece icon, and share
  the close and maximize controls of every other panel. Invalid panel trees show a readable
  error instead of a loading spinner, and JSON nulls from the host reach Lua as nil.

## 0.2.8

- Prefer the OpenCode 2 CLI over an older OpenCode 1 installation. OpenCode 2 is required by the current API; a version 1 binary now produces a clear error instead of an HTTP 400.
- The Flatpak wrapper also prefers the OpenCode 2 binary bundled with a compatible desktop app when present.

## 0.2.7

- Added a Flatpak manifest for Flathub. The sandbox runs the host `opencode`,
  `git`, shell, and file opener through `flatpak-spawn`, so projects, credentials,
  and the user's toolchain keep working.
- The `System` theme now reads the desktop color scheme from the XDG portal on
  Linux, fixing light mode on Plasma and GNOME when the desktop is dark.
- Flatpak installs no longer check for GitHub updates; they show the
  `flatpak update` command instead.
- Snippets are also loaded from the host's `~/.config/opencode/snippets` inside a
  Flatpak sandbox, and the terminal uses the host login shell.
- Linux packages are now built inside a pinned Ubuntu 20.04 container, so the
  published binary only needs glibc 2.31. It runs on Ubuntu 20.04+ (and 22.04/24.04),
  Debian 11+, Fedora 32+, RHEL 9, openSUSE Leap and current Arch, instead of
  only distributions as new as the GitHub runner.
- Native packages are published next to the tarball: `.deb` (Debian/Ubuntu),
  `.rpm` (Fedora/RHEL/openSUSE) and `.pkg.tar.zst` (Arch), with the desktop
  entry, icons and runtime dependencies declared.
- The tarball installer now checks X11/Wayland/OpenGL libraries and prints the
  exact install command for the detected distribution, supports `--uninstall`,
  and refreshes the KDE menu cache.
- The OpenCode CLI is also discovered in `~/.local/bin`, `~/.bun/bin`,
  `/opt/opencode/bin` and other common locations, which fixes launches from the
  application menu where `PATH` is shorter than in a terminal.
- HEIC conversion through `ffmpeg` now passes `-i`, and the terminal falls back
  to `/bin/sh` when `/bin/bash` is not installed.

## 0.2.6-beta

- Linux is now a published target: the release workflow builds a portable
  `RustChamber-<version>-linux-x86_64.tar.gz` with the desktop entry, hicolor icons,
  AppStream metadata, and an `install.sh` that installs to `~/.local` without sudo.
- In-app updates work on Linux for standalone installs: the new executable is verified by
  running `--version`, staged, and swapped in on restart. AppImage and system-directory
  installs open the release page instead of trying to overwrite read-only files.
- Update checks now select the asset for the running platform instead of always looking for
  the macOS ZIP, and the release-feed fallback knows about Linux builds.
- Project and file-manager labels follow the platform, and the project app menu no longer
  lists macOS-only applications on Linux.
- HEIC attachments fall back to ImageMagick, `heif-convert`, or `ffmpeg` when `sips` is not
  available.
- Newer OpenCode servers are detected through `/global/health` when `/api/info` is absent,
  and the managed server receives both the legacy and current password variables.
- The binary accepts `--version`/`-V` and prints the packaged version.

## 0.2.5-beta

- A fresh install starts with no projects. Add a project by folder, clone a repository, or
  import one from OpenCode; only sessions created after that are loaded.
- New sessions open a centered composer with a project/Chat selector and per-project drafts.
- Sessions stay in "In progress" after a turn ends until you mark them done. Running rows
  show a slow spinner and an elapsed counter, and hovering reveals mark-done, archive, and
  shift-to-delete actions.
- The composer shield switches the session between ask-always and allow-all permissions.
- Session tabs were replaced by the session title and project subtitle in the header, with a
  full session action menu.
- Fixed switching a model to the default effort, which sent `variant: null` and failed with
  HTTP 400.
- Added pointer cursors to menu rows, settings navigation, palette entries, and other
  buttons that were missing them.
- Security: settings, workspace and draft files are written atomically with private
  permissions; project reveals use Finder reveal instead of launching app bundles; clone
  URLs reject HTTP and embedded credentials; git output is bounded.

## 0.2.4-beta

- The update button now also shows the current percentage while downloading or installing.

## 0.2.3-beta

- Homebrew updates now download the DMG with a real percentage and progress bar, then install from the Homebrew cache.
- The update dialog shows the current step (Updating Homebrew, Downloading, Installing) and the percentage.

## 0.2.2-beta

- The update check now falls back to the GitHub releases Atom feed when the API is rate-limited, so the sidebar arrow still appears.
- Every check writes its result to `~/Library/Caches/rustchamber/updates/update.log`.

## 0.2.1-beta

- The Dock now shows the RustChamber icon. eframe was replacing the bundle icon with its default icon at launch.

## 0.2.0-beta

- In-app updates: RustChamber checks GitHub Releases at launch and every hour, shows a
  blue download arrow in the sidebar, opens the release notes, downloads in the background,
  and installs on restart.
- Homebrew installs update through `brew upgrade --cask rustchamber`; DMG installs replace
  the app bundle in place and reopen the new version.

## 0.1.0-beta

First beta release. The alpha and beta share the same feature set; this version marks the
first build we consider ready for daily use.

- The settings window now paints above the composer, so the input no longer covers the dialog.
- README screenshots refreshed in the RustChamber light theme, with a new git panel shot.

## 0.0.2-alpha

- The settings window now paints above the composer, so the input no longer covers the dialog.
- README screenshots refreshed in the RustChamber light theme, with a new git panel shot.

## 0.0.1-alpha

First public alpha build.

- Native egui desktop client for OpenCode over HTTP and SSE.
- Managed `opencode serve` lifecycle with automatic port, password, and reconnect.
- Streaming chat with reasoning, tool cards, permission prompts, and queued prompts.
- Session list, search, create, rename, archive, and delete.
- Model, effort, and agent pickers.
- Composer with file, folder, snippet, skill, and command mentions.
- Attachments with HEIC conversion and Office/ODF text extraction.
- Files panel with tree browsing, editor, save, and large-file guard.
- Session diff and native git panel with stage, commit, branch, log, push, and pull.
- PTY terminal with tabs and a VT emulator.
- Theme engine with light and dark pairs, bundled fonts, density, and typography controls.
- Command palette, keyboard shortcuts, and a settings window.
