# Extensions

RustChamber extensions are native: a folder with a manifest and a Lua file. They do not use
a webview and do not run JavaScript. An extension can add a panel, a section in the
session context panel, rows in the composer's `@` menu, slash commands, message actions,
tool renderers and components inside an agent reply.

## Install

Open **Settings → Extensions** and paste one of these into the field:

- the absolute path of a folder on this computer;
- the path of a `.zip` archive;
- an `https://` link to a zip archive.

Folder installs run from that folder, so saving the Lua and pressing **Reload** is enough.
Archives and links are copied into `extensions/` inside the app data directory:

- macOS: `~/Library/Application Support/rustchamber/`
- Linux: `~/.config/rustchamber/`
- Windows: `%APPDATA%\rustchamber\`

After **Add**, the card shows **Needs approval**. Press **Review permissions** and allow
the whole list, or remove the extension. A new version that asks for more capabilities
goes back to **Needs approval** until you review it again.

The same flow works from a chat link:

```markdown
[Install Deploys](rchamber://extensions/install?path=/Users/me/rustchamber-extensions/deploys)
```

## What a permission means

| Permission | What it allows |
| --- | --- |
| Storage | The extension's own JSON data, deleted on removal |
| Send messages | `host.prompt`, `host.answer`, `host.prompt_session` |
| Sessions | List, create and open sessions |
| Files | Read and write inside the open project only |
| Network origin | HTTPS requests to exactly that origin |
| Model | One-off text generation (not available in this build yet) |
| Clipboard | Copy text |
| Notifications | Show a notification |
| Run commands | `host.exec`, with your user's access. Install only if you trust the author |
| Instructions | Add a short note to what agents read |
| Events | Hear when a turn finishes or asks for permission |

An extension runs in a Lua sandbox with a 32 MiB memory limit and a 5 second limit per call.
`require`, `io`, `os`, `package` and bytecode loading are not available. Storage is capped at
2 MiB per extension, 64 KiB per value and 2,000 keys.

Disabling an extension stops its worker and hides every surface but keeps its data, grants
and settings. Removing it deletes the data and the copied package. A folder install is never
deleted from disk.

## Where it shows up

- **Panel** — a rail icon and a docked panel. The extension returns widgets; the header has
  refresh, maximize/minimize and close controls, and the panel keeps its own tab selection
  and form state.
- **Status section** — a block inside the chat's context panel, refreshed on the manifest's
  interval.
- **Mentions** — rows in the `@` menu. Selecting a row inserts its text into the composer;
  the agent receives that text.
- **Commands** — `/name args` runs the extension instead of the model.
- **Actions** — a `…` menu on assistant messages. An action may return widgets and open the
  extension panel with the result.
- **Tool renderers** — match a tool name such as `mcp.deploys.*`, then draw its header and
  output as text, JSON, Markdown, code or a table.
- **Blocks** — the agent writes a component such as `Deploys("api")` and the extension draws
  it. A block that declares `answers` can answer the agent with `host.answer`.

## Build one

Type `/new-extension` in the composer and describe what you want. The agent reads the
authoring guide, writes a folder in `~/rustchamber-extensions/<id>/`, validates it and ends
with an install link. The guide is
[`crates/rchamber-ext/assets/authoring-skill.md`](../crates/rchamber-ext/assets/authoring-skill.md).

A minimal package:

```json
{
  "apiVersion": 1,
  "id": "hello",
  "name": "Hello",
  "version": "1.0.0",
  "runtime": { "kind": "lua", "entry": "main.lua" },
  "contributes": {
    "statusSection": { "title": "Hello", "method": "status" }
  }
}
```

```lua
function status()
  return {
    { kind = "text", markdown = "Hello from a native extension." },
  }
end
```

Validate it from a terminal:

```bash
rchamber ext check ~/rustchamber-extensions/hello
```

A complete example with every surface is at
[`examples/extensions/deploys`](../examples/extensions/deploys).

## Rules

- Never store secrets in extension storage. Use the `integration` manifest block when the
  extension needs an account; the token stays in RustChamber.
- The `exec` capability is a real shell with your user's access. Prefer the host API.
- A Lua error is reported as a toast and the extension stays installed. Repeated failures
  do not crash the app.
- Calls run on a worker thread. A slow extension cannot freeze the interface.
