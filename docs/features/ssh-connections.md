# Feature: SSH connections

Shipped design for saved server connections. Read with [`../STANDARDS.md`](../STANDARDS.md).

## Outcome

Settings → General lists the managed OpenCode server, saved HTTP endpoints, and SSH hosts on one page.

The managed local server is always available and needs no entry.

Saved connections are external HTTP endpoints and SSH servers.

An SSH connection starts `opencode serve` on the remote host over SSH, forwards a local loopback port through the same connection, and connects like any other endpoint.

One connection is active. Launch uses the last active connection.

## Crate placement

| Crate | Work |
| --- | --- |
| `rchamber-api` | `ssh` module: target parsing, remote serve, tunnel launch. Owns the ssh child. |
| `rchamber-app` | `connections` store, actions and events, backend connect path. |
| `rchamber-ui` | General page server section, cards, menus, add and edit dialogs. |

`shell-words` is the only new crate dependency. It is already in the workspace.

## Storage

`connections.json` lives next to `settings.json`.

Fields: `id`, `name`, `kind` (`endpoint` or `ssh`), `endpoint`, `ssh`, `password`, `created_at`, `last_used`.

The SSH string is the source of truth. It is parsed and validated on every use.

Endpoint passwords stay plain text, same as the legacy settings file. SSH entries store no secrets.

Writes are atomic: unique temp file, `sync_all`, rename, `0600`, process wide lock.

A corrupt or invalid UTF 8 file moves aside. A future version is backed up and loads as read only.

The legacy settings endpoint and password seed the first entry once. They are cleared only after the new file writes successfully.

## SSH input

Accepted forms:

`host`

`user@host`

`ssh user@host`

`ssh -p 2222 user@host`

Full command with `-i key`, `-J jump`, `-o Option=value`, `-F config`, `-4` or `-6`

Tokenize with `shell-words`. Strip a leading `ssh`. Pass everything as argv. Never run the user command through a shell.

Rejected flags: `-N`, `-L`, `-R`, `-D`, `-W`, `-f`, `-O`, `-S`, `-M`, `-A`, and unknown short flags.

Rejected `-o` keys: anything that can run local commands or change the tunnel, including `ProxyCommand`, `LocalCommand`, `RequestTTY`, `ForkAfterAuthentication`, `ControlMaster`, `ClearAllForwardings`, and `UserKnownHostsFile`.

A user supplied `-F` config is trusted like a manual `ssh` invocation.

Require exactly one destination token. Reject empty input, shell metacharacters in the host, and more than one destination.

Errors are complete sentences shown inline in the dialog.

## Connect sequence

All steps run in the backend actor with timeouts. The UI thread never blocks.

1. Parse and validate the target. Pick a free local port `L`.
2. Choose a random high remote port `R`. Retry a remote bind collision with a new `R`, up to three attempts.
3. Generate a random password.
4. Start one ssh child with the tunnel and the remote server:

```
ssh -T -o ExitOnForwardFailure=yes -o ServerAliveInterval=15
    -o ServerAliveCountMax=3 -o StrictHostKeyChecking=accept-new -o BatchMode=yes
    <user args> -L 127.0.0.1:L:127.0.0.1:R <destination>
    sh -c 'read pw; ... opencode serve --hostname 127.0.0.1 --port R'
```

`-T` prevents a pty from echoing the secret.

The remote command checks for OpenCode 2.x, reads the password from stdin, exports `OPENCODE_PASSWORD`, `OPENCODE_SERVER_PASSWORD`, and `OPENCODE_SERVER_USERNAME`, then starts the server in the background. It uses only POSIX shell builtins and expansion during the version check, so minimal hosts work.

A foreground remote reader holds the channel open. When stdin closes or ssh dies, the exit trap kills the remote server.

5. A missing or older remote OpenCode reports the requirement on stderr. Failures are classified into a cause and a next step before they reach the UI.
6. Poll `http://127.0.0.1:L/api/info` with Basic auth for 45 seconds. A 401 fails at once with the health error.
7. The ssh child and its stdin hang off `ServerHandle` with `kill_on_drop`. Dropping the handle closes stdin and kills ssh.
8. The backend checks the ssh child once per second. An exit emits `Connection::Failed` and reconnects.
9. The SSE watchdog handles HTTP reconnects inside a live tunnel.

Host keys use `StrictHostKeyChecking=accept-new`. The first connect trusts and records a new key. README says to prepopulate `known_hosts` for fingerprint review.

## Actions and events

`Action::ConnectServer { id }` selects a saved connection. `None` selects the managed server.

`Action::SaveConnection { draft }` and `Action::DeleteConnection { id }` update the store.

`BackendEvent::Connections { connections, active }` follows every store change.

The existing `Connection` enum covers status. Only one connection is active.

The backend validates drafts again. Editing the active connection reconnects.

A failed store write reports an error and does not emit saved entries.

## UI

The General page hosts the server list. Keywords: server, ssh, connection, endpoint.

Connection card list: name, endpoint or SSH summary, status dot, and a `…` menu with Connect, Edit, Remove. The active card is highlighted. Clicking the card connects.

Top right **Add server** button opens the shared popup menu with **Add server** and **Add SSH server**.

**Add server** dialog: endpoint URL, optional password, optional name.

**Add SSH server** dialog: host or SSH command, optional remote folder, optional server name, inline parse error. Submit stays disabled until validation passes. The remote folder defaults to `.`.

Dialogs use `widgets` dialog primitives, `text_field`, `password_field`, `menu_popup`, and theme tokens.

Drafts live in `app.ui`. Menus share keyboard navigation.

The Compute readout follows the active connection.

## Security

The ssh command runs as argv, never a shell.

The tunnel binds `127.0.0.1` locally. Remote serve binds `127.0.0.1` remotely.

The password travels over stdin, never argv. Logs record `password_len` only.

SSH authentication uses the user agent and keys. The app stores nothing. Interactive password and keyboard prompts are unsupported; failures say so.

`accept-new` is an explicit first use trust decision.

`-F` config files are confined to `~/.ssh` or `/etc/ssh`, because a config file can run local commands. Other unsafe `-o` keys stay blocked.

## Testing

Parser: every accepted form, each rejected flag, unsafe `-o` keys, config path confinement, two destinations, metacharacters, empty input.

Argument construction: `-T`, forward, destination, remote command, password env vars, POSIX only remote script, `sh -n` syntax.

Failure classification: auth, host key, forwarding, missing binary, version, shell, resolve, network, config.

Store: roundtrip, old files, unversioned files, corrupt file, invalid UTF 8, future version, failed writes, permissions, migration, active cleanup.

No network or ssh in tests.

CI gates: fmt, clippy `-D warnings`, workspace tests on Linux, macOS, and Windows.

## Known limits

Flatpak routes ssh through `flatpak-spawn --host`.

Windows compiles; SSH connections there are untested.

Remote hosts need a POSIX `sh`. Windows OpenSSH remotes are unsupported.

The server must allow local TCP forwarding for the account. Forced commands and jump only accounts fail with a policy message.

Password and keyboard interactive login are unsupported. Use an agent backed key.

The remote OpenCode binary must be on the default PATH or in `~/.opencode/bin`, `~/.local/bin`, `~/.bun/bin`, or `~/.npm-global/bin`.

Remote projects use remote paths. The open project path must exist on the remote host. RustChamber does not translate paths. Remote projects are listed under their connection in Add project and are never marked missing. Projects added from an SSH host remember that server, and project lists show its name as a badge beside the project.

Tunnel reconnect has no backoff cap yet.

Connection dialogs do not autofocus or submit on Enter yet.
