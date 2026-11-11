#!/usr/bin/env bash
# Run Halo UI in debug mode with hot reload on code changes.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "$(id -u)" -eq 0 ]]; then
  echo "error: do not run 'sudo make run' (or 'sudo ./scripts/dev.sh')." >&2
  echo "macOS GUI desktop applications and development watchers must run as your normal user account." >&2
  echo "" >&2
  echo "To launch in development mode, simply run:" >&2
  echo "  make run" >&2
  exit 1
fi

./scripts/ensure-gpui-vendor.sh

ensure_cargo_watch() {
  if cargo watch --version >/dev/null 2>&1; then
    return 0
  fi
  echo "cargo-watch not found — installing (one-time)…"
  cargo install cargo-watch --locked
}

ensure_cargo_watch

# `cargo-watch` traps SIGTERM and waits for the command it is running — which
# here is the app itself, so it never gets to exit. A stopped `make run`
# therefore leaves the watcher (and its window) alive forever, and the next
# `make run` adds another one. Escalate to SIGKILL.
kill_hard() {
  local pid="$1"
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.05
  done
  kill -KILL "$pid" 2>/dev/null || true
}

# One dev session per checkout: the marker hash identifies this repo root, so it
# identifies every watcher ever started for it — whatever TMPDIR that shell had.
marker_key=$(pwd | shasum -a 256 | cut -c1-16)
for pid in $(pgrep -f "halo-dev-session-${marker_key}" || true); do
  if [[ "$pid" != "$$" ]]; then
    kill_hard "$pid"
  fi
done

# A watcher started by another user (a `sudo make run` once poisons `target/`
# with root-owned artifacts) cannot be signalled from here. Say so instead of
# silently running a second session.
survivors=$(pgrep -f "halo-dev-session-${marker_key}" || true)
if [[ -n "$survivors" ]]; then
  echo "warning: watcher(s) for this checkout survived the reap (started by another user):"
  echo "         sudo kill -9 ${survivors//$'\n'/ }"
  echo "         and fix the artifacts they built: sudo chown -R \"$(id -un)\" target"
fi

# Reap the app too. A bare binary gets one Dock tile per process, and a watcher
# that died without signalling its child leaves the window (and the tile)
# behind. The lowercase legacy name is reaped as well, so a session started
# before the `Halo` rename cannot linger.
for bin in "$(pwd)/target/debug/Halo" "$(pwd)/target/debug/halo"; do
  for pid in $(pgrep -f 'target/debug/[Hh]alo' || true); do
    exe=$(lsof -a -p "$pid" -d txt -Fn 2>/dev/null | awk '/^n/ {print substr($0,2); exit}' || true)
    if [[ "$exe" == "$bin" ]]; then
      kill_hard "$pid"
    fi
  done
done

# The marker lives outside the repo so creating it cannot retrigger the watcher,
# and under a fixed directory so a GUI-launched shell (TMPDIR=/var/folders/…/T/)
# and a plain one (TMPDIR unset) agree on where it is. Cleared once per
# `make run` so the first launch of the session still takes focus.
marker_dir="${HOME}/Library/Caches/Halo"
mkdir -p "$marker_dir"
marker="${marker_dir}/dev-session-${marker_key}"
rm -f "$marker"

# Extra args are forwarded to the halo binary (after `--`).
export HALO_DEV_ARGS="${*:-}"

# `-s` (not `-x run`): a shell cycle that reaps the orphaned app before starting
# the next build. `cargo watch -x run` only signals `cargo run`, so the app it
# spawned survives every rebuild and keeps its Dock tile.
#
# Cargo.lock is touched on every `cargo run` (and by rust-analyzer) without a
# source edit; watching it makes the app restart in a loop.
exec cargo watch --clear \
  -E HALO_HOT_RELOAD=1 \
  -E "HALO_DEV_SESSION_MARKER=${marker}" \
  -i 'Cargo.lock' \
  -i '.cursor' \
  -s './scripts/dev-run.sh'
