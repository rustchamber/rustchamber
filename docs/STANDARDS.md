# Engineering Standards

Standards and production motives used across RustChamber. Read with
[`architecture.md`](architecture.md).

## Product motives

- **Native first.** One Rust binary, `egui` UI, no webview, no Electron, no JavaScript
  runtime. Extensions are sandboxed Lua, not pages.
- **Local-first.** Projects, settings, drafts, and extension data stay on disk. The app
  reads OpenCode catalogs; it does not rewrite provider or agent configuration.
- **Fast idle.** No I/O on the UI thread. Repaint only when state changed. Idle window
  costs almost no CPU.
- **No silent behavior.** No silent updates, no silent corrupt-file overwrite, no silent
  data loss. Failed requests keep the last good data and surface a toast.
- **Explicit trust.** Every extension capability is approved. `exec` is a real shell.
  Permission mode is visible and per session.
- **Graceful degradation.** Missing routes, corrupt state, absent tools, and unsupported
  installs fall back to a readable state or a documented manual path.
- **User control.** Managed server can be replaced by an external endpoint. Package-manager
  installs are never self-updated in place.

## Workspace

Eight crates, strict downward dependency direction:

| Crate | Owns |
| --- | --- |
| `rchamber-api` | HTTP client, SSE, wire models, server lifecycle |
| `rchamber-git` | Native git CLI, porcelain parsing |
| `rchamber-term` | PTY sessions, VT emulator |
| `rchamber-update` | Release discovery, download, verify, self-install |
| `rchamber-ext` | Manifest, registry, approval, Lua sandbox, widget schema |
| `rchamber-app` | Actions, backend actor, state projection, stores |
| `rchamber-ui` | egui shell, chat, composer, panels, settings, theme |
| `rchamber` | Binary entry, `ext check` |

Rules:

- `rchamber-ui` may depend on app/git/term/update but not the reverse.
- `rchamber-api` knows nothing about UI or app state.
- Internal deps use path entries. External versions live in `[workspace.dependencies]`;
  crates inherit with `.workspace = true`.
- Public surface is re-exported from `lib.rs`. Modules stay implementation detail.
- Edition 2024, Rust 1.95, version from `[workspace.package]`.
- Dev profile: local crate `opt-level = 1`, dependencies `opt-level = 2`.

## Rust style

- `cargo fmt --all`; CI runs `--check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- No `unsafe`.
- Formatting is rustfmt default; no repo-specific style file.
- Prefer `let ... else`, `if let` chains, `is_some_and`, `matches!`, and early returns.
- Keep ownership local. Pass borrowed data unless the receiver stores or sends it.
- Name by domain noun or action verb. `Action` variants are verbs; `BackendEvent` variants
  are results. Types describe the thing, not the layer.
- Module names match the surface or concept (`composer`, `workspace`, `registry`).
- Cohesion beats file length. A large file is acceptable when it is one surface with one
  layout and one state owner.
- Global lint suppression is limited to `clippy::too_many_arguments` in `rchamber-ui`.
  Add targeted `#[allow]` only with a concrete reason; never blanket-allow.
- No debug prints in library code. Use `tracing` with `warn` for recoverable failure,
  `debug` for lifecycle detail, `info` for user-visible milestones.

## Errors

- Library crates expose typed errors with `thiserror`: `ApiError`, `GitError`,
  `TermError`, `ManifestError`, `RegistryError`, `RuntimeError`, `StorageError`.
- `Result<_, String>` is allowed at the user-facing update boundary and in host/worker
  bridges where the string is displayed.
- `anyhow` is for the API example only. App/UI code does not use it.
- Do not `unwrap` on runtime data. `expect` is reserved for a startup invariant, such as
  spawning the backend thread.
- Poisoned locks recover with `unwrap_or_else(|error| error.into_inner())`.
- Failed loads do not clear good state. Return empty/default only for the failed item.
- Errors shown to the user are complete sentences. Internal errors may be lowercase and
  technical.
- Remote failures that mean "feature absent" map to an empty result, not an error; real
  failures propagate.

## Concurrency and I/O

- UI thread never blocks on network, disk, subprocess, or extension work.
- One Tokio backend actor owns the client, SSE subscription, managed server, and file work.
- UI sends `Action` values. Backend sends `BackendEvent` values. No callbacks across the
  boundary.
- Backend wakes the UI through `request_repaint` only after a state change.
- Event bursts coalesce through a short flush window (350 ms), never one repaint per event.
- Every external operation has a timeout:
  - HTTP connect 6 s, request 90 s.
  - SSE idle watchdog 45 s; reconnect backoff 250 ms → 8 s.
  - Managed server readiness 45 s; external server 8 s.
  - Git 20 s; clone 15 min; kill the child and drain pipes on timeout.
  - Extension call 5 s in Lua, 30 s host reply.
- SSE parsing handles multi-line `data:`, comments, and partial chunks.
- Streaming deltas are ephemeral. `*.ended` events are authoritative; snapshots merge
  around live buffers instead of truncating them.
- Dropping the client or server handle kills the managed child (`kill_on_drop`, `Drop`).
- Long work runs off the UI thread: attachments, updates, extensions, git, PTY.

## State

- One authority per datum: server snapshot, live event, or local file. Do not cache a
  second copy.
- `AppModel` owns catalogs and session list. `Chat` owns transcript, queue, permissions,
  diff, and draft. Panels own their view state.
- Stale responses are discarded when the active session or project changed in flight.
- Sessions are project-scoped and paginated. Never import the whole OpenCode history.
- UI caches are keyed by `egui::Id` and stored in context memory. They are view state, not
  data authority.
- Time is milliseconds since epoch (`f64`) at boundaries; format at the edge.
- IDs are strings from the server or stable local forms (`dir:<path>`, `folder:<stamp>:<n>`).

## Serialization and persistence

- `serde` everywhere. Wire and extension formats use `camelCase`; enums use lowercase tag
  values.
- Every persisted field gets `#[serde(default)]` unless absence is an error. This keeps old
  files loadable.
- Skip empty optional fields on wire writes.
- Version persisted schemas. Bump the constant when meaning changes:
  `WORKSPACE_VERSION`, extension `API_VERSION`.
- Writes are atomic: unique temp file, write, `sync_all`, rename.
- Config directories are private on Unix (`0700`); secret-bearing files are `0600`.
- Process-wide save lock prevents concurrent workspace writes.
- Corrupt workspace moves aside. Future-version workspace is backed up, then defaults load.
- Drafts flush 500 ms after the last edit.
- Settings, workspace, drafts, and extension storage are separate files. Do not mix
  product state into one blob.
- Passwords are stored as plain text today. Document that; never imply encryption.

## UI

- Immediate mode. Own layout, focus, hit testing, and text measurement explicitly.
- Theme tokens only. Spacing, radius, type size, line height, color, motion, and glass
  opacity live in `theme.rs` / `theme_store.rs`; no ad-hoc literals in a surface.
  Semantic exceptions: terminal ANSI palette and composited overlays.
- Reuse `widgets.rs` primitives instead of hand-rolled controls.
- Text contrast is checked after alpha compositing. Body text targets ≥ 4.5:1; correct
  colors that fall below the floor.
- Interactions use the shared hit-target sizes. Hover and press states are animated with
  the shared motion constants.
- Honor reduced motion and animation time zero. Never require animation to understand
  state.
- Large views scroll, cache, and build lazily. Do not lay out every session, file, or
  message on every frame.
- Keyboard-first: command palette, rebindable shortcuts, arrow/Home/End menu navigation,
  Escape closes overlays.
- Use platform labels for platform actions. Guard macOS-only behavior with `cfg`.
- Empty, loading, error, and partial states are designed, not blank screens.

## Security

- Endpoint URLs accept `http`/`https`; plain HTTP only for loopback. Reject embedded
  credentials and non-loopback HTTP.
- Managed server binds `127.0.0.1`, uses a random password, and passes it through the
  environment, not argv.
- Workspace-scoped requests send both `x-opencode-directory` and the query parameter.
- File access is confined to the open project: canonicalize, reject `..`, reject symlinks,
  and require the result to start with the canonical base.
- Git runs with prompts disabled, pager disabled, stable locale, null stdin, capped output,
  and a timeout. Reject paths/names starting with `-` or containing NUL/newline. Redact
  credentials from error text.
- Prompt-injected markup is escaped so agent text cannot close the RustChamber fence.
- Extension packages: max 500 files, 40 MiB, reject unsafe archive paths.
- Extension runtime: 32 MiB memory, 5 s per call, instruction hook, no `require`, `io`,
  `os`, `package`, or bytecode loading.
- Extension storage: 2 MiB total, 64 KiB per value, 2,000 keys.
- Capabilities are semantic strings; network grants require an exact `https://` origin.
  All requested capabilities must be granted before the extension is active.
- Never store secrets in extension storage. Use the host integration path.
- Updates verify the staged binary (`--version`) before install. System packages,
  AppImage, read-only locations, and Flatpak installs link to the release instead of
  self-replacing.
- No embedded browser, JavaScript runtime, or iframe extension host.

## Extensions

- A package is a folder with `rchamber.json` plus a Lua entry.
- Contributions are declarative: panel, status section, mentions, commands, actions, tool
  renderers, blocks, file editors, instructions, events.
- The same `Widget` tree renders extension panels and agent `openui-lang` blocks. One
  renderer, one validation path.
- Limit contribution counts in the manifest and reject unknown capabilities.
- Re-validate installed packages on load. Ignore invalid roots outside the package folder.
- A Lua error is a toast. It does not crash the app or uninstall the extension.
- Each enabled extension gets its own worker thread and Lua state. Slow extension cannot
  freeze the UI.

## Platform integration

- Platform code lives behind `#[cfg(target_os = ...)]` in a small module or block.
- macOS: full-size content view, app icon, DMG/ditto paths, quarantine handling.
- Linux: XDG portal for system theme, host shell fallback, distribution packaging.
- Windows: compiles and checks; updates open the release page.
- Flatpak: host tools (`opencode`, `git`, shell, `xdg-open`) go through `flatpak-spawn`;
  the in-app updater is disabled; host `HOME` and snippet paths are resolved explicitly.
- Missing external tools produce a clear message with the expected name, path, or install
  command.

## Testing and CI

- Tests live in the module they exercise under `#[cfg(test)] mod tests`.
- Names describe behavior: `activating_a_session_opens_and_selects_its_tab`, not
  `test_activate`.
- Prefer pure unit tests for parsing, projection, layout math, key encoding, theme
  resolution, and path safety.
- Use `tempfile` for filesystem tests. Do not touch the real config directory.
- Host-dependent tests (PTY timing) are `#[ignore]`; default suite stays reliable.
- CI gate: `cargo fmt --all --check`, clippy with `-D warnings`, `cargo test --workspace`.
- CI also runs `cargo check --workspace --all-targets` on Linux, macOS, and Windows.
- Shell scripts are syntax-checked with `bash -n` / `sh -n`.
- `RCHAMBER_SCREENSHOT=/path/out.png` captures a settled frame off-screen for docs.

## Documentation, commits, releases

- README is the product surface: install, capabilities, architecture, limitations.
- `architecture.md` owns runtime shape and data flow. `extensions.md` owns the
  extension contract. `flatpak.md` and `releasing.md` own packaging detail.
- Prose is terse, concrete, and outcome-first. Sentence per line keeps diffs small.
- Document why a workaround exists, not what the next line does.
- CHANGELOG entries are user-visible outcomes, grouped by version. Add the section before
  tagging.
- Release checklist is mandatory: version bump, changelog, screenshots, fmt/clippy/test,
  artifacts, glibc floor, Flatpak sources, Homebrew cask.
- Asset names are stable: `RustChamber-<version>-<platform>-<arch>.<ext>`.
- Linux ships from a pinned Ubuntu 20.04 container; fail if the binary needs glibc newer
  than 2.31.
- Flatpak manifests pin tags; regenerate `cargo-sources.json` from `Cargo.lock`; add the
  AppStream release entry.
- Never commit `target/`, `dist/`, or local reference checkouts.

## Definition of done

- [ ] Behavior is in the right crate; dependency direction unchanged.
- [ ] No UI-thread blocking; timeouts and cancellation exist.
- [ ] Failure keeps good state and reports clearly.
- [ ] Persisted changes are backward-compatible and atomic.
- [ ] Untrusted input is validated, confined, or rejected.
- [ ] UI uses theme tokens, shared widgets, and reduced-motion paths.
- [ ] Tests cover the new behavior; ignored tests are justified.
- [ ] `cargo fmt --all --check`, clippy `-D warnings`, and workspace tests pass.
- [ ] Docs and CHANGELOG updated when the contract or behavior changed.
