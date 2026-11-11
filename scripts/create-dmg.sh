#!/usr/bin/env bash
# Wrapper to create a macOS Drag-and-Drop DMG Installer.
#
# Usage:
#   ./scripts/create-dmg.sh <path-to-app> <path-to-dmg> [--volname <name>] [--sign <identity>]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$ROOT/scripts/create-dmg.py" "$@"
