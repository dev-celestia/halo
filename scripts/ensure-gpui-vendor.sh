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
    vendor/gpui-component/src/input/popovers/diagnostic_popover.rs 2>/dev/null \
  && grep -q 'pub fn syntax_error_spans' \
    vendor/gpui-component/src/highlighter/highlighter.rs 2>/dev/null; then
  echo "vendor/gpui-base and vendor/gpui-component already patched."
  exit 0
fi

# Workspace Cargo.toml patches these crates to vendor/*, so `cargo fetch` in
# the workspace fails until vendor/ exists. Prefetch via a throwaway manifest.
ensure_registry_crate() {
  local name="$1"
  local version="$2"
  local src
  src="$(ls -d "${CARGO_HOME:-$HOME/.cargo}/registry/src"/*/"${name}-${version}" 2>/dev/null | head -1 || true)"
  if [[ -n "$src" && -d "$src" ]]; then
    return 0
  fi
  local tmp
  tmp="$(mktemp -d)"
  mkdir -p "$tmp/src"
  cat >"$tmp/Cargo.toml" <<EOF
[package]
name = "halo-vendor-fetch"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
${name} = "=${version}"
EOF
  echo 'fn main() {}' >"$tmp/src/main.rs"
  (cd "$tmp" && cargo fetch -q)
  rm -rf "$tmp"
}

ensure_registry_crate gpui-base 0.7.2
ensure_registry_crate gpui-component 0.7.1

fetch_crate gpui-base 0.7.2 vendor/gpui-base
fetch_crate gpui-component 0.7.1 vendor/gpui-component

apply_patch vendor/gpui-base patches/gpui-base-bare-codeblock.patch
apply_patch vendor/gpui-base patches/gpui-base-editor-perf.patch
apply_patch vendor/gpui-component patches/gpui-component-popover-markdown.patch
apply_patch vendor/gpui-component patches/gpui-component-hover-padding.patch
apply_patch vendor/gpui-component patches/gpui-component-diagnostic-code.patch
apply_patch vendor/gpui-component patches/gpui-component-syntax-lint.patch
apply_patch vendor/gpui-component patches/gpui-component-ui-polish.patch
find vendor/gpui-base vendor/gpui-component -name "*.orig" -delete
echo "vendor/gpui-base and vendor/gpui-component are ready."
