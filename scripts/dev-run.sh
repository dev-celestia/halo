#!/usr/bin/env bash
# One cargo-watch cycle: reap the Halo left over from the previous cycle, then
# build and run it.
#
# `cargo watch -x run` only signals the `cargo run` parent, so the app it spawned
# is orphaned on every rebuild — it keeps its window and its Dock tile, and a
# bare binary gets a tile per process. Killing it here, before the next build,
# also keeps the single-instance guard (`crates/ui/src/instance.rs`) from turning
# a rebuild into "no window at all".
set -uo pipefail
cd "$(dirname "$0")/.."

if [[ "$(id -u)" -eq 0 ]]; then
  echo "error: dev-run.sh will not run as root. Please run 'make run' without sudo." >&2
  exit 1
fi

kill_hard() {
  local pid="$1"
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.05
  done
  kill -KILL "$pid" 2>/dev/null || true
}

for name in Halo halo; do
  bin="$(pwd)/target/debug/${name}"
  for pid in $(pgrep -f "target/debug/${name}" || true); do
    exe=$(lsof -a -p "$pid" -d txt -Fn 2>/dev/null | awk '/^n/ {print substr($0,2); exit}' || true)
    if [[ "$exe" == "$bin" ]]; then
      kill_hard "$pid"
    fi
  done
done

# The agent's browser CLI has to sit beside the app binary (`probe::browser_cli`
# looks in `current_exe`'s directory first), so build it here and keep a dev
# session identical to the bundle. Cargo no-ops when it is already current.
if ! cargo build -p halo-browser-tools --bin halo-browser; then
  echo "warning: halo-browser failed to build; the agent will have no browser tools" >&2
fi

# HALO_DEV_ARGS is word-split on purpose: it is the `make run -- <flags>` tail.
# shellcheck disable=SC2086
exec cargo run -p halo -- ${HALO_DEV_ARGS:-}
