# Flatpak and Flathub

RustChamber is a native egui app, so it runs on the Freedesktop runtime and works on both
Plasma and GTK desktops. The Flatpak does not depend on GTK or Qt.

The app needs the user's real development environment: the OpenCode CLI, git, the login
shell, credentials, SSH keys, and repositories. Inside the sandbox those are not available
from `/usr/bin`. The manifest therefore installs small wrappers in `/app/bin` that call
`flatpak-spawn --host` for those tools. This requires the `org.freedesktop.Flatpak` talk
permission and host filesystem access.

## Files

| File | Purpose |
| --- | --- |
| `packaging/flatpak/dev.rustchamber.RustChamber.yml` | Flathub manifest, pinned to a release tag |
| `packaging/flatpak/dev.rustchamber.RustChamber.local.yml` | Builds the current checkout for local testing |
| `packaging/flatpak/cargo-sources.json` | Vendored Rust crates for an offline build |
| `packaging/flatpak/bin/` | Host wrappers for opencode, git, shell, xdg-open, and HEIC tools |

## Local build

Requirements: `flatpak` and `flatpak-builder` on the host.

```bash
# Fedora
sudo dnf install flatpak flatpak-builder appstream-compose

# Debian / Ubuntu
sudo apt install flatpak flatpak-builder appstream appstream-compose
```

Install the runtime and the Rust SDK extension:

```bash
flatpak remote-add --if-not-exists flathub \
  https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub \
  org.freedesktop.Platform//26.08 \
  org.freedesktop.Sdk//26.08 \
  org.freedesktop.Sdk.Extension.rust-stable//26.08
```

Build and install the current checkout:

```bash
cd rustchamber
flatpak-builder --user --install --force-clean \
  --disable-rofiles-fuse build-flatpak \
  packaging/flatpak/dev.rustchamber.RustChamber.local.yml

flatpak run dev.rustchamber.RustChamber
```

The recommended Flathub tool is the `org.flatpak.Builder` app, which provides
`flathub-build` and the linter:

```bash
flatpak install flathub org.flatpak.Builder

flatpak run --command=flathub-build org.flatpak.Builder --install \
  packaging/flatpak/dev.rustchamber.RustChamber.local.yml

flatpak run dev.rustchamber.RustChamber
```

## Host tools

The wrappers expect these on the host:

- `opencode` in `PATH`, `~/.opencode/bin`, or `~/.local/bin`
- `git`
- the login shell from `/etc/passwd`
- `xdg-open` (xdg-utils)
- optionally ImageMagick, `heif-convert`, or `ffmpeg` for HEIC attachments

The wrapper passes `HOME` and `PATH` explicitly, and forwards the OpenCode password
variables, so the managed server authenticates and uses the host configuration. Project
directories are opened with `--directory="$PWD"`.

The sandbox uses `--filesystem=home`, so projects inside the home directory are visible.
Projects under `/mnt`, `/run/media`, etc. need an additional `--filesystem` entry or
`--filesystem=host`.

## Updates

Flatpak and Flathub own updates. The app detects `FLATPAK_ID` and:

- does not check GitHub for updates;
- shows “Updates are managed by Flathub” in Settings;
- tells the user to run `flatpak update dev.rustchamber.RustChamber`.

## Flathub submission

1. Regenerate the vendored crates whenever `Cargo.lock` changes:

   ```bash
   flatpak-cargo-generator Cargo.lock -o packaging/flatpak/cargo-sources.json
   ```

   The generator is available from
   [flatpak/flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools).

2. Make sure the release tag exists on GitHub. The manifest uses
   `v<version>` and should also pin the commit:

   ```yaml
   - type: git
     url: https://github.com/rustchamber/rustchamber.git
     tag: v0.2.7
     commit: <full-commit-sha>
   ```

3. Validate locally:

   ```bash
   flatpak run --command=flathub-build org.flatpak.Builder --install \
     packaging/flatpak/dev.rustchamber.RustChamber.yml

   flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
     manifest packaging/flatpak/dev.rustchamber.RustChamber.yml

   appstreamcli validate packaging/linux/dev.rustchamber.RustChamber.metainfo.xml
   desktop-file-validate packaging/linux/dev.rustchamber.RustChamber.desktop
   ```

4. Fork [flathub/flathub](https://github.com/flathub/flathub), clone the fork on the
   `new-pr` branch, add the manifest, `cargo-sources.json`, and
   `packaging/flatpak/flathub.json` following the repository's new-app submission
   template, and open a pull request against `new-pr` (not `master`). The app ID is
   `dev.rustchamber.RustChamber`; the domain
   `rustchamber.dev` is owned by the project, so complete the verification requested in
   the review. The `flathub.json` limits the initial build to x86_64.

5. In the pull request, explain the permissions:

   - `--filesystem=home`: the app is a project workspace; it edits, diffs, and runs git in
     the user's repositories.
   - `--talk-name=org.freedesktop.Flatpak`: the app is a client for the host OpenCode CLI,
     git, and the user's shell. Bundling a Node/Go runtime and OpenCode would duplicate the
     user's authenticated environment and freeze its version.
   - `--socket=ssh-auth`: git over SSH.
   - `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri`,
     `--share=network`: normal desktop rendering and the loopback OpenCode server.

6. After the merge, Flathub builds and hosts the app. Future versions require a manifest
   update with the new tag/commit and a matching AppStream `<release>`.

## Notes

- The app ID stays `dev.rustchamber.RustChamber`; the desktop file, metainfo, icon, and
  `with_app_id` already match it.
- The Flatpak build does not use the Ubuntu 20.04 container used for the portable tarball.
  It compiles inside the Freedesktop SDK, so the glibc floor of the sandbox is irrelevant.
- The native packages and the Flatpak can coexist.
