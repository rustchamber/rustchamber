# Changelog

## Unreleased

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
