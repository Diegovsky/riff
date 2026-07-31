<p align="center">
  <img src="data/hicolor/scalable/apps/dev.diegovsky.Riff.svg" width="128" height="128" alt="Riff icon">
</p>

<h1 align="center">Riff</h1>

<p align="center">
    Riff is a Spotify client that puts your music library front and center. It features a clean, minimal design that lets you listen distraction-free.
</p>

![Riff Dark](data/appstream/overview-dark.png#gh-dark-mode-only)![Riff Light](data/appstream/overview-light.png#gh-light-mode-only)

<p align="center">
    <a href="https://flathub.org/apps/details/dev.diegovsky.Riff"><img width="130" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-en.png"/></a>
</p>

---

> [!NOTE]
> Requires a Spotify Premium account. Some accounts may not work due to Spotify's new PlayPlay DRM, which is proprietary and cannot be implemented externally.

> [!WARNING]
> This project does not accept AI-generated contributions, as outlined by [Flathub's Generative AI Policy](https://docs.flathub.org/docs/for-app-authors/requirements/#generative-ai-policy) and [GNOME Circle's AI Policy](https://gitlab.gnome.org/Teams/Releng/AppOrganization/-/blob/main/AppCriteria.md#circle-app-criteria).

## Features

- Modern GTK4 and libadwaita interface with light/dark themes and a responsive layout
- Play, pause, skip, seek, shuffle, and repeat with gapless playback
- Like tracks and follow artists, albums, and playlists
- Browse your saved albums, playlists, liked tracks, and followed artists
- Search for albums, artists, tracks, and playlists
- Share and open Spotify links with your friends using automatic link detection
- Fine-tune your sound with a built-in DSP engine featuring a 10-band equalizer, stereo pan, and pitch shift
- Advance desktop integration: media keys (MPRIS), inhibits session suspend while playing, and secure credential storage

## Installing

**Flathub (recommended)**

```sh
flatpak install flathub dev.diegovsky.Riff
```

**From a GitHub release**

Pre-built bundles are available on the [Releases page](https://github.com/Diegovsky/riff/releases). Download the latest full release or development build Flatpak bundle and install it with:

```sh
flatpak install --user Riff-x86_64.flatpak
```

**Build from source**

```sh
./scripts/setup-dev.sh                # install dependencies
./scripts/build.sh release --install  # build and install to ~/.local
~/.local/bin/riff                     # run Riff
```

See the [Development Guide](doc/Development.md) for full build instructions, project structure, and troubleshooting.

## Contributing

Contributions are welcome and encouraged! See [Contributing Guide](CONTRIBUTING.md) for PR guidelines and process, and the [Development Guide](doc/Development.md) for build setup, feature flags, and debug tools.

Translation contributions are temporarily paused while we migrate to a new platform.


## Code of Conduct
This project follows the [GNOME Code of Conduct](https://conduct.gnome.org). Please familiarize yourself with it before interacting with this repository.
