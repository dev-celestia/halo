#!/usr/bin/env bash
# Recreate vendor/gpui-{base,component} from the crates.io registry and apply
# Halo hover-popover polish patches. Safe to re-run.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fetch_crate() {
  local name="$1"
  local version="$2"
  local dest="$3"
  local src
  src="$(ls -d "${CARGO_HOME:-$HOME/.cargo}/registry/src"/*/"${name}-${version}" 2>/dev/null | head -1 || true)"
  if [[ -z "$src" || ! -d "$src" ]]; then
    echo "error: ${name}-${version} not in cargo registry; run: cargo fetch" >&2
    exit 1
  fi
  rm -rf "$dest"
  cp -R "$src" "$dest"
  # Path deps should not carry registry metadata that confuses some tools.
  rm -f "$dest/.cargo-ok" "$dest/.cargo_vcs_info.json"
}

apply_patch() {
  local dir="$1"
  local patch="$2"
  patch -d "$dir" -p1 --forward --batch <"$patch" || {
    echo "error: failed to apply $patch" >&2
    exit 1
  }
}

mkdir -p vendor patches

# Already applied (markers from our patches).
if [[ -f vendor/gpui-base/src/text/node.rs ]] \
  && grep -q 'fn code_block_is_bare' vendor/gpui-base/src/text/node.rs \
  && grep -q 'fn format_diagnostic_markdown' \
    vendor/gpui-component/src/input/popovers/diagnostic_popover.rs 2>/dev/null; then
  echo "vendor/gpui-base and vendor/gpui-component already patched."
  exit 0
fi

# Ensure crates are downloaded (no-op if already present).
cargo fetch -q

fetch_crate gpui-base 0.7.2 vendor/gpui-base
fetch_crate gpui-component 0.7.1 vendor/gpui-component

apply_patch vendor/gpui-base patches/gpui-base-bare-codeblock.patch
apply_patch vendor/gpui-component patches/gpui-component-popover-markdown.patch
apply_patch vendor/gpui-component patches/gpui-component-hover-padding.patch
apply_patch vendor/gpui-component patches/gpui-component-diagnostic-code.patch
echo "vendor/gpui-base and vendor/gpui-component are ready."
