# Halo

GPUI desktop app on [Celestia Desktop](../celestia-starter/packages/desktop). Proprietary software owned by Celestia; see [`LICENSE`](LICENSE).

Halo is a desktop shell (wallpaper, dock, launcher, work tabs) with focused tools for coding, source control, AI agents, notes, and proxy/security workflows. Debug builds expose every tool; shipping / `.app` builds only include **Release** apps.

## Apps

| App | Scope | Maturity |
| --- | --- | --- |
| **Coder** | Folder workspace, editor, run panel, LSP | Release |
| **Source Control** | Changes, commit, sync, graph / PRs | Release |
| **Agent** | Halo Agent — harnesses (Pi, OpenCode, Cursor, …) | Release |
| **Doctor** | Diagnose / install harnesses, LSPs, browser tools | Release |
| **Notes** | Tabbed markdown notepad + library | Release |
| **HTTP** | Captured traffic history / sessions | Alpha |
| **Intercept** | Pause, edit, forward / drop | Alpha |
| **Repeater** | Resend and compare requests | Alpha |
| **Intruder** | Payload fuzzing | Alpha |
| **Crawler** | Proxy-aware recon crawl | Alpha |
| **Diagram** | Vector diagram / flowchart / table canvas | Alpha |
| **API Mock** | Local mock server on `127.0.0.1` | Alpha |
| **API Override** | Replace live proxied responses | Alpha |
| **JWT** | Decode, audit, forge tokens | Alpha |
| **Port Scanner** | TCP connect-scan + banners | Alpha |
| **Comparer** | Side-by-side text diff | Alpha |
| **File Explorer** | Sandboxed local file workspace | Alpha |
| **R2 Storage** | Cloudflare R2 object browser | Alpha |
| **Wordlists** | Fuzzing wordlist catalog | Alpha |
| **Split View** | 2–4 tools in a multi-pane workspace | Alpha |
| **Settings** | Chrome, editor, terminal, agent, network, storage | Alpha |

Alpha apps are hidden from release / `.app` builds (`WorkKind::ships`). Theme: `⌘D` / `Ctrl+D`. Proxy start/stop lives under **Settings**.

## Run

```bash
make run          # Halo hot reload (or ./scripts/dev.sh)
make build        # release binary
make check        # fmt + clippy + test
make test
make mockup       # window chrome mockup (HALO_MOCKUP=window)
```

`make run` uses [cargo-watch](https://github.com/watchexec/cargo-watch) to rebuild and restart when workspace crates or path deps (e.g. Celestia UI) change. Installs `cargo-watch` once if missing.

One-shot / release equivalents:

```bash
cargo run -p halo
cargo build --release -p halo
```

Requires Rust ≥ 1.97 and a path to `celestia-starter/packages/desktop/crates/ui`.

## Headless CLI

`halo-cli` drives the same backend as the app, with no window and no GPUI linkage. It opens the same `halo.db`, so traffic captured by either side is visible to both.

```bash
make cli                  # cargo build -p halo-cli
make cli-install          # …and copy to ~/.local/bin
```

```bash
halo-cli proxy start --port 8888 --tls-port 8443     # foreground; --detach for background
halo-cli proxy status
halo-cli history ls -n 50 -s 4xx,5xx -H api.example.com
halo-cli history show <id>
halo-cli repeater send --raw request.txt
halo-cli scan 10.0.0.1 --ports 1-1024 --banner
halo-cli session ls
halo-cli library collection
halo-cli notes ls
halo-cli cert export ~/Desktop/halo-ca.pem
halo-cli storage info
```

Add `--json` to any command for machine-readable output. Full reference: **[docs/cli.md](docs/cli.md)**.

Wired today: `proxy`, `history`, `session`, `cert`, `storage`, `repeater`, `scan`, `library`, `notes`. Not yet on the CLI: intercept, intruder, crawler, mock forge, coder filesystem/git. `pty` and `agent` stay out of scope (need a streaming transport).

Every command is a function in `halo_backend::commands::<group>` taking `&BackendRuntime`; the desktop app reaches the same functions through `halo_bridge::NativeBackend`. Adding a CLI command means adding it to the command layer first — the binary stays a thin argument parser.

## macOS packaging

`scripts/bundle-macos.sh` builds a release binary and packs it into a double-clickable `.app` under `dist/`. Darwin only.

Language servers ship inside the app so a fresh install has working Bash/ShellCheck, TypeScript/JavaScript, and Rust diagnostics without npm, Homebrew, or rustup setup. Stage them once (≈254 MB upstream, per-arch, version- and SHA-256-pinned) before bundling; `scripts/lsp-slim.sh` then strips and prunes the payload to ≈189 MB on its way into the bundle:

```bash
make fetch-lsp           # vendor/lsp ← Node, bash-lsp, tls, ShellCheck, rust-analyzer
make bundle              # dist/Halo.app (~60–75 MB, resolves LSP from PATH)
make bundle-full         # dist/Halo.app with offline language servers (~251 MB)
make bundle-dmg          # .app + dist/Halo-<version>-<arch>.dmg
make bundle-full-dmg     # full .app + .dmg
make install-macos       # bundle and copy to /Applications

# or call the script directly:
./scripts/fetch-lsp.sh                 # stage vendor/lsp (runs lsp-slim.sh at the end)
./scripts/lsp-slim.sh vendor/lsp       # strip + prune an already-staged tree
./scripts/bundle-macos.sh              # release .app
./scripts/bundle-macos.sh --with-lsp   # include offline language servers
./scripts/bundle-macos.sh --dmg        # also create .dmg
./scripts/bundle-macos.sh --install    # also install to /Applications
./scripts/bundle-macos.sh --skip-build # reuse existing release binary
./scripts/bundle-macos.sh --no-sign    # skip ad-hoc codesign
./scripts/bundle-macos.sh --no-slim    # keep language servers unstripped (~316 MB app)
```

App size, measured with `--skip-build` against a ~60 MB release binary:

| Build | `.app` | `Resources/lsp` |
| --- | --- | --- |
| default | ~60–75 MB | — |
| `--with-lsp` (slim) | ~251 MB | ~189 MB |
| `--with-lsp --no-slim` | ~316 MB | ~254 MB |

The slim step is two levers: `strip` the nested Mach-Os (`node` keeps its non-local symbols, because bare `strip` breaks its `dlsym` lookups with `dyld: missing symbol called`), and drop npm payload nothing loads (`typescript/lib/_tsc.js`, `*.map`). Stripping invalidates the binaries' signatures, so each one is re-signed ad-hoc immediately afterwards — arm64 will not exec an invalidly-signed binary — and the bundle codesign then re-seals them with the real identity.

| Output | Path |
| --- | --- |
| App bundle | `dist/Halo.app` |
| Disk image | `dist/Halo-<version>-<arch>.dmg` (with `--dmg`) |
| Install | `/Applications/Halo.app` (with `--install`) |

Bundle layout:

```
Halo.app/
  Contents/
    Info.plist          # CFBundleIdentifier = com.halo.app
    MacOS/Halo          # release binary
    Resources/icon.icns # from crates/ui/assets/
    Resources/lsp/      # bundled language servers (omit with --no-lsp)
      bin/              # bash-language-server, typescript-language-server,
                        # shellcheck, rust-analyzer (wrappers/native binaries)
      node/             # pinned Node runtime for the JS servers
      node_modules/     # bash-language-server, typescript-language-server, TypeScript 5
      licenses/         # third-party license texts
      THIRD_PARTY.md
```

Open or install:

```bash
open dist/Halo.app
# or drag the .dmg volume’s Halo.app into Applications
```

Overrides (env):

| Variable | Default | Purpose |
| --- | --- | --- |
| `VERSION` | workspace `halo` package version | `CFBundleShortVersionString` |
| `DIST_DIR` | `dist/` | output directory |
| `CODESIGN_IDENTITY` | `-` (ad-hoc) | Developer ID for a distributable, hardened-runtime build |

Example:

```bash
VERSION=0.2.0 ./scripts/bundle-macos.sh --dmg
```

Notes:

- Ad-hoc codesign (`codesign --sign -`) runs by default so local launches are less friction; use `--no-sign` to skip.
- An ad-hoc signature carries **no identity and no integrity guarantee**. Since this app installs a trusted root CA, do not distribute an ad-hoc build. Set `CODESIGN_IDENTITY` to a Developer ID to sign with the hardened runtime and `scripts/Halo.entitlements`, then notarize the DMG (`xcrun notarytool submit … --wait && xcrun stapler staple …`).
- Never blanket-clear quarantine (`xattr -cr`) on a downloaded copy. If a notarized build is blocked, the signature or notarization is broken — fix that instead of teaching users to bypass Gatekeeper.
- Icon comes from the canonical set in `crates/ui/assets/icons/`: `icon.icns` is copied into `Contents/Resources` as-is, and any point size it lacks is filled from the largest PNG so the Dock never upscales a small bitmap. With `--dmg` the volume gets the same mark via `.VolumeIcon.icns` + `SetFile -a C` (needs the Xcode command line tools; skipped with a note otherwise). `icon.rs` embeds `icons/icon.png` from that same directory for the dev-run window / Dock icon, so the bundle and a dev run share one source and cannot drift.
- Bundled servers are detected ahead of `PATH` and their processes get the bundled `bin`/`node` directories prepended to `PATH`. `HALO_LSP_DIR` overrides the payload location for portable builds and tests.
- Python LSP (Pyright) is **not** bundled; install it on demand from Coder → Language Servers (`npm install -g pyright`).
- Bundled rust-analyzer still needs the user's Rust toolchain (`rustc`/`cargo`) for cargo and std diagnostics; the server itself starts without rustup.
- ShellCheck is GPL-3.0; its license ships in `Resources/lsp/licenses/` and the rest of the inventory is in `Resources/lsp/THIRD_PARTY.md`.
- Nested Mach-O binaries (`node`, `shellcheck`, `rust-analyzer`) are codesigned before the bundle so hardened-runtime and notarized builds pass Gatekeeper.

## Layout

```
crates/
  ui              # GPUI app (package name: halo) — desktop shell + pages
  bridge          # Typed UI ↔ backend API, reactive state, NativeBackend / StubBackend
  backend         # Runtime facade + commands/* shared with halo-cli
  cli             # halo-cli — headless commands over halo-backend::commands
  core            # Paths, FS security, event bus, shared types
  storage         # SQLite + payload store + history bridge
  traffic         # MITM proxy (HTTP/1.1 & HTTP/2), CA, interception hooks
  coder           # Editor persistence, PTY, backups
  agent           # Coding-agent harness orchestration
  git             # Source-control git helpers
  harness         # Pi RPC + external agent integrations
  lsp             # Stdio LSP client for Coder
  terminal-ui     # Reusable GPUI terminal surface
  crawler         # Spider runner + AI browser automation
  debugger        # In-app DevTools panels
  browser-tools   # Agent browser snapshot / act / read
  desktop-tools   # Computer-use tools over accessibility
  intercept       # Intercept engine + managed Chrome / CA trust
  repeater        # Repeater / Forge request runner
  intruder        # Fuzz attack engine
  mock-forge      # Local mock server engine
  port-scanner    # TCP connect-scan engine
  maintenance     # Cache cleanup / log rotation
  collaborator    # Out-of-band collaborator client
  mitm-proxy      # Low-level HTTPS MITM (hexbuffer-proxy)
  spider          # celestia-spider crawler engines
  cutlass/        # Vendored media stack (see licenses/CUTLASS-NOTICE)
```

Architecture deep-dives and reference mappings live under **[docs/](docs/README.md)**; CLI reference in **[docs/cli.md](docs/cli.md)**.
