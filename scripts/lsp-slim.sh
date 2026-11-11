#!/usr/bin/env bash
# Shrink a staged language-server tree — vendor/lsp, or the copy already in
# Contents/Resources/lsp.
#
# Two levers, both measured against the real 254 MB tree:
#
#   1. strip the bundled Mach-Os, then restore an ad-hoc signature.
#        node           107.8 MB -> 86.9 MB   (strip -x)
#        ShellCheck      58.7 MB -> 33.9 MB   (strip)
#        rust-analyzer   36.5 MB -> 28.3 MB   (strip)
#
#      `strip` with no flags also removes the symbols node resolves through
#      dlsym at run time. The result still answers `node -e 'console.log(1)'`,
#      but every real script dies with `dyld: missing symbol called` — so node
#      is the one binary that gets -x. Anything unrecognised gets -x too.
#
#      Re-signing is not optional: arm64 SIGKILLs (exit 137) a binary whose
#      signature `strip` invalidated, and `codesign -v` then reports
#      "invalid signature (code or signature have been modified)".
#
#   2. drop npm payload nothing in the bundle loads:
#        typescript/lib/_tsc.js + tsc.js   the `tsc` CLI (5.9 MB). Only the
#          language server ships, and it drives _tsserver.js; nothing in the
#          bundle creates a `tsc` wrapper.
#        node_modules/**/*.map             sourcemaps (4.6 MB, 71 files).
#
# Idempotent: a `.slim` marker records the recipe, so a second run is a no-op
# and the marker survives `ditto` into the .app.
#
# Usage:
#   ./scripts/lsp-slim.sh <lsp-dir>              # slim in place
#   ./scripts/lsp-slim.sh <lsp-dir> --force      # redo even if already slim
#   ./scripts/lsp-slim.sh <lsp-dir> --dry-run    # report only, change nothing
#   ./scripts/lsp-slim.sh <lsp-dir> --quiet      # suppress per-file lines
set -euo pipefail

# Bump when the recipe changes so existing trees are re-processed.
RECIPE="lsp-slim/2 node=-x shellcheck=strip rust-analyzer=strip prune=tsc,maps,tests,locales"
MARKER_NAME=".slim"

# Binaries where bare `strip` is verified safe (shellcheck: real analysis of a
# script with SC2045/SC2086; rust-analyzer: full LSP `initialize` handshake).
AGGRESSIVE=("bin/shellcheck" "bin/rust-analyzer")
# Binaries that must keep their non-local symbols for dlsym.
CONSERVATIVE=("node/bin/node")

DIR=""
FORCE=0
DRY=0
QUIET=0

usage() {
  sed -n '2,/^set -euo/p' "$0" | sed -e '/^set -euo/d' -e 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1 ;;
    --dry-run) DRY=1 ;;
    --quiet|-q) QUIET=1 ;;
    -h|--help) usage 0 ;;
    -*) echo "unknown option: $1" >&2; usage 1 ;;
    *) DIR="$1" ;;
  esac
  shift
done

[[ -n "$DIR" ]] || { echo "error: missing <lsp-dir>" >&2; usage 1; }
[[ -d "$DIR" ]] || { echo "error: not a directory: $DIR" >&2; exit 1; }
DIR="$(cd "$DIR" && pwd)"
MARKER="$DIR/$MARKER_NAME"

log()  { printf '==> %s\n' "$*" >&2; }
note() { [[ "$QUIET" -eq 1 ]] || printf '    %s\n' "$*" >&2; }
die()  { echo "error: $*" >&2; exit 1; }

# Bytes → "12.3 MB" (no unit for the awk side; callers format).
mb() { awk -v b="$1" 'BEGIN { printf "%.1f MB", b / 1048576 }'; }
size_of() { wc -c < "$1" | tr -d ' '; }
tree_bytes() { find "$1" -type f -exec wc -c {} + 2>/dev/null | awk '$2!="total"{s+=$1} END{print s+0}'; }

if [[ "$FORCE" -eq 0 && -f "$MARKER" && "$(cat "$MARKER")" == "$RECIPE" ]]; then
  log "already slim ($(du -sh "$DIR" | cut -f1)) — use --force to redo"
  exit 0
fi

BEFORE="$(tree_bytes "$DIR")"
STRIP_SAVED=0
PRUNE_SAVED=0

# --- 1. strip + re-sign ---------------------------------------------------

is_macho() { file -b "$1" 2>/dev/null | grep -q 'Mach-O'; }

# True when the binary carries a real signing identity. Stripping it would
# throw that away, so leave Developer ID / App Store builds alone.
has_identity() {
  codesign -dv "$1" 2>&1 | grep -q '^Authority='
}

# slim_binary <path> <strip-args...>
slim_binary() {
  local f="$1"; shift
  [[ -f "$f" ]] || return 0
  is_macho "$f" || return 0
  if has_identity "$f"; then
    note "skip $(basename "$f") — carries a signing identity"
    return 0
  fi

  local before after
  before="$(size_of "$f")"
  if [[ "$DRY" -eq 1 ]]; then
    note "would strip $(basename "$f") ($(mb "$before"))"
    return 0
  fi

  strip "$@" "$f" 2>/dev/null || { log "warning: strip failed on $f — left as-is"; return 0; }
  # arm64 refuses to exec a binary whose signature strip invalidated.
  codesign --force --sign - "$f" >/dev/null 2>&1 \
    || die "re-sign failed for $f (arm64 would SIGKILL it)"

  after="$(size_of "$f")"
  STRIP_SAVED=$(( STRIP_SAVED + before - after ))
  note "$(printf '%-22s %10s -> %10s' "$(basename "$f")" "$(mb "$before")" "$(mb "$after")")"
}

log "stripping bundled binaries"
for rel in "${AGGRESSIVE[@]}";    do slim_binary "$DIR/$rel"; done
for rel in "${CONSERVATIVE[@]}";  do slim_binary "$DIR/$rel" -x; done

# Anything else that is Mach-O gets the conservative treatment: an unknown
# binary may be a .node addon whose exported symbols are load-bearing.
while IFS= read -r f; do
  case "$f" in
    "$DIR/bin/shellcheck"|"$DIR/bin/rust-analyzer"|"$DIR/node/bin/node") continue ;;
  esac
  slim_binary "$f" -x
done < <(find "$DIR" -type f -perm -u+x 2>/dev/null)

# --- 2. prune npm payload nothing loads -----------------------------------

prune_path() {
  local p="$1" why="$2" bytes
  [[ -e "$p" ]] || return 0
  bytes="$(size_of "$p")"
  if [[ "$DRY" -eq 1 ]]; then
    note "would prune ${p#"$DIR"/} ($(mb "$bytes")) — $why"
    return 0
  fi
  rm -f "$p" || { log "warning: could not prune $p"; return 0; }
  PRUNE_SAVED=$(( PRUNE_SAVED + bytes ))
  note "$(printf '%-22s -%10s' "$(basename "$p")" "$(mb "$bytes")")  $why"
}

prune_dir() {
  local p="$1" why="$2" bytes
  [[ -d "$p" ]] || return 0
  bytes="$(tree_bytes "$p")"
  if [[ "$DRY" -eq 1 ]]; then
    note "would prune dir ${p#"$DIR"/} ($(mb "$bytes")) — $why"
    return 0
  fi
  rm -rf "$p" || { log "warning: could not prune $p"; return 0; }
  PRUNE_SAVED=$(( PRUNE_SAVED + bytes ))
  note "$(printf '%-22s -%10s' "$(basename "$p")" "$(mb "$bytes")")  $why"
}

log "pruning unused npm payload"
prune_path "$DIR/node_modules/typescript/lib/_tsc.js" "tsc CLI — tsserver never loads it"
prune_path "$DIR/node_modules/typescript/lib/tsc.js"  "shim for the pruned tsc CLI"
prune_path "$DIR/node_modules/typescript/lib/typescript.d.ts" "typescript types"
prune_dir "$DIR/node_modules/@mixmark-io/domino/test" "domino test fixtures"
prune_dir "$DIR/node_modules/zod/src" "zod source files"
for loc in cs de es fr it ja ko pl pt-br ru tr zh-cn zh-tw; do
  prune_dir "$DIR/node_modules/typescript/lib/$loc" "typescript locale bundle ($loc)"
done

if [[ -d "$DIR/node_modules" ]]; then
  map_bytes=0
  map_count=0
  while IFS= read -r f; do
    map_bytes=$(( map_bytes + $(size_of "$f") ))
    map_count=$(( map_count + 1 ))
    [[ "$DRY" -eq 1 ]] || rm -f "$f"
  done < <(find "$DIR/node_modules" -type f -name '*.map' 2>/dev/null)

  if [[ "$map_count" -gt 0 ]]; then
    PRUNE_SAVED=$(( PRUNE_SAVED + map_bytes ))
    note "$(printf '%-22s -%10s' "$map_count sourcemaps" "$(mb "$map_bytes")")"
  fi
fi

# --- 3. report ------------------------------------------------------------

if [[ "$DRY" -eq 1 ]]; then
  log "dry run — nothing changed"
  exit 0
fi

# Verify the two runtime binaries actually still run. This is the check that
# catches an over-eager strip before it reaches a user.
verify() {
  local f="$1" label="$2"
  [[ -x "$f" ]] || return 0
  if ! "$f" --version >/dev/null 2>&1; then
    die "$label does not run after slimming ($f) — refusing to ship it"
  fi
}
verify "$DIR/node/bin/node" "node"
verify "$DIR/bin/shellcheck" "shellcheck"
verify "$DIR/bin/rust-analyzer" "rust-analyzer"

printf '%s' "$RECIPE" > "$MARKER"

AFTER="$(tree_bytes "$DIR")"
log "slim: $(mb "$BEFORE") -> $(mb "$AFTER")  (-$(mb "$(( BEFORE - AFTER ))"); $(du -sh "$DIR" | cut -f1) on disk)"
