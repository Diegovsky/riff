build := "BUILD"

compile *ARGS:
    just meson compile {{ARGS}}

run: install
    env RUST_BACKTRACE=1 riff

install:
    just meson install

meson command *ARGS:
    meson {{command}} -C {{build}} {{ARGS}}

update-sources:
    python build-aux/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json

init *ARGS:
    meson setup -Dbuildtype=debug -Doffline=false --prefix="$HOME/.local" {{build}} {{ARGS}}

