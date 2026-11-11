#!/usr/bin/env bash
# Release-build Halo UI.
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/ensure-gpui-vendor.sh
exec cargo build --release -p halo "$@"
