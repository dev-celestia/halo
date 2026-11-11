#!/usr/bin/env bash
# Stage the language servers that ship inside Halo.app.
#
# Layout (vendor/lsp, copied to Contents/Resources/lsp by bundle-macos.sh):
#   bin/bash-language-server     wrapper → bundled Node + npm package
#   bin/typescript-language-server
#   bin/shellcheck               native binary (GPLv3)
#   bin/rust-analyzer            native binary
#   node/bin/node                pinned Node runtime
#   node_modules/                npm packages (bash-lsp, tls, typescript@5)
#   licenses/                    third-party license texts
#
# Everything is version- and SHA-256-pinned. JavaScript packages are
# architecture-independent; the native binaries are per-arch.
#
# The last step is `scripts/lsp-slim.sh`, which strips the native binaries
# (re-signing them ad-hoc, or arm64 will not exec them) and drops npm payload
# nothing loads. Upstream bytes total 254 MB; the slim tree is 189 MB on disk.
# Pass --no-slim to keep the untouched upstream copies.
#
# Usage:
#   ./scripts/fetch-lsp.sh            # stage for the current Mac
#   ./scripts/fetch-lsp.sh --force    # restage even if the stamp matches
#   ./scripts/fetch-lsp.sh --no-slim  # keep the upstream (unstripped) bytes
#
# Env:
#   LSP_STAGE_DIR   override vendor/lsp
#   LSP_CACHE_DIR   override vendor/lsp-cache
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: bundled language servers are macOS-only" >&2
  exit 1
fi

NODE_VERSION="22.23.4"
NODE_SHA_ARM64="c9e1dbc748b33ed2d14f42445191ecaf14c56259c7a9a26d0042c49ec2b66428"
NODE_SHA_X64="efcc2b0db6f94bf53eddb038fe2b370277a042bd29e4f19b8f8e23598795b819"

SHELLCHECK_VERSION="0.11.0"
SHELLCHECK_SHA_ARM64="339b930feb1ea764467013cc1f72d09cd6b869ebf1013296ba9055ab2ffbd26f"
SHELLCHECK_SHA_X64="c2c15e08df0e8fbc374c335b230a7ee958c313fa5714817a59aa59f1aa594f51"

RUST_ANALYZER_VERSION="2026-10-19"
RUST_ANALYZER_SHA_ARM64="eedad08a96f7bf92afd0890463787337d17591ea2e3df216033c6bc3ccc9d802"
RUST_ANALYZER_SHA_X64="bdfd46e82731a8d3502d852d28b6462c1f1c13eed0b3e76bbcf1ba839028249c"

BASH_LS_VERSION="5.8.1"
TS_LS_VERSION="6.0.2"
# typescript-language-server needs `tsserver.js`; TypeScript 7 removed it.
TS_VERSION="5.9.3"

FORCE=0
DO_SLIM=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1 ;;
    --no-slim) DO_SLIM=0 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed -e '/^set -euo/d' -e 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64)
    NODE_ARCH="arm64"
    SHELLCHECK_ARCH="aarch64"
    RA_ARCH="aarch64"
    NODE_SHA="$NODE_SHA_ARM64"
    SHELLCHECK_SHA="$SHELLCHECK_SHA_ARM64"
    RA_SHA="$RUST_ANALYZER_SHA_ARM64"
    ;;
  x86_64)
    NODE_ARCH="x64"
    SHELLCHECK_ARCH="x86_64"
    RA_ARCH="x86_64"
    NODE_SHA="$NODE_SHA_X64"
    SHELLCHECK_SHA="$SHELLCHECK_SHA_X64"
    RA_SHA="$RUST_ANALYZER_SHA_X64"
    ;;
  *)
    echo "error: unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

LSP_DIR="${LSP_STAGE_DIR:-$ROOT/vendor/lsp}"
CACHE_DIR="${LSP_CACHE_DIR:-$ROOT/vendor/lsp-cache}"
STAMP="$LSP_DIR/.stamp"
STAMP_TEXT="node=$NODE_VERSION shellcheck=$SHELLCHECK_VERSION rust-analyzer=$RUST_ANALYZER_VERSION bash-ls=$BASH_LS_VERSION ts-ls=$TS_LS_VERSION typescript=$TS_VERSION arch=$ARCH"

# stderr so `fetch` command substitution only captures the file path.
log() { printf '==> %s\n' "$*" >&2; }
die() { echo "error: $*" >&2; exit 1; }

# Strip + prune the staged tree. Idempotent (own `.slim` marker), so this is
# safe to run on a tree that was staged before the slim step existed.
slim() {
  if [[ "$DO_SLIM" -eq 0 ]]; then
    log "keeping upstream bytes (--no-slim)"
    return 0
  fi
  "$ROOT/scripts/lsp-slim.sh" "$LSP_DIR" || die "slimming $LSP_DIR failed"
}

if [[ "$FORCE" -eq 0 && -f "$STAMP" && "$(cat "$STAMP")" == "$STAMP_TEXT" ]]; then
  log "language servers already staged ($ARCH) — use --force to restage"
  slim
  exit 0
fi

command -v curl >/dev/null || die "curl is required"
command -v shasum >/dev/null || die "shasum is required"
command -v npm >/dev/null || die "npm is required to stage the JS language servers"
command -v tar >/dev/null || die "tar is required"

mkdir -p "$CACHE_DIR" "$LSP_DIR/bin" "$LSP_DIR/licenses"

# fetch URL SHA DEST_NAME → verified file path in the cache
fetch() {
  local url="$1" sha="$2" name="$3" path="$CACHE_DIR/$3"
  if [[ ! -f "$path" ]]; then
    log "downloading $name"
    curl -fsSL --retry 3 --connect-timeout 15 -o "$path.part" "$url"
    mv "$path.part" "$path"
  fi
  printf '%s  %s\n' "$sha" "$path" | shasum -a 256 -c - >/dev/null \
    || die "checksum mismatch for $name"
  printf '%s\n' "$path"
}

NODE_TAR_NAME="node-v${NODE_VERSION}-darwin-${NODE_ARCH}.tar.gz"
NODE_TAR="$(fetch \
  "https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TAR_NAME}" \
  "$NODE_SHA" "$NODE_TAR_NAME")"

SHELLCHECK_TAR_NAME="shellcheck-v${SHELLCHECK_VERSION}.darwin.${SHELLCHECK_ARCH}.tar.gz"
SHELLCHECK_TAR="$(fetch \
  "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/${SHELLCHECK_TAR_NAME}" \
  "$SHELLCHECK_SHA" "$SHELLCHECK_TAR_NAME")"

RA_GZ_NAME="rust-analyzer-${RA_ARCH}-apple-darwin.gz"
RA_GZ="$(fetch \
  "https://github.com/rust-lang/rust-analyzer/releases/download/${RUST_ANALYZER_VERSION}/${RA_GZ_NAME}" \
  "$RA_SHA" "$RA_GZ_NAME")"

log "unpacking Node $NODE_VERSION ($NODE_ARCH)"
rm -rf "$CACHE_DIR/node-extract" "$CACHE_DIR/shellcheck-extract"
mkdir -p "$CACHE_DIR/node-extract" "$LSP_DIR/node/bin"
tar -xzf "$NODE_TAR" -C "$CACHE_DIR/node-extract" \
  "node-v${NODE_VERSION}-darwin-${NODE_ARCH}/bin/node" \
  "node-v${NODE_VERSION}-darwin-${NODE_ARCH}/LICENSE"
NODE_EXTRACT="$CACHE_DIR/node-extract/node-v${NODE_VERSION}-darwin-${NODE_ARCH}"
cp "$NODE_EXTRACT/bin/node" "$LSP_DIR/node/bin/node"
chmod 755 "$LSP_DIR/node/bin/node"
cp "$NODE_EXTRACT/LICENSE" "$LSP_DIR/licenses/node-LICENSE.txt"

log "unpacking ShellCheck $SHELLCHECK_VERSION ($SHELLCHECK_ARCH)"
mkdir -p "$CACHE_DIR/shellcheck-extract"
tar -xzf "$SHELLCHECK_TAR" -C "$CACHE_DIR/shellcheck-extract"
SC_EXTRACT="$CACHE_DIR/shellcheck-extract/shellcheck-v${SHELLCHECK_VERSION}"
cp "$SC_EXTRACT/shellcheck" "$LSP_DIR/bin/shellcheck"
chmod 755 "$LSP_DIR/bin/shellcheck"
[[ -f "$SC_EXTRACT/LICENSE.txt" ]] && cp "$SC_EXTRACT/LICENSE.txt" "$LSP_DIR/licenses/shellcheck-LICENSE.txt"

log "unpacking rust-analyzer $RUST_ANALYZER_VERSION ($RA_ARCH)"
gunzip -c "$RA_GZ" > "$LSP_DIR/bin/rust-analyzer"
chmod 755 "$LSP_DIR/bin/rust-analyzer"

log "installing JS language servers"
# Keep npm's cache local: a root-run dev session can leave ~/.npm unwritable.
npm install \
  --prefix "$LSP_DIR" \
  --cache "$CACHE_DIR/npm-cache" \
  --omit=dev \
  --ignore-scripts \
  --no-audit \
  --no-fund \
  --no-package-lock \
  --loglevel=error \
  "bash-language-server@${BASH_LS_VERSION}" \
  "typescript-language-server@${TS_LS_VERSION}" \
  "typescript@${TS_VERSION}"

# npm shims use `#!/usr/bin/env node`; wrappers make the bundled runtime and
# ShellCheck discoverable regardless of the user's PATH.
write_wrapper() {
  local name="$1" entry="$2"
  cat > "$LSP_DIR/bin/$name" <<EOF
#!/bin/sh
set -e
DIR=\$(CDPATH= cd -- "\$(dirname -- "\$0")/.." && pwd)
PATH="\$DIR/bin:\$DIR/node/bin:\$PATH"
export PATH
exec "\$DIR/node/bin/node" "\$DIR/node_modules/${entry}" "\$@"
EOF
  chmod 755 "$LSP_DIR/bin/$name"
}

write_wrapper "bash-language-server" "bash-language-server/out/cli.js"
write_wrapper "typescript-language-server" "typescript-language-server/lib/cli.mjs"

for pkg in bash-language-server typescript-language-server typescript; do
  for name in LICENSE LICENSE.txt LICENSE.md; do
    if [[ -f "$LSP_DIR/node_modules/$pkg/$name" ]]; then
      cp "$LSP_DIR/node_modules/$pkg/$name" "$LSP_DIR/licenses/$pkg-LICENSE.txt"
      break
    fi
  done
done

cat > "$LSP_DIR/THIRD_PARTY.md" <<EOF
# Bundled language servers

| Component | Version | License | Source |
| --- | --- | --- | --- |
| Node.js | ${NODE_VERSION} | MIT | https://nodejs.org |
| TypeScript | ${TS_VERSION} | Apache-2.0 | https://github.com/microsoft/TypeScript |
| typescript-language-server | ${TS_LS_VERSION} | Apache-2.0 | https://github.com/typescript-language-server/typescript-language-server |
| bash-language-server | ${BASH_LS_VERSION} | MIT | https://github.com/bash-lsp/bash-language-server |
| ShellCheck | ${SHELLCHECK_VERSION} | GPL-3.0 | https://github.com/koalaman/shellcheck |
| rust-analyzer | ${RUST_ANALYZER_VERSION} | MIT OR Apache-2.0 | https://github.com/rust-lang/rust-analyzer |

ShellCheck is GPL-3.0. Its source is available at the URL above; the full
license text ships in \`licenses/shellcheck-LICENSE.txt\`.
EOF

# Slim before stamping, so the stamp means "staged and ready to ship".
slim

printf '%s' "$STAMP_TEXT" > "$STAMP"
log "staged in $LSP_DIR"
du -sh "$LSP_DIR"
