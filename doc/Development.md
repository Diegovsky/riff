# Development Guide

This guide is the starting point for developing Riff. It walks you from a clean checkout to a running debug build, then covers how the codebase is organized, expectations of your first Pull Request, and the tools available for development.

## Quick Setup

Get a development build running in three steps. Run these from the repository root.

```sh
# 1. Install dependencies (detects dnf, apt-get, pacman, or zypper)
./scripts/setup-dev.sh

# 2. Build a debug build and install to ~/.local
./scripts/build.sh dev --install

# 3. Run with debug logging
RUST_LOG='riff=debug,librespot=error' ~/.local/bin/riff
```

For a release build, use `./scripts/build.sh release --install`. See the [detailed setup and build reference](#appendix-detailed-reference) below for manual dependency lists, Meson/Justfile usage, and GNOME Builder.

## Architecture Overview

Riff is a GTK4/libadwaita application written in Rust, built with Meson (Ninja backend). The core follows a unidirectional data flow:

- **AppState** is the single source of truth, mutated only via dispatched **actions**.
- Actions produce **events** that listeners use to update GTK widgets.
- All dispatching flows through a `futures::channel::mpsc` channel consumed on the GLib main loop.
- The app state is only readable from the main thread.

See [`Design.md`](Design.md) for the full data flow explanation.

### Directory Structure

| Directory | Responsibility |
| --- | --- |
| `src/` | Rust source code and bundled UI assets. |
| `src/api/` | Spotify Web API client with response caching. |
| `src/app/` | Core application logic and state. |
| `src/app/state/` | Centralized application state and reducers. |
| `src/app/models/` | Presentation models binding state to UI. |
| `src/app/components/` | GTK widget wrappers and event listeners. |
| `src/app/dev_tools/` | Dev menu (debug builds only). |
| `src/auth/` | OAuth2 login flow and secure token storage. |
| `src/audio_engine/` | DSP chain between librespot and the audio backend (EQ, pitch, pan, mono, mix). |
| `src/connect/` | Spotify Connect device support. |
| `src/player/` | librespot session and local playback management. |
| `src/dbus/` | MPRIS / D-Bus integration. |
| `data/` | Application data installed on the system (icons, desktop file, appstream, GSchema). |
| `po/` | Translations (gettext). |
| `scripts/` | Developer and tooling scripts. |
| `scripts/lint/` | HIG and style linters (Python). |
| `flatpak/` | Flatpak packaging manifests and cargo sources. |
| `doc/` | Developer documentation. |
| `subprojects/` | Meson subproject wrap files. |
| `.github/` | GitHub Actions workflows, issue templates, dependabot config. |

## PR Guidelines

Riff is moving towards trunk-based development. Work merges into the `development` branch in small, complete increments rather than living on long-lived branches. Open pull requests against `development`; `master` tracks releases.

- **Prefer complete features over half-finished ones.** A PR should land a feature that works end to end. If a feature is not ready, gate it behind a [feature flag](#feature-flags) rather than merging a partial, user-visible implementation.
- **Squashing commits is recommended.** Collapse a PR into a focused set of commits (often a single commit) before merge so history stays readable.
- **Long commits are fine when the feature is pinpointed.** A large diff is acceptable as long as the commit represents one clearly scoped feature or change. Avoid mixing unrelated changes into the same commit.
- **Code comments are encouraged.** Explain intent and non-obvious decisions, especially around state, events, and the audio engine.
- **Unit tests are encouraged.** Add tests alongside new features and bug fixes. See [Testing and Debugging](#testing-and-debugging).

### Code Style

- Rust code follows `rustfmt` with the config in `rustfmt.toml`.
- Indent with spaces: 4 for Rust, 2 for `.blp`, `.yml`, `.css`, and `.ui` files (see `.editorconfig`).
- UI updates must follow [GNOME HIG](https://developer.gnome.org/hig/) conventions.

## Feature Flags

Incomplete or experimental features are gated behind runtime feature flags rather than merged in a half-finished, always-on state. This keeps trunk-based development safe: a feature can land on the `development` branch while remaining hidden until it is ready.

Flags are GSettings booleans defined in `src/feature_flags.rs` and toggled in the Settings dialog under the "Experiments" section.

### Creating a New Feature Flag

Adding a flag takes two coordinated changes: the Rust enum and the GSettings schema.

1. In `src/feature_flags.rs`, add a variant to the `FeatureFlag` enum, add it to `FeatureFlag::ALL`, and fill in the `key()`, `title()`, and `description()` match arms:

   ```rust
   pub enum FeatureFlag {
       // ...
       MyFeature,
   }

   // in key():   FeatureFlag::MyFeature => "feature-my-feature",
   // in title(): FeatureFlag::MyFeature => "My Feature",
   ```

2. In `data/dev.diegovsky.Riff.gschema.xml`, add a matching boolean key (the name must equal `key()`):

   ```xml
   <key name="feature-my-feature" type="b">
     <default>false</default>
     <summary>Enable my experimental feature</summary>
   </key>
   ```

3. Rebuild so the schema is recompiled (`./scripts/build.sh dev --install` runs `glib-compile-schemas` via meson's post-install). Then gate the feature in code with `is_enabled(FeatureFlag::MyFeature)`.

### Using Feature Flags in Code

```rust
use crate::feature_flags::{is_enabled, FeatureFlag};

if is_enabled(FeatureFlag::DeviceSelector) {
    // show device selector
}
```

### Overriding Flags During Development

Use `gsettings` to toggle flags without opening the Settings dialog:

```sh
gsettings set dev.diegovsky.Riff feature-device-selector true
gsettings set dev.diegovsky.Riff feature-select-mode false
```

## Testing and Debugging

### Unit Tests

```sh
./scripts/build.sh test
# or directly:
cargo test --workspace
```

Note: `cargo test` requires a placeholder `src/config.rs` since that file is normally generated by meson. The build script handles this, but if you run cargo directly:

```sh
echo > src/config.rs
cargo test --workspace
```

### Lint and Quality Checks

Run the static checks before committing:

```sh
./scripts/check-quality.sh
```

### Debug Tools (Dev Menu)

Debug builds include a developer menu in the sidebar header (a wrench/gear icon). It is compiled only in debug builds and lives in `src/app/dev_tools/`. The menu provides:

| Tool | What It Does |
| --- | --- |
| Force Skeleton | Adds the `force-skeleton` CSS class to the window so all skeleton/loading states render simultaneously. Useful for testing loading UI without slow network conditions. |
| Debug CSS | Toggles an overlay stylesheet that highlights alignment and rendering issues. |
| Panel Sizes | Overlays each major panel (sidebar, header, navigation stack, playback bar) with its current pixel dimensions in a distinct color. |
| Simulate Offline | Blocks all HTTP calls and kills the librespot session. The connection-lost banner appears naturally once requests fail, mirroring how a real network drop is detected. |
| Kill Player | Sends `DevKillPlayer` to force-terminate the playback session. |
| Kill Session | Sends `DevKillSession` to destroy the librespot TCP session without stopping local playback state. |
| Inject API Error | Forces every Spotify Web API request to fail with a chosen error (429 rate limit, 401 unauthorized, 500 server error, or empty response). Toggle buttons select the error type; "Off" clears it. |
| Expire OAuth Token | Backdates the cached token and forces an immediate refresh cycle. |
| Test Toast | Fires a notification to verify the toast overlay is working and positioned correctly. |
| Reload CSS | Reloads stylesheets from source files on disk (not bundled gresource copies). CSS edits apply immediately without restarting the app. |
| Dump State | Logs a summary of the current application state to the terminal: logged-in user, playlist count, browser screen/nav depth, selection state, and playback state. |

### Log Levels

Riff uses the `env_logger` crate. Control verbosity with `RUST_LOG`:

```sh
# App debug, silence librespot
RUST_LOG='riff=debug,librespot=error' riff

# Trace-level for the API layer
RUST_LOG='riff::api=trace,riff=info' riff

# Everything at trace (very noisy)
RUST_LOG=trace riff
```

### HTTP Proxy

Riff uses [isahc](https://github.com/sagebind/isahc) (backed by libcurl) for HTTP. You can route API traffic through a proxy:

```sh
https_proxy=http://localhost:8080 riff
```

In debug mode, Riff skips SSL certificate verification so MITM proxies (mitmproxy, Charles, etc.) work without extra CA setup.

### Cache

Riff caches images and HTTP responses in `~/.cache/riff/`. Clear this directory to force fresh fetches.

<div style="page-break-after: always;"></div>

---

# Appendix: Detailed Reference

## Installing Dependencies

`./scripts/setup-dev.sh` detects your package manager (dnf, apt-get, pacman, zypper), lists packages, and asks before installing. On Debian/Ubuntu it also installs Rust via rustup since the packaged toolchain is too old.

To install manually, you need:

- Rust (stable toolchain)
- Meson (>= 0.59) and Ninja
- GTK4 and libadwaita development libraries
- GLib and OpenSSL development libraries
- GStreamer and GStreamer base plugins development libraries
- ALSA and PulseAudio development libraries
- libcurl development libraries
- `blueprint-compiler` (UI files)
- `gettext` (translations)
- `libxml2` / `libxml2-utils`
- `pkg-config`

## Building

`./scripts/build.sh` handles the common workflows (debug/release builds, install, test, clean) and takes flags like `--install`, `--no-check`, and `--features`. Run `./scripts/build.sh --help` for the full list of modes and options.

Alternatives to the build script:

```sh
# Meson directly (use -Dbuildtype=release for an optimized build)
meson setup target -Dbuildtype=debug -Doffline=false --prefix="$HOME/.local"
ninja install -C target

# Justfile
just init      # meson setup with debug defaults
just compile
just install
just run       # install + run with debug logging
```

GNOME Builder: open the project, make `flatpak/dev.diegovsky.Riff.snapshots.json` active, and build. Requires `org.freedesktop.Sdk.Extension.rust-stable` matching the Freedesktop SDK GNOME uses.

## UI Development (Blueprint)

Widget layouts are written in [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/) (`.blp`) files that live next to the Rust component that drives them (for example `src/app/components/shell/playback/playback_controls.blp`). At build time `blueprint-compiler` compiles every `.blp` into a `.ui` file, which is bundled into a GResource and loaded by the component's GObject template.

To add or change UI:

1. Edit the relevant `.blp` file, or add a new one.
2. If you added a file, register it in two places:
   - the `blueprints` `custom_target` input list in `src/meson.build`, so it gets compiled; and
   - `src/riff.gresource.xml`, so the resulting `.ui` (and any `.css`) is bundled. Reference the compiled `.ui` name, not the `.blp`.
3. Bind the template to a Rust widget with the usual `gtk::CompositeTemplate` pattern, using the resource path from `riff.gresource.xml`.
4. Rebuild. Blueprint and resource compilation run as part of the meson build.

The dev menu's "Reload CSS" reloads stylesheets from disk without a restart, but `.blp` changes require a rebuild.

## Adding a GSetting

Persistent settings are GSettings keys defined in `data/dev.diegovsky.Riff.gschema.xml`. To add one:

1. Add a `<key>` (and an `<enum>` above the schema if it is an enumerated value) to the gschema. Boolean example:

   ```xml
   <key name="my-setting" type="b">
     <default>false</default>
     <summary>Short description of the setting</summary>
   </key>
   ```

2. Rebuild and install so the schema is recompiled (`./scripts/build.sh dev --install`; meson's post-install runs `glib-compile-schemas`). Reading a key that is not yet in the compiled schema aborts the app, so the schema must be installed before the code that uses it runs.
3. Read or write it via `gio::Settings::new("dev.diegovsky.Riff")`.

See [Creating a New Feature Flag](#creating-a-new-feature-flag) for the flag-specific variant of this workflow.

## Translations

Strings live in `.blp` and `.rs` files listed in `po/POTFILES.in`. After adding translatable strings:

```sh
ninja riff-pot -C target        # regenerate the .pot template
ninja riff-update-po -C target  # update .po files
poeditor pull                   # pull translations from POEditor
```

## Spotify CLI

`scripts/spotify-cli.py` is an authenticated Spotify Web API client for the terminal. It shares credentials with Riff through the system keyring, so once you have logged in to the app you can hit the API directly to inspect responses while developing.

```sh
spotify-cli v1/me                     # GET the current user
spotify-cli v1/me/playlists           # list playlists (interactive pagination)
spotify-cli -X PUT v1/me/player/play -d '{"uris":["spotify:track:..."]}'
spotify-cli --check-auth              # verify stored credentials
spotify-cli --logout                  # clear stored credentials
spotify-cli --setup-completions       # enable bash/zsh tab completion
```

Run it with `uv` from the `scripts/` directory (`uv sync` first), or install it with `pip install .`. Output is colorized JSON.
