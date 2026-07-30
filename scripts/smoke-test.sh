#!/usr/bin/env bash
set -uo pipefail

# Startup smoke test: launches Riff, lets it run for a few seconds, then shuts
# it down and asserts that no error-level messages were logged during startup.
#
# Detects:
#   - Rust log crate ERROR lines (env_logger output)
#   - GLib/GTK/GObject/Pango/Adwaita CRITICAL and WARNING diagnostics
#   - Rust panics
#
# Requires a display, a session D-Bus, and (ideally) an unlocked secret service.
# The riff-smoke.yml workflow provides these via xvfb-run + dbus-run-session +
# gnome-keyring. Locally you can run:
#   xvfb-run -a dbus-run-session ./scripts/smoke-test.sh
#
# Environment:
#   RIFF_BIN     path to the riff binary (default: target/src/riff, then PATH)
#   RUN_SECONDS  how long to let the app run before shutting down (default: 8)

RUN_SECONDS="${RUN_SECONDS:-8}"

# Locate the binary.
if [[ -n "${RIFF_BIN:-}" ]]; then
    BIN="$RIFF_BIN"
elif [[ -x "target/src/riff" ]]; then
    BIN="target/src/riff"
elif command -v riff >/dev/null 2>&1; then
    BIN="$(command -v riff)"
else
    echo "ERROR: could not find the riff binary. Set RIFF_BIN." >&2
    exit 1
fi

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

echo "Launching $BIN for ${RUN_SECONDS}s ..."

# Surface all riff logs; keep GTK/GLib fatal behaviour off so the app is not
# killed by warnings before we can collect them.
export RUST_LOG="${RUST_LOG:-riff=debug}"
export RUST_BACKTRACE=1
export G_DEBUG="${G_DEBUG:-fatal-criticals}"

# Run the app in the background and stop it after RUN_SECONDS.
"$BIN" >"$LOG" 2>&1 &
APP_PID=$!

sleep "$RUN_SECONDS"

if ! kill -0 "$APP_PID" 2>/dev/null; then
    # The process already exited; capture its status.
    wait "$APP_PID"
    STATUS=$?
    echo "WARNING: app exited early with status $STATUS" >&2
else
    # Ask it to quit, then force-kill if it lingers.
    kill -TERM "$APP_PID" 2>/dev/null || true
    for _ in $(seq 1 10); do
        kill -0 "$APP_PID" 2>/dev/null || break
        sleep 0.5
    done
    kill -KILL "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
fi

echo "----- captured startup log -----"
cat "$LOG"
echo "--------------------------------"

# Patterns that indicate a startup problem.
#   \[..ERROR..\]     - env_logger error level (inside the bracketed header)
#   <Domain>-CRITICAL/WARNING ** - GLib/GTK style diagnostics
#   panic markers
if grep -nE \
    '\[[^]]*\bERROR\b[^]]*\]|(GLib|GLib-GObject|GLib-GIO|Gtk|Gdk|Adwaita|Pango|GStreamer)-(CRITICAL|WARNING) \*\*|thread .* panicked|panicked at' \
    "$LOG"; then
    echo ""
    echo "FAIL: error-level messages were logged during startup (see above)."
    exit 1
fi

echo "PASS: no error-level messages during startup."
