#!/usr/bin/env bash
# Run workspace tests.
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo test --workspace "$@"
