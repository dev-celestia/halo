# Hexbuffer — Security Audit

**Date:** 2026-10-21
**Scope:** `crates/hexbuffer`, `crates/hexbuffer-bridge`, `crates/hexbuffer-backend`, plus the
`hexbuffer-proxy` 1.0.1 dependency and the macOS bundling script.
**Method:** Manual source review of all trust boundaries (MITM proxy, TLS/CA handling, process
spawning, filesystem APIs, SQLite persistence, network binds), plus verification of live on-disk
state on the build host. Dependency versions reviewed from `Cargo.lock` (975 crates).

---

## 1. Executive summary

Hexbuffer is a local pentest/API-inspection tool that (a) generates a **trusted root CA**, (b) runs a
**TLS-intercepting forward proxy**, and (c) **stores all captured traffic** including cookies and
`Authorization` headers. That combination means a single weakness in key handling, bind address, or
storage permissions escalates directly into *"attacker can read the user's TLS sessions"*.

The core protocol code is in decent shape — SQL is parameterized, no shell interpolation, no
`danger_accept_invalid_certs`, and header values are re-validated by `hyper` before forwarding. The
weaknesses are in the **operational envelope around** that code: file modes, bind address,
privileged/destructive side effects, and unbounded queues.

| # | Severity | Finding |
|---|---|---|
| C-1 | **Critical** | Root CA **private key stored world-readable (0644)** |
| H-1 | **High** | Proxy binds **0.0.0.0** with no auth — open LAN proxy + localhost SSRF |
| H-2 | **High** | Captured traffic DB/payloads unencrypted **and world-readable (0644)** |
| H-3 | **High** | Starting the proxy **SIGTERM/SIGKILLs** whatever owns the port, hardcoded `reuse: true` |
| H-4 | **High** | Intercept queue unbounded, paused requests **never time out** (memory/hang DoS) |
| M-1 | Medium | Proxy-record channel is `unbounded_channel` despite a "bounded" comment |
| M-2 | Medium | Log path `/tmp/hexbuffer.log` — predictable, world-writable, truncate-follows-symlink |
| M-3 | Medium | CA "validity" check is a substring match; corrupt CA ⇒ startup **panic** |
| M-4 | Medium | Coder filesystem API has **no root confinement**; recursive delete on any absolute path |
| M-5 | Medium | `export_ca_cert_to` creates directories / writes at any attacker-supplied path |
| M-6 | Medium | Proxy injects `x-rusxy` into every request and strips cache validators |
| M-7 | Medium | Ad-hoc codesign, no hardened runtime, no notarization for a CA-installing app |
| L-1 | Low | `is_proxy_enabled()` fails **open** when no handle is registered |
| L-2 | Low | `repeater_send` blocks the **UI thread** with a no-timeout HTTP client |
| L-3 | Low | `payload_store` joins `session_id` into paths without UUID validation |
| L-4 | Low | `PRAGMA`/`ALTER TABLE` built with `format!` (constants today, footgun tomorrow) |
| L-5 | Low | Cross-app keychain deletion: removes *other products'* CA certs by name |
| L-6 | Low | Port bind race: probe-bind, `drop(listener)`, re-bind in the crate |

**Nothing here requires a malicious website to exploit.** Every high/critical issue is reachable by
a **local unprivileged process** (or, for H-1, by **any host on the same LAN**) — which is exactly the
threat model a developer machine running a pentest tool lives in.

### 1.1 Remediation status (applied 2026-10-21)

All findings below have been fixed in the working tree. The descriptions in §3 record the state at
audit time; this table records what changed.

| # | Status | Change |
|---|---|---|
| C-1 | **Fixed** | New `fs_secure` module; CA dir created `0700` before the key exists, key forced `0600`, re-asserted on every boot (`proxy/ca.rs`, `paths.rs`, `lib.rs`). |
| H-1 | **Fixed** | `ProxyConfig.bind_addr` defaults to `127.0.0.1`; network exposure requires `HEXBUFFER_PROXY_BIND`, which logs a warning (`proxy/mod.rs`). |
| H-2 | **Fixed** | App dir `0700`, DB/WAL/SHM/payload segments `0600`, healed at boot (`paths.rs`, `payload_store.rs`, `fs_secure.rs`). Encryption at rest is still open — see §5. |
| H-3 | **Fixed** | `ensure_port_free` never signals anything by default; opt-in path is same-UID, `SIGTERM`-only, no `SIGKILL`, and reports PIDs instead (`proxy/utils.rs`). |
| H-4 | **Fixed** | `MAX_PAUSED_REQUESTS = 200` with fail-open forwarding; `PAUSE_TIMEOUT = 120s` auto-forwards unchanged (`proxy/state.rs`, `proxy/lifecycle.rs`). |
| M-1 | **Fixed** | `mpsc::channel(1024)` + `try_send`; overflow drops the record and counts it (`proxy/completion.rs`). |
| M-2 | **Fixed** | Log moved to `<app dir>/logs/hexbuffer.log`, opened `0600` with `O_NOFOLLOW` (`commands/storage.rs`). |
| M-3 | **Fixed** | Substring check replaced with PEM/DER validation; load wrapped in `catch_unwind` so a corrupt CA regenerates instead of panicking (`proxy/ca.rs`). |
| M-4 | **Fixed** | Coder FS API confined to the open project via canonicalized prefix checks, plus a protected-name deny list and a trash-only `purge` (`commands/coder.rs`). |
| M-5 | **Fixed** | `export_ca_cert_to` no longer creates directories; the parent must exist (`commands/cert.rs`). |
| M-6 | **Fixed** | `x-rusxy` injection removed (nothing read it); cache-validator stripping is now opt-in, default off (`proxy/lifecycle.rs`, `proxy/state.rs`). |
| M-7 | **Fixed** | Hardened-runtime signing path via `CODESIGN_IDENTITY` + `scripts/Hexbuffer.entitlements`; ad-hoc builds print a distribution warning; README no longer recommends `xattr -cr`. |
| L-1 | **Fixed** | `is_proxy_enabled()` returns `false` when no flag is registered. |
| L-2 | **Fixed** | Repeater client gets 10 s connect / 60 s total timeouts. |
| L-3 | **Fixed** | `safe_session_component` rejects non-identifier session ids before path joins. |
| L-4 | **Fixed** | `ensure_column` validates table/column/type against a character allowlist. |
| L-5 | **Fixed** | Keychain cleanup only deletes certificates whose SHA-256 matches our CA. |
| L-6 | Documented | Inherent to the dependency's builder API; comment added, impact is a failed start. |

Verified by `cargo test -p hexbuffer-backend` (27 tests, including new guards for the CA key mode, the
loopback default, and symlink-refusing truncation) and `cargo check` on the backend and bridge.

**One manual step remains on the machine this was audited on:** `~/.hexbuffer` and `ca-key.pem` are
owned by `root`, so the boot-time `chmod` cannot take effect (it fails with `EPERM`, silently). Fix
ownership once:

```bash
sudo chown -R "$(whoami)" ~/Library/Application\ Support/com.hexbuffer/.hexbuffer
chmod 700 ~/Library/Application\ Support/com.hexbuffer/.hexbuffer
chmod 600 ~/Library/Application\ Support/com.hexbuffer/.hexbuffer/ca-key.pem
```

---

## 2. Threat model

| Actor | Capability assumed | Realistic path |
|---|---|---|
| Local unprivileged user / process | read `~/Library/Application Support`, spawn processes | C-1, H-2 |
| Malware running as the user | same, plus code exec | C-1 → mint certs → full TLS MITM |
| LAN peer | reach `0.0.0.0:8888` | H-1 → open relay, localhost SSRF |
| Malicious web page / proxied site | generate arbitrary requests through the proxy | H-4 → OOM, wedge |
| Malicious repository opened in Coder | plant files, prompt the coding agent | M-4 → arbitrary file delete/write |

Out of scope: OS-level compromise, supply chain of the Rust toolchain, physical access.

---

## 3. Findings

### C-1 — Root CA private key is world-readable (Critical)

**Where**
- `crates/hexbuffer-backend/src/proxy/ca.rs:30-35` (`create_proxy_authority`), `:45-54`
  (`regenerate_ca`), `:80-114` (`ensure_ca_exists`)
- Root cause in the dependency: `hexbuffer-proxy-1.0.1/src/ca.rs:117-121`

```rust
std::fs::create_dir_all(cert_dir).expect("Failed to create cert directory");
std::fs::write(&cert_path, &ca_cert_pem).expect("Failed to save CA certificate");
std::fs::write(&key_path, ca_key.serialize_pem()).expect("Failed to save CA key");
```

`fs::write` creates files with mode `0666 & ~umask` → **0644**. Nothing in the Hexbuffer wrapper
tightens the mode afterwards.

**Evidence (verified on this machine)**

```
-rw-r--r--  1 root  staff  241  ca-key.pem      # ~/Library/Application Support/com.hexbuffer/.hexbuffer/
-rw-r--r--  1 root  staff  583  ca.pem
```

**Impact**
Any local account or process that can read that directory obtains the signing key for a CA that the
user has installed as a **trusted root**. With it an attacker mints `*.bank.example` /
`*.google.com` leaf certificates and transparently MITMs every TLS session on the machine — including
sessions that never touch Hexbuffer. This is the single most damaging issue in the codebase, and it is
invisible to the user because everything keeps working.

**Fix**
1. Create the CA directory `0700` and the key `0600` **before** writing, and re-assert the mode on
   every startup (existing installs are already 0644 and will never self-heal otherwise):

```rust
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

fn harden_ca_perms(ca_dir: &Path) {
    let _ = fs::set_permissions(ca_dir, fs::Permissions::from_mode(0o700));
    let key = ca_dir.join("ca-key.pem");
    if key.exists() {
        let _ = fs::set_permissions(&key, fs::Permissions::from_mode(0o600));
    }
}
```

2. Call it from `create_proxy_authority()` *and* `ensure_ca_exists()` (so upgrades are fixed).
3. Better: stop persisting the CA key as PEM at all — store it in the macOS Keychain
   (`SecKeyCreateRandomKey` / `security add-generic-password`) and keep only the cert on disk.
4. On Windows, the equivalent is an ACL restricted to the current SID; `std::fs` gives you nothing
   there — the file inherits `%APPDATA%` inheritance, which is per-user but includes `Administrators`.

---

### H-1 — Proxy binds `0.0.0.0` with no authentication (High)

**Where** `crates/hexbuffer-backend/src/proxy/mod.rs:113` and `:248`

```rust
runtime.block_on(TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)))
...
let socket_addr: std::net::SocketAddr = (Ipv4Addr::UNSPECIFIED, port).into();
```

**Impact** — three distinct problems from one line:

1. **Open forward proxy on the LAN.** Anyone on the coffee-shop / office network can point their
   browser at `http://victim-ip:8888` and relay traffic through the user's machine. Abuse is
   attributed to the victim's IP; there is no auth, no ACL, no rate limit.
2. **Localhost SSRF.** A proxy `CONNECT 127.0.0.1:5432` or `GET http://127.0.0.1:9200/_cat/indices`
   reaches services that are firewalled from the network but trusted from localhost (Elasticsearch,
   Redis admin UIs, Docker API, dev servers, Prometheus). The proxy happily tunnels them.
3. **Internal network reconnaissance** from a host that is inside the perimeter.

**Fix** Bind loopback by default; make LAN exposure an explicit, warned opt-in with a shared secret:

```rust
let bind_ip = if config.expose_to_lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
```

and, when exposed, require `Proxy-Authorization` (constant-time compare) plus a Host allowlist that
refuses `127.0.0.0/8`, `::1`, `169.254.0.0/16`, and RFC1918 targets unless the operator opted in.

---

### H-2 — Captured traffic is unencrypted and world-readable (High)

**Where**
- `crates/hexbuffer-backend/src/paths.rs:8-44` — `create_dir_all` with default 0755
- `crates/hexbuffer-backend/src/db/payload_store.rs:177` — segment files via `OpenOptions`
- `crates/hexbuffer-backend/src/db/schema.rs:199` — `api_key TEXT NOT NULL` (collaborator servers)

**Evidence**

```
-rw-r--r--  hexbuffer.db          # 671 KB, every captured request/response
-rw-r--r--  hexbuffer.db-wal      # 4.3 MB of the most recent traffic
drwxr-xr-x  sessions/             # raw payload segments
```

The database is plain SQLite holding request/response headers and bodies: session cookies,
`Authorization: Bearer …`, password-reset tokens, API keys, and full request bodies. Because the file
is 0644, **any** local account can read it, and so can any backup/sync agent that runs as a different
user. `collaborator_servers.api_key` is additionally stored in cleartext as a column.

**Impact** A pentest tool that captures production credentials becomes the largest credential store
on the machine, protected by nothing but a directory listing.

**Fix**
1. `0700` on the app data dir, `0600` on DB, `-wal`, `-shm`, and every payload segment
   (`OpenOptions::new().mode(0o600)`), asserted at boot for existing installs.
2. Encrypt at rest: SQLCipher (`rusqlite` feature `bundled-sqlcipher`) with a key from the Keychain;
   payload segments sealed with AES-GCM using the same key.
3. Encrypt or move collaborator `api_key` out of the DB.
4. Add a "clear history on quit" / retention window, and redact `authorization`, `cookie`,
   `set-cookie` by default in the stored copy (viewable on demand, not persisted).

---

### H-3 — Starting the proxy kills whatever owns the port (High)

**Where**
- `crates/hexbuffer-backend/src/proxy/utils.rs:19-107` — `ensure_port_free(port, reuse)`
- Callers hardcode `reuse: true`: `crates/hexbuffer-backend/src/commands/proxy.rs:32`,
  `crates/hexbuffer-bridge/src/native.rs:398`, `crates/hexbuffer-backend/src/proxy/mod.rs:87`

```rust
println!("Killing process {} on port {}", pid, port);
Command::new("kill").arg(&pid)...
...
Command::new("kill").args(["-9", pid])...
```

The port is user-editable in Settings. Point it at `5432`, `3306`, `3000`, or `22`, press Start, and
Hexbuffer `SIGTERM`s then `SIGKILL`s the owner — a local Postgres, a dev server, or `sshd` on a
non-standard port. The error text even says *"Use --reuse to auto-kill it"*, implying it is an opt-in
flag, but the UI path always passes `true`.

**Impact** Unannounced destruction of unrelated state: a killed database mid-transaction, a killed
build, a killed remote shell. Silent data loss with no audit trail.

**Fix**
- Default `reuse: false`; on conflict, return the PIDs + command lines to the UI.
- Require an explicit confirmation that **lists** `pid`, executable path, and user before signalling.
- Drop the `-9` escalation (or gate it behind a second confirmation) — never `SIGKILL` a process you
  do not own.
- Never kill when the owner is another UID (compare `lsof` user field); report instead.

---

### H-4 — Unbounded intercept queue and no timeout on paused requests (High)

**Where**
- `crates/hexbuffer-backend/src/proxy/state.rs:68-70` — `paused_requests.push(req)` (no cap)
- `crates/hexbuffer-backend/src/proxy/lifecycle.rs:291-296` and `:529-534`

```rust
loop {
    tokio::time::sleep(Duration::from_millis(100)).await;
    if proxy_state.get_paused_request(&paused_id).is_none() { break; }
}
```

There is no deadline and no cap. Each paused entry retains up to `MAX_RECORDED_BODY_BYTES`
(32 MiB, `lifecycle.rs:25`) of request body, plus the response body for response-intercepts.

**Impact** With Intercept enabled and a scope of `*` (the default tab scope), a single page load that
fires a few hundred XHRs — or an attacker on the LAN via H-1 — queues hundreds of MB and holds one
Tokio task + one client connection per paused request. The UI never drains them fast enough, memory
climbs to OOM, and the proxy wedges. This is a **remote, unauthenticated** DoS given H-1.

**Fix**
- Cap `paused_requests` (e.g. 200); when full, auto-forward (fail-open) and surface a toast.
- Add a per-request deadline (e.g. 120 s) that auto-forwards with a logged warning.
- Replace the 100 ms poll with a `Notify`/`oneshot` per paused id — polling every paused request every
  100 ms is also a CPU cost that scales with the queue.
- Cap the body retained for *interception* separately from the body retained for *recording*.

---

### M-1 — Proxy record channel is unbounded (Medium)

`crates/hexbuffer-backend/src/proxy/completion.rs:9-14`

```rust
static LOG_SENDER: OnceLock<mpsc::UnboundedSender<(ProxyRecord, Option<String>)>> = OnceLock::new();
```

The comment in `lifecycle.rs:22-25` claims the "unbounded log channel … stay[s] bounded" because
bodies are capped at 32 MiB. Capping each item does not bound the queue: 500 items × 32 MiB is still
16 GB. A traffic burst with a slow SQLite writer (large WAL, fsync stall) grows the channel without
limit.

**Fix** `mpsc::channel(1024)` with `try_send`; on `Full`, drop the record and increment a
"records dropped" counter shown in the status bar (recording is best-effort, the proxy must not block).

---

### M-2 — `/tmp/hexbuffer.log` is a predictable, world-writable path (Medium)

`crates/hexbuffer-backend/src/commands/storage.rs:65`, `:151-159`

```rust
let _ = fs::write("/tmp/hexbuffer.log", "");
```

Two problems:

1. **Symlink/truncate attack.** On a multi-user machine another user can pre-create
   `/tmp/hexbuffer.log` as a symlink to a file the victim owns; the "delete log" action then truncates
   that file (`fs::write` with an empty buffer follows symlinks and truncates). On Linux with
   `fs.protected_symlinks=0` (or any sticky-bit-unaware path) this is a classic local DoS/data-loss.
2. **Information disclosure.** Any log written there inherits `/tmp` semantics — readable by other
   users, and preserved across reboots on some systems. Traffic-derived log lines are sensitive.

**Fix** Move to `get_shared_app_dir()/logs/hexbuffer.log`, open with
`OpenOptions::new().append(true).create(true).mode(0o600)` and `O_NOFOLLOW` (`custom_flags(libc::O_NOFOLLOW)`).
Rotate instead of truncating.

---

### M-3 — CA validation is a substring check; corrupt CA panics at startup (Medium)

`crates/hexbuffer-backend/src/proxy/ca.rs:56-78` and `:90-94`

```rust
if cert_pem.contains("Hexbuffer Proxy CA") { return true; }   // before any parsing
...
key_content.contains("PRIVATE KEY")
```

and in the dependency, `new_in` uses `.expect("Failed to read CA certificate PEM")` /
`.expect("Failed to parse CA key from PEM")`.

**Impact**
- A file containing the literal string `Hexbuffer Proxy CA` passes validation, then panics inside the
  crate → the app cannot start the proxy (or crashes at boot) with a stack trace instead of
  regenerating.
- Cert/key mismatch is never detected: a valid cert with a stale key loads "fine" and then every
  forged leaf fails to verify in the browser, producing confusing TLS errors rather than a clear
  "regenerate your CA" path.

**Fix** Parse with `rcgen`/`x509-parser`, check `BasicConstraints CA:TRUE`, `KeyUsage keyCertSign`,
the validity window, and that `SubjectPublicKeyInfo` of the cert matches the loaded key. On any
failure, rename the pair to `ca.pem.broken-<ts>` and regenerate. Replace `.expect()` in the vendored
crate with `Result`.

---

### M-4 — Coder filesystem API has no root confinement (Medium)

`crates/hexbuffer-backend/src/commands/coder.rs:114-236`, exposed through
`crates/hexbuffer-bridge/src/native.rs:973-1012`

Every operation takes an absolute path string and acts on it directly:

```rust
pub fn delete_path(path: &str) -> Result<(), String> {
    if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}
pub fn purge_trash_entry(trash_path: &str) -> Result<(), String> { ... delete_path(trash_path) ... }
```

No canonicalization against the open workspace root, no deny-list, no confirmation, and
`purge_trash_entry` will delete **any** path handed to it (it only checks the *parent* name length is
36 before tidying up). `write_file` will `create_dir_all` + write anywhere.

**Impact** The Coder workspace is user-chosen and often an untrusted repository. A crafted repo plus a
plausible agent instruction ("clean up the stale files listed in SETUP.md") turns `delete_path` into
arbitrary recursive deletion under the user's home. Same primitive for arbitrary file overwrite.

**Fix**
- Resolve both the workspace root and the target with `canonicalize()`; reject when
  `!target.starts_with(root)` (compare after canonicalization, not string prefix — symlinks).
- Route every deletion through the OS trash, and require typed confirmation for directories.
- `purge_trash_entry` must verify the path is inside `coder-trash/<uuid>/`.
- Deny-list `.git`, `.ssh`, `~/Library`, `/etc`, `/` even inside the root.

---

### M-5 — `export_ca_cert_to` is an arbitrary-path writer (Medium)

`crates/hexbuffer-backend/src/commands/cert.rs:22-32`

```rust
if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
std::fs::write(&path, ca_pem)?;
```

The path arrives from the save dialog today, so exploitation needs a UI-path bug — but the function is
a general "create directories and write a file anywhere" primitive reachable from the bridge
(`native.rs:1183`). It also silently overwrites (`fs::write` truncates).

**Fix** Restrict to the dialog-returned path, refuse to overwrite without confirmation, don't
`create_dir_all` (the directory exists if the user picked it), and validate the extension is `.pem`/`.crt`.

---

### M-6 — Request tampering that leaks tool identity (Medium)

`crates/hexbuffer-backend/src/proxy/lifecycle.rs:141-142`, `:262-263`, `:353-354`, `:365-366`

Every forwarded request gets `x-rusxy: 1` injected, and `if-none-match` / `if-modified-since` are
stripped.

- **Fingerprinting:** any server, WAF, or anti-fraud system can trivially detect "this client is being
  intercepted by Hexbuffer" — useful to an adversary who wants to serve benign content to the
  researcher and malicious content to real users.
- **Correctness:** stripping cache validators changes response codes and bytes for everyone using the
  proxy, which can invalidate findings ("the endpoint returned 200 here but 304 in prod").
- `x-rusxy` can collide with an application's own header.

**Fix** Make the marker opt-in and configurable (or drop it — the proxy knows its own connections),
and only strip validators behind an explicit "disable caching" toggle, defaulted off.

---

### M-7 — Distribution: ad-hoc signature, no hardened runtime, no notarization (Medium)

`scripts/bundle-macos.sh:166-170`

```bash
codesign --force --deep --sign - "$APP_BUNDLE"
```

The README instructs users to `xattr -cr` when Gatekeeper complains. For a normal app that is a
papercut; for an app whose entire purpose is to install a **trusted root CA**, it trains users to
bypass exactly the control that would catch a trojanized build. `--deep` is also deprecated by Apple.

**Fix** Developer ID + hardened runtime + notarization for any distributed build; keep ad-hoc for
local dev only, and gate `trust_ca()` so it refuses to run in a non-notarized bundle without an
explicit warning.

---

### L-1 — `is_proxy_enabled()` fails open

`crates/hexbuffer-backend/src/proxy/mod.rs:59-67` returns `true` when no `AtomicBool` handle is
registered. If a future refactor makes this the gate for bypassing interception, the failure mode is
"intercept everything" rather than "intercept nothing". Return `false`, or make it `Option<bool>` and
force the caller to decide.

### L-2 — `repeater_send` blocks the UI thread with no timeout

`crates/hexbuffer-backend/src/commands/repeater.rs:54-57` builds a `reqwest::Client` with no
`.timeout()`; `crates/hexbuffer-bridge/src/native.rs:760-762` calls it via `tokio.block_on` on the
UI-facing path. A server that accepts and never responds freezes the app. Add
`.timeout(Duration::from_secs(30))` and `.connect_timeout(...)`.

### L-3 — `session_id` used as a path component without validation

`crates/hexbuffer-backend/src/db/payload_store.rs:261`, `:275`, `:314`
(`sessions_dir.join(sid)`). IDs are UUIDs today, so this is not exploitable now — but a crafted or
imported DB row with `sid = "../../.."` escapes the directory. Validate with `Uuid::parse_str` and
reject otherwise; cheap insurance.

### L-4 — Identifier interpolation in DDL

`crates/hexbuffer-backend/src/db/repository/mod.rs:231-245` builds
`PRAGMA table_info({table_name})` / `ALTER TABLE {} ADD COLUMN {} {}` with `format!`. All current
callers pass literals, so there is no injection — but the helper is a loaded gun. Assert
`table_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')` at the top of `ensure_column`.

### L-5 — Cross-app keychain deletion

`crates/hexbuffer-backend/src/commands/cert.rs:90-110` and
`crates/hexbuffer-backend/src/commands/intercept.rs:480-501` delete certificates by name from the
user's login keychain in a loop, including `"Apprecon Proxy CA"` and
`"Apprecon Security Tools Root CA"` — another product's trust anchors. Clicking "Trust CA" silently
untrusts a different tool. Only delete certs whose public key matches *this* CA.

### L-6 — Port bind race

`crates/hexbuffer-backend/src/proxy/mod.rs:237-245` binds a listener to test the port, then
`drop(listener)`, then `hexbuffer-proxy` binds again inside `proxy.start()`. Another process can take
the port in between, producing a confusing "started" then immediate stop. Pass the bound listener into
the builder instead of re-binding.

---

## 4. What is already done right

Worth recording so a future refactor doesn't regress it:

- **No shell interpolation.** Every spawn (`git`, `security`, `certutil`, `kill`, `lsof`, `claude`) uses
  `Command::new(..).args([..])` — no `sh -c`, no string concatenation into a shell. This is the single
  most common RCE source in desktop apps and it is absent here.
- **SQL is parameterized.** `proxy_logs.rs`, `notes.rs`, `collaborator.rs`, `websocket.rs` all use `?n`
  bindings; `notes.rs:97` builds `IN (?,?,…)` placeholders, not values.
- **Header injection is blocked by construction.** Intercept edits are re-parsed into `HeaderName` /
  `HeaderValue` (`lifecycle.rs:329-335`, `:555-561`), so CRLF smuggling into the upstream request is
  rejected by `hyper`, not by a hand-rolled filter.
- **Upstream TLS is verified.** No `danger_accept_invalid_certs` anywhere in the workspace; WebSocket
  upstreams use `webpki_roots` (`ws_proxy.rs:213-221`).
- **Body size caps exist** on both the response buffer (64 MiB, with a streamed fallthrough and a 502
  on chunked overflow) and the recorded body (32 MiB). The caps are well-reasoned; only the *queues*
  around them are unbounded.
- **Dependency versions are current** (`rustls 0.23.46`, `time 0.3.55`, `hyper 1.12`, `reqwest 0.12.28`,
  `rusqlite 0.40.2`); no obviously vulnerable crate in the lock file. Run `cargo audit` / `cargo deny`
  in CI to keep it that way.
- **No IPC socket.** The UI↔backend bridge is in-process typed calls, not a localhost HTTP/WebSocket
  control channel. That removes an entire class of "any local process can drive the pentest tool"
  bugs that comparable Tauri-based tools have.

---

## 5. Remaining work

The code-level findings are fixed (§1.1). What is left is design work, not patches:

1. **Encryption at rest.** File modes stop *other users*; they do not stop a backup agent, a sync
   client, or malware running as the same user. The traffic store is the most sensitive artefact this
   app produces. Move to SQLCipher (`rusqlite` feature `bundled-sqlcipher`) with the key in the
   Keychain, and seal payload segments with AES-GCM under the same key.
2. **Redaction on write.** Persisting `cookie`, `set-cookie` and `authorization` in cleartext is a
   deliberate product choice, but the default should be "store a redacted copy, reveal on demand",
   not "store everything forever". Pair it with a retention window.
3. **LAN exposure as a real feature.** H-1 now defaults to loopback and can only be widened with an
   env var. If remote-device interception is wanted, it needs a UI toggle, a per-session bearer token,
   and a target ACL that refuses loopback/link-local/RFC1918 by default.
4. **Windows ACLs.** `fs_secure::set_mode` is a no-op on Windows; `%APPDATA%` inheritance includes
   `Administrators`. Needs `SetNamedSecurityInfo` or a DPAPI-wrapped key file.
5. **`Notify` instead of polling.** The pause loop still polls every 100 ms. The cap and deadline
   bound the damage, but a `tokio::sync::Notify` per paused id removes the wakeup cost entirely.
6. **CI gates.** Add `cargo audit` / `cargo deny` and a test that asserts no `0o644` CA key is ever
   produced (the unit test added today covers the happy path; CI should also cover the upgrade path
   from an existing 0644 install).
7. **Coder agent confinement.** The filesystem API is confined now, but the Claude Code CLI runs in
   that directory with the user's full rights. If agent output is ever treated as instructions rather
   than text, the workspace is the blast radius — consider a sandbox profile (`sandbox-exec` on
   macOS) for agent runs.

---

## 6. Appendix — reproduction notes

```bash
# C-1: CA key mode
stat -f "%Sp %N" "$HOME/Library/Application Support/com.hexbuffer/.hexbuffer/"*

# H-2: traffic store mode
stat -f "%Sp %z %N" "$HOME/Library/Application Support/com.hexbuffer/hexbuffer.db"

# H-1: confirm the bind address while the proxy is running
lsof -nP -iTCP:8888 -sTCP:LISTEN
#   expect  *:8888   (i.e. 0.0.0.0) rather than 127.0.0.1:8888

# H-1 impact: reach a loopback-only service from another host
curl -x http://<victim-ip>:8888 http://127.0.0.1:9200/
```
