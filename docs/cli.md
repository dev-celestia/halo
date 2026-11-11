# halo-cli — headless Halo

`halo-cli` drives Halo's backend from a terminal. It links no GPUI and opens no
window: it boots the same `BackendRuntime` the desktop app uses, so it reads and
writes the same `halo.db` and can run while the app is closed.

```
crates/cli            argument parsing + table/JSON output only
crates/backend/src/commands   the actual command bodies
```

Every command is a function in `halo_backend::commands::<group>` taking
`&BackendRuntime`. The desktop app reaches the same functions through
`halo_bridge::NativeBackend`. Adding a CLI command means adding it to the
command layer first — the binary stays a thin parser.

## Build

```bash
make cli                 # cargo build -p halo-cli
make cli-install         # …and copy to ~/.local/bin
```

## Global flag

`--json` — emit machine-readable JSON instead of a table. Applies to any
subcommand and is safe to pipe into `jq`.

## Commands

### proxy

```bash
halo-cli proxy start                    # foreground, blocks until Ctrl-C
halo-cli proxy start --port 8888 --tls-port 8443
halo-cli proxy start --detach           # background, returns when listening
halo-cli proxy stop                     # SIGTERM the detached proxy
halo-cli proxy status [--json]
```

`--detach` re-execs the binary as a hidden `serve` child in its own session, so
it survives the shell that started it. Where it landed is recorded in
`<app data>/cli/proxy.json`; `status` and `stop` read that file. If the
requested port is taken, the proxy binds the next free one and reports the port
it actually got.

The process must stay alive while the proxy runs — the listener lives in a
thread with process-global state. Use `--detach` for scripts, or run the
foreground form under `tmux`.

### history

```bash
halo-cli history ls                          # newest 30
halo-cli history ls -n 100 -m GET,POST
halo-cli history ls -s 4xx,5xx               # status classes, ranges, exact codes
halo-cli history ls -H api.example.com -q token
halo-cli history show <id>                   # request + response, headers, body
halo-cli history rm <id>
halo-cli history clear --all
halo-cli history clear --before week         # today | week | month | custom
halo-cli history clear --before custom --date 2026-10-01
```

### session

```bash
halo-cli session ls
halo-cli session new "Acme pentest"
halo-cli session use <id>
halo-cli session clear <id>
```

### repeater

```bash
halo-cli repeater send --url https://target/api -m POST -H "Content-Type: application/json" -d '{"a":1}'
halo-cli repeater send --raw request.txt
```

`--raw` parses `METHOD URL HTTP/1.1`, headers, a blank line, then the body.

### scan

```bash
halo-cli scan 10.0.0.1 --ports 1-1024
halo-cli scan example.com -p 22,80,443,8000-8100 --timeout 300 --concurrency 200 --banner
```

TCP connect scan only (`connect` is the only implemented scan type upstream).

### cert

```bash
halo-cli cert show
halo-cli cert export ~/Desktop/halo-ca.pem
halo-cli cert trust
halo-cli cert regenerate
```

Clients that go through the proxy must trust this CA; `curl` needs
`--cacert <pem>` or `-k`.

### library

```bash
halo-cli library collection
halo-cli library stash
halo-cli library endpoint
halo-cli library context
halo-cli library chronicle --limit 100
```

### notes

```bash
halo-cli notes ls
halo-cli notes rm <id>
halo-cli notes import notes.json      # JSON array, same shape as `notes ls --json`
```

### storage

```bash
halo-cli storage info
halo-cli storage prune database
halo-cli storage prune regression_artifacts
halo-cli storage prune log_file
halo-cli storage reset          # stops the proxy first
```

## Not wired yet

These backend groups have command bodies but no CLI subcommand: intercept,
intruder, crawler, mock forge, coder filesystem and git. `pty` and `agent` are
deliberately out of scope — they need a streaming transport, not one-shot
commands.

## Concurrency

SQLite runs in WAL mode with a 5 s busy timeout, so a CLI read while the app is
running is safe. Two processes cannot both hold the proxy port — the second
falls back to the next free port, and `status` reports which one it got.
