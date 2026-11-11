#!/usr/bin/env bash
# Format check + clippy + tests for the workspace.
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/ensure-gpui-vendor.sh

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test"
cargo test --workspace

echo "OK"
