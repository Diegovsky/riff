#!/usr/bin/env bash
set -euo pipefail

# Dev setup script for Riff - installs build dependencies.
# Detects the system package manager and prompts before installing.

confirm_risks() {
    echo ""
    echo "WARNING!"
    echo "This script is a best effort attempt to setup the required packages for building Riff on your system."
    echo "As follows this script might fail or break dependencies on your system."
    echo "If you are unconformable or do not understand the risks with installing packages, do not proceed."
    echo ""

    read -r -p "Proceed? [y/N] " response
    [[ "$response" =~ ^[Yy]$ ]]
}

confirm_install() {
    read -r -p "Install the above packages? [y/N] " response
    [[ "$response" =~ ^[Yy]$ ]]
}

detect_pkg_manager() {
    for cmd in dnf yum apt-get pacman zypper; do
        if command -v "$cmd" &>/dev/null; then
            echo "$cmd"
            return
        fi
    done
    echo ""
}

if ! confirm_risks; then
    echo "Aborted."
    exit 0
fi
echo ""

PKG_MANAGER=$(detect_pkg_manager)

if [[ -z "$PKG_MANAGER" ]]; then
    echo "Error: No supported package manager found (dnf, yum, apt-get, pacman, zypper)."
    exit 1
fi

echo "Detected package manager: $PKG_MANAGER"

NEED_RUSTUP=0

case "$PKG_MANAGER" in
    dnf|yum)
        PACKAGES=(
            meson
            ninja-build
            cargo
            rust
            pkgconf-pkg-config
            gtk4-devel
            libadwaita-devel
            glib2-devel
            openssl-devel
            alsa-lib-devel
            pulseaudio-libs-devel
            gstreamer1-devel
            gstreamer1-plugins-base-devel
            gettext-devel
            libcurl-devel
            blueprint-compiler
            libxml2
        )
        INSTALL_CMD="sudo $PKG_MANAGER install"
        ;;
    apt-get)
        PACKAGES=(
            meson
            ninja-build
            pkg-config
            libgtk-4-dev
            libadwaita-1-dev
            libglib2.0-dev
            libssl-dev
            libasound2-dev
            libpulse-dev
            libgstreamer1.0-dev
            libgstreamer-plugins-base1.0-dev
            gettext
            libcurl4-openssl-dev
            blueprint-compiler
            libxml2-utils
        )
        INSTALL_CMD="sudo apt-get install"
        NEED_RUSTUP=1
        ;;
    pacman)
        PACKAGES=(
            meson
            ninja
            rust
            pkgconf
            gtk4
            libadwaita
            glib2
            openssl
            alsa-lib
            libpulse
            gstreamer
            gst-plugins-base
            gettext
            curl
            blueprint-compiler
            libxml2
        )
        INSTALL_CMD="sudo pacman -S --needed"
        ;;
esac

echo ""
echo "The following packages will be installed:"
printf '  %s\n' "${PACKAGES[@]}"
echo ""

if confirm_install; then
    $INSTALL_CMD "${PACKAGES[@]}"

    if [[ "$NEED_RUSTUP" -eq 1 ]]; then
        echo ""
        echo "Debian/Ubuntu ship a Rust toolchain that is too old for Riff's dependencies."
        echo "Installing the latest stable Rust via rustup..."
        if command -v rustup &>/dev/null; then
            rustup update stable
        else
            echo "Couldn't find rustup. Aborting."
            exit 1
        fi
        echo "Rust $(rustc --version) installed via rustup."
    fi

    echo ""
    echo "Done! You can now build with:"
    # shellcheck disable=SC2016
    echo '  meson setup target -Dbuildtype=debug -Doffline=false --prefix="$HOME/.local"'
    echo '  ninja install -C target'
    echo 'You can run your local build with:'
    echo '  ~/.local/bin/riff'
else
    echo "Aborted."
fi
