#!/usr/bin/env bash
# Bundle Halo as a macOS .app (and optionally a .dmg).
#
# Usage:
#   ./scripts/bundle-macos.sh              # release .app → dist/Halo.app (~60 MB)
#   ./scripts/bundle-macos.sh --with-lsp   # include offline language servers (~250 MB)
#   ./scripts/bundle-macos.sh --dmg        # also create dist/Halo-<ver>-<arch>.dmg
#   ./scripts/bundle-macos.sh --install    # copy .app into /Applications
#   ./scripts/bundle-macos.sh --skip-build # reuse existing target/release/Halo
#   ./scripts/bundle-macos.sh --no-lsp     # bundle without the language servers (default)
#   ./scripts/bundle-macos.sh --no-slim    # keep the language servers unstripped
#
# Language servers (Bash + ShellCheck, TypeScript/JS, rust-analyzer) are staged
# by scripts/fetch-lsp.sh into vendor/lsp and copied to Contents/Resources/lsp.
# Missing staging is an error unless --no-lsp is passed.
#
# The copy is then run through scripts/lsp-slim.sh (before the bundle is
# signed), which strips the nested Mach-Os and drops unused npm payload:
# 254 MB of language servers becomes 189 MB, so the .app is ~250 MB instead of
# ~315 MB. `--no-slim` skips it, `--no-lsp` drops the payload entirely (~60 MB
# .app that resolves language servers from PATH instead).
#
# The icon comes from crates/ui/assets/icons/ (the canonical generated set): the
# .icns is copied as-is, and any point size it lacks is filled from the largest
# PNG so the Dock never has to upscale. With --dmg the volume gets the same mark.
#
# Env overrides: VERSION, DIST_DIR, CODESIGN_IDENTITY
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS bundling requires Darwin" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Halo identity — fixed; this script only bundles the Halo app.
APP_NAME="Halo"
BUNDLE_ID="com.halo.app"
BINARY_NAME="Halo"
DIST_DIR="${DIST_DIR:-$ROOT/dist}"

# Icon sources: `crates/ui/assets/icons/` — PNG ladder (16…512 plus @2x) and a
# multi-resolution `icon.icns`. `icon.rs` embeds `icons/icon.png` from the same
# directory for the dev-run window / Dock icon, so the bundle and a dev run
# always show the same mark.
ICON_DIR="$ROOT/crates/ui/assets/icons"
ICON_ICNS="$ICON_DIR/icon.icns"
ICON_PNG_CANDIDATES=(
  "$ICON_DIR/icon.png"
  "$ICON_DIR/128x128@2x.png"
)
# Point sizes macOS expects in an .icns (each also wants an @2x entry).
ICNS_SIZES=(16 32 128 256 512)

VERSION="${VERSION:-}"
if [[ -z "$VERSION" ]]; then
  VERSION="$(
    cargo metadata --no-deps --format-version 1 2>/dev/null \
      | python3 -c "
import json, sys
meta = json.load(sys.stdin)
for p in meta.get('packages', []):
    if p.get('name') == 'halo':
        print(p['version'])
        break
" 2>/dev/null || true
  )"
fi
VERSION="${VERSION:-0.1.0}"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64) ARCH_LABEL="aarch64" ;;
  x86_64) ARCH_LABEL="x86_64" ;;
  *) ARCH_LABEL="$ARCH" ;;
esac

DO_BUILD=1
DO_DMG=0
DO_INSTALL=0
DO_SIGN=1
DO_LSP=0
DO_SLIM=1

usage() {
  sed -n '2,/^set -euo/p' "$0" | sed -e '/^set -euo/d' -e 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) DO_DMG=1 ;;
    --install) DO_INSTALL=1 ;;
    --skip-build) DO_BUILD=0 ;;
    --no-sign) DO_SIGN=0 ;;
    --with-lsp) DO_LSP=1 ;;
    --no-lsp) DO_LSP=0 ;;
    --no-slim) DO_SLIM=0 ;;
    -h|--help) usage 0 ;;
    *)
      echo "unknown option: $1" >&2
      usage 1
      ;;
  esac
  shift
done

APP_BUNDLE="$DIST_DIR/${APP_NAME}.app"
CONTENTS="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RESOURCES_DIR="$CONTENTS/Resources"

# Respect CARGO_TARGET_DIR (and cargo metadata) so sandbox / CI overrides work.
TARGET_DIR="$(
  cargo metadata --no-deps --format-version 1 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['target_directory'])" \
    2>/dev/null || echo "$ROOT/target"
)"
BINARY_SRC="$TARGET_DIR/release/$BINARY_NAME"

log() { printf '==> %s\n' "$*"; }

# --- icon helpers -------------------------------------------------------

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

# Pixel width of a raster, or nothing when `sips` cannot read it.
png_width() {
  sips -g pixelWidth "$1" 2>/dev/null | awk '/pixelWidth:/ {print $2; exit}'
}

# Echo the largest of the given rasters (by pixel width); empty when none read.
largest_png() {
  local best="" best_px=0 f px
  for f in "$@"; do
    [[ -f "$f" ]] || continue
    px="$(png_width "$f")"
    [[ -n "$px" ]] || continue
    if (( px > best_px )); then
      best_px="$px"
      best="$f"
    fi
  done
  [[ -n "$best" ]] && printf '%s\n' "$best"
  return 0
}

# How many sizes an .icns carries ("10 sizes", or "unreadable").
icns_sizes() {
  local tmp count
  tmp="$(mktemp -d)"
  if iconutil -c iconset "$1" -o "$tmp/i.iconset" 2>/dev/null; then
    count="$(find "$tmp/i.iconset" -name '*.png' | wc -l | tr -d ' ')"
    printf '%s sizes' "$count"
  else
    printf 'unreadable'
  fi
  rm -rf "$tmp"
}

# Echo "<iconset filename> <pixel size>" for a point size and 1x / 2x scale.
iconset_entry() {
  local size="$1" scale="$2"
  if (( scale == 2 )); then
    printf 'icon_%sx%s@2x.png %s\n' "$size" "$size" "$((size * 2))"
  else
    printf 'icon_%sx%s.png %s\n' "$size" "$size" "$size"
  fi
}

# Build a complete .icns (every point size, both scales) from one raster.
build_icns() {
  local png="$1" out="$2" tmp size scale name px
  tmp="$(mktemp -d)"
  mkdir -p "$tmp/icon.iconset"
  for size in "${ICNS_SIZES[@]}"; do
    for scale in 1 2; do
      read -r name px <<<"$(iconset_entry "$size" "$scale")"
      sips -z "$px" "$px" "$png" --out "$tmp/icon.iconset/$name" >/dev/null
    done
  done
  iconutil -c icns "$tmp/icon.iconset" -o "$out"
  rm -rf "$tmp"
}

# Fill in the sizes an .icns is missing, in place, from `$2`.
#
# A partial .icns (say 16…256 only) makes the Dock and Finder substitute a
# smaller entry and upscale it, which reads as a blurry icon — exactly the
# symptom a 234px source produces once sips stretches it to 1024.
repair_icns() {
  local icns="$1" png="$2" tmp size scale name px filled=0
  tmp="$(mktemp -d)"
  if ! iconutil -c iconset "$icns" -o "$tmp/icon.iconset" 2>/dev/null; then
    rm -rf "$tmp"
    return 1
  fi
  for size in "${ICNS_SIZES[@]}"; do
    for scale in 1 2; do
      read -r name px <<<"$(iconset_entry "$size" "$scale")"
      [[ -f "$tmp/icon.iconset/$name" ]] && continue
      sips -z "$px" "$px" "$png" --out "$tmp/icon.iconset/$name" >/dev/null
      filled=$((filled + 1))
    done
  done
  if (( filled > 0 )); then
    iconutil -c icns "$tmp/icon.iconset" -o "$icns"
    log "icon: filled $filled missing size(s) from $(basename "$png")"
  fi
  rm -rf "$tmp"
}

# Drop the chosen icon into Contents/Resources and report where it came from.
install_icon() {
  local dest="$RESOURCES_DIR/icon.icns" png
  png="$(largest_png "${ICON_PNG_CANDIDATES[@]}")"

  if [[ -f "$ICON_ICNS" ]]; then
    cp "$ICON_ICNS" "$dest"
    [[ -n "$png" ]] && repair_icns "$dest" "$png" || true
    log "icon: ${ICON_ICNS#"$ROOT"/} ($(icns_sizes "$dest"))"
    return 0
  fi

  if [[ -n "$png" ]]; then
    log "icon: synthesising icon.icns from ${png#"$ROOT"/} ($(png_width "$png")px)"
    build_icns "$png" "$dest"
    log "icon: generated ($(icns_sizes "$dest"))"
    return 0
  fi

  echo "warning: no icon.icns / icon.png under ${ICON_DIR#"$ROOT"/} — bundle will have no icon" >&2
  return 0
}

# `icon.rs` embeds the same `icons/icon.png` this script reads, so the dev-run
# Dock icon and the bundled `.app` cannot drift. Guard the shared source instead
# of comparing two copies: a missing PNG is a compile error in `crates/ui`.
require_embedded_icon() {
  local embedded="$ICON_DIR/icon.png"
  [[ -f "$embedded" ]] && return 0
  echo "warning: ${embedded#"$ROOT"/} is missing — crates/ui will not compile" >&2
  echo "warning: regenerate the set under ${ICON_DIR#"$ROOT"/} (icon.rs embeds its icon.png)" >&2
}

# Bump the bundle mtime and re-register it, so Finder / the Dock drop the
# cached icon instead of painting the previous one until the next login.
refresh_launch_services() {
  local app="$1"
  [[ -d "$app" ]] || return 0
  touch "$app"
  [[ -x "$LSREGISTER" ]] && "$LSREGISTER" -f "$app" >/dev/null 2>&1 || true
}

# The person who will drag the next copy into /Applications. A `sudo` run
# still installs as that user: a root-owned bundle cannot be replaced.
console_user() {
  if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
    printf '%s\n' "$SUDO_USER"
    return
  fi
  local user
  user="$(id -un)"
  if [[ "$user" != "root" ]]; then
    printf '%s\n' "$user"
    return
  fi
  user="$(stat -f %Su /dev/console 2>/dev/null || true)"
  if [[ -n "$user" && "$user" != "root" ]]; then
    printf '%s\n' "$user"
    return
  fi
  echo "error: cannot tell which user should own ${APP_NAME}.app" >&2
  exit 1
}

# Finder replaces an app by deleting the old bundle. A root-owned or locked
# copy makes that delete fail, and the drag ends with "some items had to be
# skipped". Hand the bundle to the console user and clear the locked flag.
own_bundle() {
  local bundle="$1"
  local owner group
  [[ -d "$bundle" ]] || return 0
  owner="$(console_user)"
  group="$(id -gn "$owner")"
  if ! chflags -R nouchg "$bundle" 2>/dev/null; then
    sudo chflags -R nouchg "$bundle"
  fi
  if [[ "$(stat -f %Su "$bundle")" != "$owner" ]]; then
    sudo chown -R "$owner:$group" "$bundle"
  fi
  # Capital X keeps execute on directories and binaries only, so the
  # signature's file modes stay as they were sealed.
  if ! chmod -R u+rwX "$bundle" 2>/dev/null; then
    sudo chmod -R u+rwX "$bundle"
  fi
}

if [[ "$DO_BUILD" -eq 1 ]]; then
  log "building release binary (Halo)"
  cargo build --release -p halo
  # The agent's browser CLI. A separate package, so the main build does not
  # produce it; it ships beside the app binary (probe::browser_cli looks there).
  log "building release binary (halo-browser)"
  cargo build --release -p halo-browser-tools --bin halo-browser
fi

if [[ ! -x "$BINARY_SRC" ]]; then
  echo "error: missing binary at $BINARY_SRC (run without --skip-build)" >&2
  exit 1
fi

log "assembling $APP_BUNDLE"
rm -rf "$APP_BUNDLE"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BINARY_SRC" "$MACOS_DIR/$BINARY_NAME"
chmod 755 "$MACOS_DIR/$BINARY_NAME"

# halo-browser lives in MacOS/, not Resources/: the harness resolves it beside
# the running app binary, which is the only directory it can trust without PATH.
BROWSER_TOOL_SRC="$TARGET_DIR/release/halo-browser"
if [[ -x "$BROWSER_TOOL_SRC" ]]; then
  cp "$BROWSER_TOOL_SRC" "$MACOS_DIR/halo-browser"
  chmod 755 "$MACOS_DIR/halo-browser"
else
  echo "warning: missing $BROWSER_TOOL_SRC; the agent will have no browser tools" >&2
fi

require_embedded_icon
install_icon

# --- third-party licence notices ---------------------------------------
# Vendored code is only usable closed-source if its licence texts travel with
# the binary (MIT + Apache-2.0 s4). Bundle licenses/ into the app so the DMG and
# the installed .app both carry them; see licenses/CUTLASS-NOTICE.
LIC_SRC="$ROOT/licenses"
LIC_DEST="$RESOURCES_DIR/licenses"
if [[ -d "$LIC_SRC" ]]; then
  log "bundling third-party licences ($(find "$LIC_SRC" -type f | wc -l | tr -d ' ') files)"
  rm -rf "$LIC_DEST"
  ditto "$LIC_SRC" "$LIC_DEST"
  if [[ -f "$ROOT/LICENSE" ]]; then
    cp "$ROOT/LICENSE" "$LIC_DEST/LICENSE-Halo"
  fi
else
  echo "warning: $LIC_SRC missing; the distributed build will ship no third-party notices" >&2
fi

LSP_SRC="$ROOT/vendor/lsp"
LSP_DEST="$RESOURCES_DIR/lsp"
if [[ "$DO_LSP" -eq 1 ]]; then
  if [[ ! -x "$LSP_SRC/bin/bash-language-server" ]]; then
    echo "error: language servers are not staged at $LSP_SRC" >&2
    echo "       run ./scripts/fetch-lsp.sh first, or pass --no-lsp" >&2
    exit 1
  fi
  log "copying language servers ($(du -sh "$LSP_SRC" | cut -f1))"
  # ditto preserves permissions, symlinks, and extended attributes.
  ditto "$LSP_SRC" "$LSP_DEST"
  if [[ "$DO_SLIM" -eq 1 ]]; then
    # Must run before the signing block below: `strip` invalidates the nested
    # binaries' signatures and arm64 will not exec them until they are
    # re-signed (lsp-slim.sh does that ad-hoc; codesign then re-seals them with
    # the real identity). No-op when fetch-lsp.sh already slimmed the source.
    "$ROOT/scripts/lsp-slim.sh" "$LSP_DEST"
  else
    log "keeping language servers unstripped (--no-slim)"
  fi
else
  log "skipping language servers (--no-lsp)"
fi

cat > "$CONTENTS/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>${APP_NAME}</string>
	<key>CFBundleExecutable</key>
	<string>${BINARY_NAME}</string>
	<key>CFBundleIconFile</key>
	<string>icon.icns</string>
	<key>CFBundleIdentifier</key>
	<string>${BUNDLE_ID}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>${APP_NAME}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${VERSION}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
EOF

# Empty PkgInfo keeps some Launch Services tooling happy.
printf 'APPL????' > "$CONTENTS/PkgInfo"

if [[ "$DO_SIGN" -eq 1 ]]; then
  # Ad-hoc by default (local builds). Set CODESIGN_IDENTITY to a "Developer ID
  # Application: ..." identity for a build you intend to hand to someone else;
  # that path also enables the hardened runtime and the entitlements file.
  CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
  ENTITLEMENTS="$ROOT/scripts/Halo.entitlements"
  NESTED_BINS=("$MACOS_DIR/halo-browser")
  if [[ "$DO_LSP" -eq 1 ]]; then
    # Sign nested Mach-Os before the bundle that seals them. Skipping this
    # leaves arm64 unable to exec unsigned copies and makes notarization reject
    # the bundle.
    NESTED_BINS=(
      "$MACOS_DIR/halo-browser"
      "$LSP_DEST/node/bin/node"
      "$LSP_DEST/bin/shellcheck"
      "$LSP_DEST/bin/rust-analyzer"
    )
  fi

  if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
    log "ad-hoc codesigning (identity=-) — local builds only"
    cat >&2 <<'WARN'
warning: an ad-hoc signature carries no identity and no integrity guarantee.
warning: a tampered copy of this bundle is indistinguishable from a good one,
warning: and this app installs a trusted root CA. Do not distribute it.
WARN
    for bin in "${NESTED_BINS[@]+"${NESTED_BINS[@]}"}"; do
      [[ -f "$bin" ]] && codesign --force --sign - "$bin"
    done
    # `--deep` is deprecated by Apple and masks signing problems; sign nested
    # executables explicitly above, then the bundle itself.
    codesign --force --sign - "$APP_BUNDLE"
  else
    log "codesigning with $CODESIGN_IDENTITY (hardened runtime)"
    for bin in "${NESTED_BINS[@]+"${NESTED_BINS[@]}"}"; do
      [[ -f "$bin" ]] || continue
      codesign --force --options runtime --timestamp \
        --entitlements "$ENTITLEMENTS" \
        --sign "$CODESIGN_IDENTITY" \
        "$bin"
    done
    codesign --force --options runtime --timestamp \
      --entitlements "$ENTITLEMENTS" \
      --sign "$CODESIGN_IDENTITY" \
      "$APP_BUNDLE"
    log "next: xcrun notarytool submit <dmg> --wait  (then xcrun stapler staple)"
  fi
fi

# Own it before the disk image is built, so a sudo bundle does not leave a
# root-owned app that the next drag cannot replace.
own_bundle "$APP_BUNDLE"

# Refresh Launch Services so Finder/Dock pick up the new bundle icon/name.
refresh_launch_services "$APP_BUNDLE"

DMG_PATH=""
if [[ "$DO_DMG" -eq 1 ]]; then
  DMG_PATH="$DIST_DIR/${APP_NAME}-${VERSION}-${ARCH_LABEL}.dmg"
  log "creating drag-and-drop installer: $DMG_PATH"
  CREATE_DMG_ARGS=(
    "$APP_BUNDLE"
    "$DMG_PATH"
    "--volname" "$APP_NAME"
  )
  if [[ -f "$RESOURCES_DIR/icon.icns" ]]; then
    CREATE_DMG_ARGS+=("--icon" "$RESOURCES_DIR/icon.icns")
  fi
  if [[ "$DO_SIGN" -eq 1 && -n "$CODESIGN_IDENTITY" && "$CODESIGN_IDENTITY" != "-" ]]; then
    CREATE_DMG_ARGS+=("--sign" "$CODESIGN_IDENTITY")
  fi
  python3 "$ROOT/scripts/create-dmg.py" "${CREATE_DMG_ARGS[@]}"
fi

if [[ "$DO_INSTALL" -eq 1 ]]; then
  DEST="/Applications/${APP_NAME}.app"
  log "installing to $DEST"
  # Remove the previous copy outright. Drag-replace merges into it, and any
  # file the user cannot delete is reported as skipped.
  if [[ -d "$DEST" ]]; then
    if ! chflags -R nouchg "$DEST" 2>/dev/null; then
      sudo chflags -R nouchg "$DEST"
    fi
    if [[ -w "$DEST" ]]; then
      rm -rf "$DEST"
    else
      log "removing the existing copy (it is not writable by this user)"
      sudo rm -rf "$DEST"
    fi
  fi
  ditto "$APP_BUNDLE" "$DEST"
  own_bundle "$DEST"
  refresh_launch_services "$DEST"
fi

log "done"
echo "  app:     $APP_BUNDLE"
echo "  size:    $(du -sh "$APP_BUNDLE" | cut -f1)"
if [[ "$DO_LSP" -eq 1 ]]; then
  echo "  lsp:     $(du -sh "$LSP_DEST" | cut -f1) of the above"
fi
echo "  version: $VERSION ($ARCH_LABEL)"
[[ -n "$DMG_PATH" ]] && echo "  dmg:     $DMG_PATH"
[[ "$DO_INSTALL" -eq 1 ]] && echo "  installed: /Applications/${APP_NAME}.app"
echo "  open:    open \"$APP_BUNDLE\""
