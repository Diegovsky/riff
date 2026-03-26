build := "BUILD"

compile *ARGS:
    just meson compile {{ARGS}}

run: install
    env RUST_BACKTRACE=1 riff

install:
    just meson install

meson command *ARGS:
    meson {{command}} -C {{build}} {{ARGS}}

init *ARGS:
    meson setup -Dbuildtype=debug -Doffline=false --prefix="$HOME/.local" {{build}} {{ARGS}}

