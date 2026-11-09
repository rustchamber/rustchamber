# Writing a RustChamber extension

## Fast path (read this first)

- Build the **smallest** extension that satisfies the request: one panel **or** one status section, not a test bench.
- Do **not** read, grep or explore the RustChamber source. This guide is the whole contract.
- Do **not** run the app, start services, write test harnesses, or use a standalone Lua interpreter.
- Write the manifest and Lua, run `rchamber ext check <folder>` **once**, and stop when it says ready.
- Finish with the install link. A test extension should take a few minutes, not ten.

An extension is a folder with `rchamber.json` and a Lua file. RustChamber reads
the manifest, approves what it asks for, and runs the Lua in a sandbox. There is
no build step: after the user adds the folder once, editing and saving the Lua
reloads it.

Your job is to put the feature where the user already looks. A panel nobody
opens is the weakest answer. Most good extensions use two or three points:
a status line while working, a command or `@` row, an action on a message, a
component inside a reply.

## Steps

1. **Design.** Pick the points that fit from *Where it lives*. Tell the user
   what they will see, where, and when it needs them. Ask only what the request
   leaves open.
2. **Create** the folder the user named, else `~/rustchamber-extensions/<id>`.
   Never create it inside the project being worked on. Write `rchamber.json`
   and `main.lua`.
3. **Check.** Run `rchamber ext check <folder>` and fix what it names.
4. **Hand over.** End the reply with an install link, which the chat draws as a
   button:

   ```markdown
   [Install Deploys](rchamber://extensions/install?path=/Users/me/rustchamber-extensions/deploys)
   ```

   Use the absolute path. One click, then the user approves the permissions.

## Where it lives

| The user wants | Use |
| --- | --- |
| To glance at a state while working | `statusSection` |
| The agent to know something | `instructions` |
| To hand the agent one item | `mentions` |
| A form or a picker | `attach` (a dialog is not available yet: use `panel` or `commands`) |
| To act on what is on screen | `actions` |
| To decide with the agent | `blocks` |
| Rich data inside a reply | `blocks` |
| To open a file type its own way | `fileEditors` |
| A place for long work | `panel` |
| To connect an account | `integration` |
| To react when a turn ends | `events` |

## Manifest

```json
{
  "apiVersion": 1,
  "id": "deploys",
  "name": "Deploys",
  "version": "1.0.0",
  "description": "Services running on Fly.io",
  "icon": "rocket",
  "runtime": { "kind": "lua", "entry": "main.lua" },
  "capabilities": ["storage", "prompt", "network:https://api.fly.io"],
  "contributes": {
    "panel": { "title": "Deploys" },
    "statusSection": { "title": "Deploys", "method": "status", "interval": 5000 },
    "mentions": { "label": "Deploys" },
    "commands": [{ "name": "deploy", "description": "Deploy a service" }],
    "actions": [
      { "id": "explain", "label": "Explain this deploy", "where": "message" }
    ],
    "tools": [
      {
        "match": "mcp.deploys.*",
        "name": "Deploys",
        "title": "{input.app}",
        "output": "table",
        "columns": ["service", "state"]
      }
    ],
    "blocks": [
      {
        "name": "Deploys",
        "description": "A live deploy card. Args: app name.",
        "args": ["app"]
      }
    ],
    "instructions": "Deploys run on Fly.io. Use `fly deploy` from the app folder.",
    "fileEditors": [
      { "id": "fly-log", "title": "Fly log", "match": "*.flylog", "method": "view" }
    ],
    "events": ["turn.finished", "permission.asked"]
  }
}
```

`id` is kebab-case and is the identity forever. `version` must be semver.
`runtime.command` is reserved for a future process runtime; leave it out.

Limits: 8 actions, 8 commands, 16 tools, 4 blocks, 8 file editors, 40
characters in an action label or mentions label, 4,096 characters of
instructions, and a status interval of at least 1,000 ms.

## Capabilities

Declare only what the code actually uses. The user approves the whole list once.

| Capability | Grants |
| --- | --- |
| `storage` | Its own JSON storage (`host.storage_get`, `set`, `delete`, `keys`) |
| `prompt` | `host.prompt`, `host.prompt_session`, `host.answer` |
| `sessions` | `host.sessions`, `host.start_session`, `host.open_session` |
| `files` | `host.read_file`, `host.write_file`, `host.list_dir`, `host.stat` inside the open project |
| `network:<https origin>` | `host.http` to exactly that origin |
| `model` | `host.generate` (not available in this build yet) |
| `clipboard` | `host.clipboard` |
| `notifications` | `host.notify` |
| `exec` | `host.exec` — runs commands with your user's access. Ask for it only when there is no other way |
| `instructions` | The `contributes.instructions` text reaches agents |
| `events` | `on_event` for `turn.finished` and `permission.asked` |

Declare `storage` when the code calls `host.storage_*`; the host denies those calls
without it. Never ask for `exec` for something the host API can do.

## Lua API

The file runs once when the extension loads. Define global functions; RustChamber
calls them by name. A function that does not exist is simply unavailable.

| Function | Called with | Returns |
| --- | --- | --- |
| `on_load()` | once | nothing |
| `status()` | status refresh | array of widgets |
| `panel()` | panel draw | array of widgets |
| `mentions(query)` | text typed after `@` | array of `{label, detail?, text}` |
| `command(name, args)` | `/name rest` | nothing (use effects) |
| `action(id, item)` | a message/session menu | array of widgets or nothing |
| `block(name, args)` | the agent writes `Name(...)` | array of widgets |
| `on_event(name, payload)` | `turn.finished` / `permission.asked` | nothing |
| `view(path)` | a matching file in Files | array of widgets |

`item` for a message action is `{kind = "message", text = "...", role = "assistant"}`.
For a session action: `{kind = "session", id = "...", title = "..."}`. The
current project and session are always in `host.context()`.

### `host`

```lua
host.context()                  -- {project, session, theme, locale, extension}
                                -- session is a table: {id, title, directory}; use session.title
host.args()                     -- the arguments of the current call
host.toast({kind = "info", message = "Done"})
host.compose({text = "...", send = false})
host.prompt({text = "Fix the build", send = true})
host.prompt_session({session = "ses_...", text = "..."})
host.attach({provider_id = "deploys", id = "api", title = "api", url = "..."})
host.set_badge(3)               -- nil clears it
host.answer("the user's choice")-- a block that answers the agent
host.open_url("https://...")
host.clipboard("text")
host.notify({title = "Deploys", body = "api is down"})
host.log("debug message")       -- tracing only
host.storage_get("key")         -- JSON value or nil
host.storage_set("key", value)
host.storage_delete("key")
host.storage_keys()
host.http({method = "GET", url = "...", headers = {}, body = nil})
host.read_file("relative/or/absolute")
host.write_file("path", "content")
host.list_dir("path")           -- [{name, kind}]
host.stat("path")               -- {kind, size}
host.sessions()                 -- [{id, title, directory}]
host.start_session({project = "/path", title = "..."})
host.open_session("ses_...")
host.generate({prompt = "...", system = "..."})
host.exec({command = "git", args = {"status"}, cwd = "/path"})
```

`host.compose` puts text in the composer. `host.prompt` sends a message in the
current session (or the one named by `session`). `host.answer` answers the agent
from a block that declares `"answers": true`.

Values are ordinary Lua tables. A table with keys `1..n` becomes an array; other
keys become an object. `nil` is JSON null.

## Widgets

`status`, `panel`, `action`, `block` and `view` return an array of widgets. Each
is a table with a `kind`:

```lua
{
  { kind = "heading", text = "Deploys" },
  { kind = "stat", label = "Running", value = "3", tone = "success" },
  { kind = "table", columns = {"Service", "State"}, rows = rows() },
  { kind = "progress", label = "Rollout", value = 62, max = 100 },
  { kind = "callout", markdown = "One service is unhealthy.", tone = "warning" },
  { kind = "button", label = "Refresh", action = "refresh" },
}
```

Available kinds:

- `stack {children, gap?}`, `row {children, wrap?}`, `card {title?, children}`
- `tabs {tabs = [{label, children}]}`
- `text {markdown}`, `heading {text, level?}`, `code {code, language?}`
- `badge {label, tone?}`, `callout {markdown, tone?, title?}`
- `key_value {items = [{label, value}]}`
- `table {columns, rows}`
- `list {items = [{title, markdown?}], ordered?}`
- `steps {items = [{title, markdown?, status?}], direction?}`
- `stat {label, value, delta?, tone?}`
- `progress {label, value, max?}`
- `bar_chart {labels, series = [{name, values}], unit?, whole?}`
- `line_chart {labels, series, unit?}`
- `choice {name, options, label?, multiple?}`
- `input {name, label?, placeholder?, multiline?}`
- `date_input {name, label?, time?, value?}`
- `slider {name, min, max, label?, step?, value?, unit?}`
- `details {title, children}`
- `divider`
- `button {label, variant?, value?}`
- `image {path, alt?, height?}` (the path must be readable by the host)
- `extension_block {name, args}` — only produced by the host, not by you

Tones: `neutral`, `info`, `success`, `warning`, `error`.
Step statuses: `todo`, `done`, `current`, `blocked`.

In a panel or status section, buttons and forms come back to the extension:
a button calls `command(button.value or button.label, values)`, where `values`
is a table of the form fields. In a chat block, a button or a form sends its
answer to the agent as a user message instead. Keep the tree small: it is
redrawn on every refresh and tables show the first 200 rows.

## Prefixes and text

`mentions` returns rows like:

```lua
function mentions(query)
  return {
    { label = "api", detail = "production", text = "The api service logs:\n..." }
  }
end
```

The selected row's `text` becomes part of the prompt. Do not ask for `prompt`
just to render a mention; the host attaches the text.

`command(name, args)` runs when the user sends `/name args` in the composer. It
never reaches the model. Use effects (`host.compose`, `host.prompt`,
`host.attach`, `host.toast`) and return nothing.

`action(id, item)` runs from the message/session menu. Return widgets to open a
result window; return nothing to stay in the background.

`block(name, args)` runs when the agent writes a component the host does not
know. For example, with the manifest block above, the agent writes:

```
root = Stack([d])
d = Deploys("api")
```

and `block("Deploys", {"api"})` must return the widgets. If the block declares
`"answers": true`, a click may call `host.answer(text)`.

## Instructions

`contributes.instructions` is a short, factual note the agent sees in every
session: where the service runs, how to deploy, which endpoint is the staging
one. Keep it under 4,096 characters. Do not put secrets in it.

## Events

`contributes.events` with `on_event(name, payload)` gets:

- `turn.finished` — `{session, directory, outcome = "completed" | "failed" | "cancelled"}`
- `permission.asked` — `{session, directory, tool, title}`

Events only run while RustChamber is open. Do not ask for them if the extension
has nothing useful to do with them.

## Checking and installing

```sh
rchamber ext check ~/rustchamber-extensions/deploys
```

If `rchamber` is not on `PATH`, run `cargo run -q -p rchamber -- ext check <folder>`
from a RustChamber checkout, or skip this step: the install sheet validates the
manifest and the sandbox loads the Lua. The check validates the manifest, loads
the Lua, calls `on_load`, and lists the methods it found. Then finish with:

```markdown
[Install Deploys](rchamber://extensions/install?path=/Users/me/rustchamber-extensions/deploys)
```

The user clicks **Install**, reviews the permissions, and the extension appears
where it declared. Editing `main.lua` and clicking **Reload** in
Settings → Extensions picks up the change.

## Recipes

### A status line from a command

```lua
function status()
  local result = host.exec({ command = "docker", args = { "ps", "--format", "{{.Names}} {{.Status}}" } })
  if result.status ~= 0 then
    return { { kind = "callout", markdown = "Docker is not running.", tone = "warning" } }
  end
  local rows = {}
  for line in result.stdout:gmatch("[^\r\n]+") do
    local name, state = line:match("^(%S+)%s+(.+)$")
    if name then
      table.insert(rows, { name, state })
    end
  end
  return {
    { kind = "table", columns = { "Container", "State" }, rows = rows }
  }
end
```

(Requires `"capabilities": ["exec"]`; a command with no other route may be worth
it, otherwise prefer an API through `host.http`.)

### A form that sends a prompt

```lua
function command(name, args)
  local app = args:match("^(%S+)")
  if not app then
    host.toast({ kind = "warning", message = "Usage: /deploy <app>" })
    return
  end
  host.prompt({
    text = "Deploy " .. app .. ". Read the deploy notes first and ask before pushing to production.",
    send = true
  })
end
```

### A block with a button

```lua
function block(name, args)
  return {
    { kind = "card", title = "Deploy " .. (args[1] or "?"), children = {
      { kind = "text", markdown = "Last run: 3 minutes ago." },
      { kind = "row", children = {
        { kind = "button", label = "Open logs", value = "logs" },
        { kind = "badge", label = "running", tone = "success" }
      } }
    } }
  }
end

function action(id, item)
  if id == "logs" then
    host.open_url("https://fly.io/apps/" .. args[1] .. "/monitoring")
  end
end
```

## Rules

- Never read or write outside the project with `files`; the host refuses it.
- Never ask for `exec` to bypass a missing host API without telling the user.
- Catch errors: a Lua error shows a toast and disables the call, not the app.
- Do not loop forever. Calls have a 5 second limit and 32 MiB of memory.
- Do not use `require`, `io`, `os`, `package`, `load` or bytecode. The sandbox
  does not provide them.
- Do not store secrets in `storage`. This build accepts an `integration` block in
  the manifest for future host-managed accounts, but it does not draw the account
  card yet. Ask the user for a short-lived token through a command, and keep it out
  of the package.
- A new version that asks for more capabilities must be approved again. Bump
  `version` when you change the manifest.
