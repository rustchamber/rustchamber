# Architecture

## Shape

RustChamber is a single native process. The UI runs on the main thread with `egui`; a Tokio
actor runs the network, server process, and file work. The two sides exchange values, not
callbacks:

```
UI thread                         Backend thread
─────────                         ──────────────
AppModel  ──── Action ───────────▶ actor loop
   ▲                                  │
   └──────── BackendEvent ◀───────────┘
                                      │
                         OpenCode HTTP + SSE
```

The UI never blocks on I/O. The backend wakes the UI with `request_repaint` only when
something changed, so an idle window costs almost no CPU.

## Crates

| Crate | Responsibility |
| --- | --- |
| `rchamber-api` | HTTP client, SSE subscription, typed wire models, server process lifecycle |
| `rchamber-git` | Git CLI wrapper with timeouts and porcelain parsing |
| `rchamber-term` | PTY sessions and a VT emulator |
| `rchamber-app` | Action and event types, backend actor, state projection, drafts, attachments, snippets |
| `rchamber-ui` | egui shell, chat, composer, panels, settings, theme engine |
| `rchamber` | Binary entry point |

Dependencies point downward: `rchamber` depends on `rchamber-ui`; the UI depends on
`rchamber-app`, `rchamber-git`, and `rchamber-term`; the app depends on `rchamber-api`.
`rchamber-api` has no knowledge of the UI.

## Server lifecycle

With no endpoint configured, the backend looks for the `opencode` binary in `PATH` and in
common install locations, then starts:

```
opencode serve --hostname 127.0.0.1 --port <free> --env OPENCODE_PASSWORD=<random>
```

It reads stdout and stderr into the log, polls `/api/info` until the server answers (45 s
budget), and keeps the child handle tied to the client. Dropping the client kills the child.
When an external endpoint is configured, no process is spawned and the password comes from
the settings file or `OPENCODE_PASSWORD`.

Authentication is HTTP Basic with the username `opencode`. Requests scoped to a workspace
also send `x-opencode-directory` and a `directory` query parameter.

## Events

`GET /api/event` streams Server-Sent Events. The parser handles multi-line `data:` frames,
comment keep-alives, and partial chunks. A 45 s idle watchdog treats a silent connection as
dead. Reconnects use exponential backoff from 250 ms to 8 s, and every successful
`server.connected` triggers a catalog refresh.

Events are projected into a small `LiveEvent` enum in `rchamber-app/src/live.rs`. The UI
reducer then applies them to state. Streaming text and reasoning deltas are ephemeral;
`*.ended` events carry the authoritative text. Terminal execution events request a snapshot
refresh.

Snapshots can arrive while a response is still streaming. `merge_messages` keeps the local
live buffers for assistant messages that are still running and merges the authoritative
snapshot around them, so a slow refresh cannot truncate the visible answer.

Failed requests do not clear good data. The backend emits a toast and keeps the previous
catalog. Stale session loads are discarded when the active session changed while a request
was in flight.

## State

`AppModel` owns the session list, catalogs, settings draft, and active `Chat`. A `Chat` owns
the transcript, pending permissions, diff, queue, and its own draft. Panels own their view
state and caches. There is exactly one authority for each piece of data: a server snapshot,
a live event, or a local file.

Settings and drafts are stored as JSON:

- macOS: `~/Library/Application Support/rustchamber/`
- Linux: `~/.config/rustchamber/`
- Windows: `%APPDATA%\rustchamber\`

Drafts are flushed 500 ms after the last edit. Window and panel geometry live in the same
settings file.

## Composer

The composer keeps a single text buffer and renders mentions inline. `@` lists files and
folders from the workspace, `/` lists server commands, `$` lists skills, and `#` lists
snippets. Attachments are prepared on a worker before the prompt is sent:

- images below 10 MiB are sent inline as base64;
- larger files and directories are passed as `file://` URIs;
- HEIC/HEIF is converted to JPEG;
- Office and ODF archives are unpacked into text and embedded images.

Prompts sent while the agent is busy are queued locally and can be reordered or edited
before delivery.

## Surfaces

- **Files** builds the tree lazily, reads directories through the server, keeps an editor
  buffer per path, and refuses to open files above the configured size.
- **Changes** parses patch text from tool output and from the session diff endpoint, keeping
  per-file additions and deletions.
- **Git** runs native `git` commands with a timeout, parses porcelain status, and exposes
  stage, unstage, discard, commit, branch, log, fetch, pull, and push.
- **Terminal** spawns a PTY with `portable-pty`, feeds bytes through a `vte` parser, and
  stores cells, colors, cursor, scrollback, and the alternate screen. Input encoding covers
  control keys, arrows, function keys, and Alt combinations.

## Theme engine

Palettes come from bundled JSON theme files. Each file defines a light or dark variant; the
settings hold one of each and a brightness value interpolates between them. Text colors are
checked for contrast against their surface after alpha compositing, and corrected when they
fall below the floor. UI scale, spacing, font family, code size, terminal size, density, and
motion are applied globally from the same settings object.

The fonts are bundled: Inter, JetBrains Mono, and Instrument Serif. Icons are vector data
compiled into the binary.

## Testing

- `cargo test --workspace` covers parsing, state projection, layout math, editor behavior,
  key encoding, and theme resolution.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- CI checks formatting, lints, and tests, then builds the workspace on Linux, macOS, and
  Windows.

`RCHAMBER_SCREENSHOT=/path/out.png` runs the app off-screen, captures one frame after the
backend settles, and exits. It is used for the screenshots in this repository.

## Boundaries

The app reads provider, agent, command, and skill catalogs but does not write their
configuration. It does not bundle the OpenCode binary; the CLI is installed separately.
There is no embedded browser, plugin host, or auto-updater. These are deliberate limits of
the current scope, not hidden placeholders.
