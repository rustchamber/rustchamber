# Deploys (demo extension)

A fake deploy board that exercises every native extension surface: a rail panel, a context
status section, `@` mentions, a slash command, a message action, a tool renderer and a chat
block.

Validate it:

```bash
cargo run -q -p rchamber -- ext check examples/extensions/deploys
```

Install it from **Settings → Extensions** by pasting the folder's absolute path, or use:

```markdown
[Install Deploys](rchamber://extensions/install?path=/absolute/path/to/examples/extensions/deploys)
```

The data is fake and stays in the Lua file. Nothing is sent anywhere.
